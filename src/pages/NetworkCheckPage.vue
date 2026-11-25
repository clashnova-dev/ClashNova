<script setup lang="ts">
import { computed, ref, watch, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { CircleCheck, Crosshair, Gauge, Globe, Network, Power, ExternalLink, ShieldAlert, ShieldCheck, Wifi, Wrench } from "@lucide/vue";
import HelpTip from "../components/ui/HelpTip.vue";
import ToggleSwitch from "../components/ui/ToggleSwitch.vue";
import UnlockCheck from "../components/UnlockCheck.vue";
import SpeedTestDialog from "../components/SpeedTestDialog.vue";
import { useApp } from "../composables/useApp";
import { ipv6Assessment, webrtcAssessment, type PrivacyTone, type RtcObservation } from '../lib/privacy-check';
import { collectWebrtc } from "../lib/webrtc-check";
import { confirmation } from "../lib/confirmation";
import type { MessageKey, Text } from "../i18n";

const { t, tx, isNative, setTunProtection, repairNetwork, networkError, networkSettings, connected, guardArmed, busy, networkLoaded, connectionMode, setConnectionMode, selectedNode, routeMode, phase, tab, openExternal } = useApp();
const speedDialog = ref<InstanceType<typeof SpeedTestDialog>>();
const modeOptions = computed(() => [
  { value: 'system' as const, label: t('common.systemProxy') },
  { value: 'tun' as const, label: 'TUN' },
  { value: 'manual' as const, label: t('settings.manualProxy') },
]);
const resolverName = computed(() => networkSettings.value.tun.dnsDomestic === 'dnspod' ? 'DNSPod DoH' : 'AliDNS DoH');
const remoteName = computed(() => ({cloudflare: 'Cloudflare DoH', google: 'Google DoH', quad9: 'Quad9 DoH'})[networkSettings.value.tun.dnsRemote]);
async function changeMode(value: 'system' | 'tun' | 'manual') {
  const previous = connectionMode.value;
  if (busy.value || value === previous) return;
  if (!await confirmation.request({ title: 'confirmation.proxyTitle', body: 'confirmation.proxyBody' })) return;
  if (!busy.value && connectionMode.value === previous) await setConnectionMode(value);
}

// 网络修复:打开即自动检测,逐项给出状态,支持单项修复与一键修复。
type RepairKey = "connection" | "systemProxy" | "dns";
type RepairStatus = "checking" | "ok" | "problem" | "fixing" | "fixed" | "failed";
const REPAIR_KEYS: RepairKey[] = ["connection", "systemProxy", "dns"];
const REPAIR_LABEL: Record<RepairKey, MessageKey> = { connection: "secnet.itemConnection", systemProxy: "secnet.itemProxy", dns: "secnet.itemDns" };
const REPAIR_STATUS_LABEL: Record<RepairStatus, MessageKey> = { checking: "secnet.statusChecking", ok: "secnet.statusOk", problem: "secnet.statusProblem", fixing: "secnet.statusFixing", fixed: "secnet.statusFixed", failed: "secnet.statusFailed" };
const repairDialog = ref<HTMLDialogElement | null>(null);
const repairChecks = ref<Record<RepairKey, RepairStatus>>({ connection: "checking", systemProxy: "checking", dns: "checking" });
const repairBusy = ref(false);
const repairUnavailable = ref(false);
const problemKeys = computed(() => REPAIR_KEYS.filter((key) => repairChecks.value[key] === "problem"));
const detecting = computed(() => REPAIR_KEYS.some((key) => repairChecks.value[key] === "checking"));

function evaluateNative(native: Native): Record<RepairKey, "ok" | "problem"> {
  const proxyHealthy = native.proxy === "ok";
  return {
    connection: native.connected ? "ok" : "problem",
    systemProxy: proxyHealthy ? "ok" : "problem",
    dns: native.dnsOk ? "ok" : "problem",
  };
}
async function detectRepairs() {
  if (!isNative) { repairUnavailable.value = true; return; }
  repairUnavailable.value = false;
  for (const key of REPAIR_KEYS) repairChecks.value[key] = "checking";
  try {
    const result = evaluateNative(await invoke<Native>("network_check"));
    for (const key of REPAIR_KEYS) repairChecks.value[key] = result[key];
  } catch { for (const key of REPAIR_KEYS) repairChecks.value[key] = "failed"; }
}
// 实际修复动作:重新同步系统代理(重新应用当前连接模式),再按项复检。
async function runRepair(keys: RepairKey[]) {
  if (repairBusy.value || busy.value || !isNative || !keys.length) return;
  repairBusy.value = true;
  for (const key of keys) repairChecks.value[key] = "fixing";
  try {
    closeRepair();
    const repaired = await repairNetwork();
    repairDialog.value?.showModal();
    if (!repaired) { await detectRepairs(); return; }
    const result = evaluateNative(await invoke<Native>("network_check"));
    for (const key of REPAIR_KEYS) {
      if (keys.includes(key)) repairChecks.value[key] = result[key] === "ok" ? "fixed" : "failed";
      else if (repairChecks.value[key] !== "fixed") repairChecks.value[key] = result[key];
    }
  } catch { for (const key of keys) repairChecks.value[key] = "failed"; }
  finally { repairBusy.value = false; }
}
function openRepair() { repairDialog.value?.showModal(); void detectRepairs(); }
function closeRepair() { repairDialog.value?.close(); }

type Native = {
  localOk: boolean;
  proxy: string;
  dnsOk: boolean;
  exitIp: string;
  directIp?: string;
  ipv6Ip: string;
  proxyIpv6Ip?: string;
  connected: boolean;
  mode: string;
  tunIpv6: boolean;
};
type Tone = PrivacyTone;
type Rtc = RtcObservation;

const status = ref<"idle" | "running" | "done">("idle");
const ran = ref(false);
let generation = 0;
let cancelRtc: (() => void) | undefined;
onUnmounted(() => { generation++; cancelRtc?.(); });
const privacyBusy = ref(false);
watch([phase, connectionMode, selectedNode, routeMode, networkSettings], () => {
  generation++;
  cancelRtc?.();
  resetResults();
  status.value = 'idle';
});
async function protectPrivacy() {
  if (busy.value || privacyBusy.value) return;
  privacyBusy.value = true;
  try {
    await changeMode('tun');
    if (connectionMode.value === 'tun' && connected.value) await run();
  } finally { privacyBusy.value = false; }
}
const error = ref("");
const localText = ref<Text>("check.pending");
const proxyText = ref<Text>("check.pending");
const dnsText = ref<Text>("check.pending");
const exitText = ref<Text>("check.pending");
const localTone = ref<Tone>("idle");
const proxyTone = ref<Tone>("idle");
const dnsTone = ref<Tone>("idle");
const exitTone = ref<Tone>("idle");
const dnsLeak = ref<Text>("check.notRun");
const ipv6Leak = ref<Text>("check.notRun");
const webrtcLeak = ref<Text>("check.notRun");
const dnsToneRow = ref<Tone>("idle");
const ipv6Tone = ref<Tone>("idle");
const webrtcTone = ref<Tone>("idle");

const checkedAt = ref('');
function resetResults() {
  ran.value = false;
  checkedAt.value = '';
  error.value = '';
  for (const value of [localText, proxyText, dnsText, exitText]) value.value = 'check.pending';
  for (const value of [dnsLeak, ipv6Leak, webrtcLeak]) value.value = 'check.notRun';
  for (const tone of [localTone, proxyTone, dnsTone, exitTone, dnsToneRow, ipv6Tone, webrtcTone]) tone.value = 'idle';
}
function gatherWebrtc(): Promise<Rtc> {
  cancelRtc?.();
  const check = collectWebrtc();
  cancelRtc = check.cancel;
  return check.result;
}

function applyWebrtc(rtc: Rtc, native?: Native) {
  const result = webrtcAssessment(rtc, native);
  webrtcLeak.value = result.text;
  webrtcTone.value = result.tone;
}

function applyNative(native: Native, rtc: Rtc) {
  localText.value = native.localOk ? "check.localOk" : "check.localFail";
  localTone.value = native.localOk ? "ok" : "bad";
  proxyText.value = native.proxy === "ok" ? "check.proxyOk" : native.proxy === "fail" ? "check.proxyFail" : "check.proxyOff";
  proxyTone.value = native.proxy === "ok" ? "ok" : native.proxy === "fail" ? "bad" : "idle";
  dnsText.value = native.dnsOk ? "check.dnsOk" : "check.dnsFail";
  dnsTone.value = native.dnsOk ? "ok" : "bad";
  exitText.value = native.exitIp || "check.exitFail";
  exitTone.value = native.exitIp ? "ok" : "bad";
  if (!native.connected) { dnsLeak.value = "check.dnsLeakNeedConnection"; dnsToneRow.value = "idle"; }
  else if (native.mode === "tun") { dnsLeak.value = "check.dnsLeakTun"; dnsToneRow.value = "warn"; }
  else { dnsLeak.value = "check.dnsLeakSystem"; dnsToneRow.value = "warn"; }
  const ipv6 = ipv6Assessment(native);
  ipv6Leak.value = ipv6.text;
  ipv6Tone.value = ipv6.tone;
  applyWebrtc(rtc, native);
}

async function run() {
  if (status.value === "running" || busy.value) return;
  const runId = ++generation;
  resetResults();
  status.value = "running";
  const [nativeResult, rtcResult] = await Promise.allSettled([
    isNative ? invoke<Native>("network_check") : Promise.reject(new Error("check.desktopOnly")),
    gatherWebrtc(),
  ]);
  if (runId !== generation) return;
  const rtc: Rtc = rtcResult.status === "fulfilled" ? rtcResult.value : { kind: "off" };
  ran.value = true;
  status.value = "done";
  checkedAt.value = new Date().toLocaleString();
  if (nativeResult.status === "fulfilled") {
    applyNative(nativeResult.value, rtc);
    return;
  }
  const reason = nativeResult.reason;
  error.value = reason instanceof Error ? reason.message : String(reason);
  applyWebrtc(rtc);
}
</script>

<template>
  <div class="check-stack">
  <section class="card secnet" :aria-label="t('secnet.title')">
    <header class="secnet-head">
      <span class="secnet-badge"><ShieldCheck :size="22" :stroke-width="1.9" /></span>
      <div class="secnet-headtext">
        <h2>{{ t("secnet.title") }}</h2>
        <p>{{ t("secnet.subtitle") }}</p>
      </div>
    </header>
    <div class="secnet-field">
      <span class="sr-label">{{ t('common.proxyHandling') }}</span>
      <div class="seg" role="group" :aria-label="t('common.proxyHandling')">
        <button v-for="mode in modeOptions" :key="mode.value" type="button" :aria-pressed="connectionMode === mode.value" :class="{ chosen: connectionMode === mode.value }" :disabled="busy || privacyBusy" @click="changeMode(mode.value)">{{ mode.label }}</button>
      </div>
    </div>
    <p class="scope-note">{{ t(connectionMode === 'tun' ? 'check.tunScope' : 'check.proxyScope') }}</p>
    <template v-if="connectionMode === 'tun'">
      <div class="setting-row">
        <span class="sr-label"><Network class="sr-icon" :size="17" />{{ t('settings.tunIpv6') }}<HelpTip :text="t('settings.tunIpv6Help')" /></span>
        <ToggleSwitch :model-value="networkSettings.tun.ipv6" :label="t('settings.tunIpv6')" :disabled="busy || connected || !networkLoaded" @update:model-value="setTunProtection({ ipv6: $event })" />
      </div>
      <div class="setting-row">
        <span class="sr-label"><Power class="sr-icon" :size="17" />{{ t('secnet.killSwitch') }}<HelpTip :text="t('secnet.killSwitchHint')" /></span>
        <ToggleSwitch :model-value="networkSettings.tun.killSwitch" :label="t('secnet.killSwitch')" :disabled="busy || connected || !networkLoaded" @update:model-value="setTunProtection({ killSwitch: $event })" />
      </div>
      <p v-if="connected && networkSettings.tun.killSwitch" class="secnet-guard" :class="guardArmed ? 'ok' : 'bad'" role="status"><component :is="guardArmed ? ShieldCheck : ShieldAlert" :size="15" :stroke-width="2" />{{ t(guardArmed ? 'secnet.killSwitchActive' : 'secnet.killSwitchInactive') }}</p>
      <p v-if="connected" class="scope-note">{{ t('settings.disconnectToChangeNetworkSettings') }}</p>
      <div class="dns-path">
        <span>{{ t('check.dnsConfiguration') }}</span>
        <strong>{{ resolverName }} · {{ networkSettings.tun.dnsMode === 'fakeip' ? t('settings.dnsFakeip') : remoteName }}</strong>
        <span v-if="networkSettings.tun.routeExclude.length">{{ t('check.routeExceptions', { count: networkSettings.tun.routeExclude.length }) }}</span>
        <button type="button" @click="tab = 'settings'">{{ t('settings.dnsSettings') }}</button>
      </div>
    </template>
    <p v-else class="scope-note secnet-guard-na"><ShieldAlert :size="14" :stroke-width="1.9" />{{ t('secnet.killSwitchTunOnly') }}</p>
    <p v-if="networkError" class="form-error" role="alert">{{ tx(networkError) }}</p>
    <div class="setting-row">
      <span class="sr-label"><Wrench class="sr-icon" :size="17" :stroke-width="1.8" />{{ t("secnet.repair") }}<HelpTip :text="t('secnet.repairHint')" /></span>
      <button type="button" class="secnet-repair" @click="openRepair">{{ t("secnet.repairOpen") }}</button>
    </div>
  </section>
  <dialog ref="repairDialog" class="secnet-dialog" aria-labelledby="repair-title">
    <h3 id="repair-title">{{ t("secnet.repairTitle") }}</h3>
    <p class="secnet-dialog-desc">{{ t("secnet.repairDesc") }}</p>
    <p v-if="repairUnavailable" class="secnet-result bad">{{ t("secnet.repairDesktopOnly") }}</p>
    <template v-else>
      <ul class="repair-list">
        <li v-for="key in REPAIR_KEYS" :key="key">
          <span class="repair-name">{{ t(REPAIR_LABEL[key]) }}</span>
          <span class="repair-status" :class="repairChecks[key]">{{ t(REPAIR_STATUS_LABEL[repairChecks[key]]) }}</span>
          <button v-if="repairChecks[key] === 'problem'" type="button" class="repair-fix" :disabled="repairBusy" @click="runRepair([key])">{{ t("secnet.fix") }}</button>
        </li>
      </ul>
      <p v-if="!detecting && REPAIR_KEYS.every(key => ['ok', 'fixed'].includes(repairChecks[key]))" class="secnet-result ok">{{ t("secnet.repairNoIssue") }}</p>
    </template>
    <div class="secnet-dialog-actions">
      <button type="button" :disabled="detecting || repairBusy || repairUnavailable" @click="detectRepairs">{{ t("secnet.repairRecheck") }}</button>
      <button type="button" @click="closeRepair">{{ t("common.close") }}</button>
      <button type="button" class="primary" :disabled="repairBusy || detecting || !problemKeys.length" @click="runRepair(problemKeys)"><Crosshair :size="16" :stroke-width="2" />{{ t("secnet.repairRun") }}</button>
    </div>
  </dialog>
  <section class="card check-page" :aria-label="t('nav.networkCheck')">
    <div class="check-health">
      <h3>{{ t("check.health") }}<HelpTip :text="t('check.healthHint')" /></h3>
      <div class="check-health-actions">
        <button type="button" class="check-speed-btn" @click="speedDialog?.open()"><Gauge :size="16" :stroke-width="2" />{{ t("check.speedTest") }}</button>
        <button class="primary" :disabled="status === 'running' || busy" @click="run"><Crosshair :size="16" :stroke-width="2" />{{ status === "running" ? t("check.running") : ran ? t("check.again") : t("check.start") }}</button>
      </div>
    </div>
    <div class="check-grid">
      <div class="check-item"><span><Wifi :size="16" :stroke-width="1.8" />{{ t("check.local") }}</span><strong :class="localTone">{{ tx(localText) }}</strong></div>
      <div class="check-item"><span><CircleCheck :size="16" :stroke-width="1.8" />{{ t("check.proxy") }}</span><strong :class="proxyTone">{{ tx(proxyText) }}</strong></div>
      <div class="check-item"><span><Globe :size="16" :stroke-width="1.8" />{{ t("check.dns") }}</span><strong :class="dnsTone">{{ tx(dnsText) }}</strong></div>
      <div class="check-item"><span><Crosshair :size="16" :stroke-width="1.8" />{{ t("check.exitIp") }}</span><strong :class="exitTone">{{ tx(exitText) }}</strong></div>
    </div>
    <div class="check-split" />
    <h3>{{ t("check.privacy") }}</h3>
    <div class="check-row"><span class="check-label">{{ t("check.dnsLeak") }}<HelpTip :text="t('check.dnsLeakHint')" /></span><span class="pill" :class="dnsToneRow">{{ tx(dnsLeak) }}</span></div>
    <div class="check-row"><span class="check-label">{{ t("check.ipv6Leak") }}<HelpTip :text="t('check.ipv6LeakHint')" /></span><span class="pill" :class="ipv6Tone">{{ tx(ipv6Leak) }}</span></div>
    <div class="check-row"><span class="check-label">{{ t("check.webrtcLeak") }}<HelpTip :text="t('check.webrtcLeakHint')" /></span><span class="pill" :class="webrtcTone">{{ tx(webrtcLeak) }}</span></div>
    <div v-if="ran && connected && connectionMode !== 'tun'" class="privacy-repair"><p>{{ t('check.privacyRepairHint') }}</p><button type="button" :disabled="busy || privacyBusy || status === 'running'" @click="protectPrivacy">{{ t('check.privacyRepair') }}</button></div>
    <p class="scope-note">{{ t('check.webviewScope') }}</p>
    <div class="manual-checks" :aria-label="t('check.browserChecks')">
      <span>{{ t('check.browserChecks') }}</span>
      <button v-for="kind in ['dns', 'webrtc', 'ip']" :key="kind" type="button" @click="openExternal('https://browserleaks.com/' + kind)"><ExternalLink :size="13" />{{ kind === 'dns' ? 'DNS' : kind === 'webrtc' ? 'WebRTC' : 'IP / IPv6' }}</button>
    </div>
    <p v-if="checkedAt" class="checked-time">{{ t('check.checkedAt', { time: checkedAt }) }}</p>
    <p v-if="error" class="form-error" role="alert">{{ tx(error) }}</p>
  </section>
  <UnlockCheck />
  <SpeedTestDialog ref="speedDialog" />
  </div>
</template>

<style scoped>
.scope-note { margin: 8px 0; color: var(--muted); font-size: 12px; line-height: 1.65; }
.dns-path { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px; padding: 12px 0; color: var(--muted); font-size: 12px; }
.dns-path strong { font-weight: 500; color: var(--text); overflow-wrap: anywhere; }
.dns-path button { margin-left: auto; min-height: 32px; font-size: 12px; }
.manual-checks { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; padding-top: 12px; border-top: 1px solid var(--line); font-size: 12px; color: var(--muted); }
.manual-checks button { min-height: 32px; padding: 0 10px; gap: 6px; font-size: 12px; }
.checked-time { font-size: 11px; color: var(--muted); margin: 12px 0 0; font-variant-numeric: tabular-nums; }
.check-item strong { overflow-wrap: anywhere; }

.check-stack { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
.check-page { display: flex; flex-direction: column; gap: 0; }
.check-page h3, .check-label { display: inline-flex; align-items: center; white-space: nowrap; }
.check-page h3 { margin: 0; font-size: 15px; font-weight: 600; }
.check-label { font-size: 15px; font-weight: 400; }
.check-health, .check-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
.check-health button { flex: none; min-height: 40px; padding: 0 16px; }
.check-health-actions { display: flex; align-items: center; gap: 8px; flex: none; }
.check-speed-btn { gap: 6px; }
.check-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px 28px; margin-top: 14px; }
.check-item span { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: 13px; font-weight: 400; }
.check-item strong { display: block; margin-top: 4px; font-size: 16px; font-weight: 600; font-variant-numeric: tabular-nums; }
.check-item strong.ok { color: var(--success); }
.check-item strong.bad { color: var(--danger); }
.check-split { height: 1px; margin: 22px 0 18px; background: var(--line); }
.check-row { align-items: center; padding: 10px 0; }
.check-row .pill { max-width: 46%; white-space: normal; text-align: right; }
.check-row .pill.ok { background: var(--success-bg); color: var(--success); }
.check-row .pill.warn { background:var(--warn-bg); color:var(--warn); }
.privacy-repair { display:flex; align-items:center; gap:12px; margin-top:12px; padding-top:12px; border-top:1px solid var(--line); }
.privacy-repair p { flex:1; margin:0; font-size:12px; color:var(--muted); line-height:1.5; }
.privacy-repair button { flex:none; min-height:32px; font-size:12px; }
.check-row .pill.bad { background: var(--danger-bg); color: var(--danger); }
.secnet { display: flex; flex-direction: column; gap: 0; }
.secnet-head { display: flex; align-items: center; gap: 13px; padding-bottom: 8px; }
.secnet-badge { display: inline-flex; align-items: center; justify-content: center; flex: none; width: 40px; height: 40px; border-radius: 12px; color: var(--accent); background: color-mix(in srgb, var(--accent) 13%, transparent); }
.secnet-headtext h2 { margin: 0; font-size: 17px; font-weight: 600; }
.secnet-headtext p { margin: 4px 0 0; color: var(--muted); font-size: 13px; line-height: 1.5; }
.secnet-field { display: flex; flex-direction: column; gap: 10px; padding: 14px 0 16px; }
.secnet-field > label { display: inline-flex; align-items: center; align-self: flex-start; font-size: 15px; font-weight: 500; }
.secnet-sub { padding: 2px 0 14px; }
.secnet-sub :deep(.select-menu) { width: 100%; min-width: 0; }
.sr-label { display: inline-flex; align-items: center; }
.sr-icon { flex: none; margin-right: 10px; color: var(--muted); }
.secnet .seg { display: flex; }
.secnet .seg > button { flex: 1; }
.secnet :focus-visible, .secnet-dialog :focus-visible { outline: none; box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
.secnet-repair { flex: none; min-height: 34px; padding: 0 18px; }
.secnet-guard { display: flex; align-items: center; gap: 7px; margin: 10px 0 0; font-size: 13px; font-weight: 500; line-height: 1.5; }
.secnet-guard.ok { color: var(--success); }
.secnet-guard.bad { color: var(--danger); }
.secnet-guard svg, .secnet-guard-na svg { flex: none; }
.secnet-guard-na { display: flex; align-items: center; gap: 7px; }
.secnet-dialog { width: min(420px, calc(100vw - 48px)); border: 1px solid var(--line); border-radius: 16px; padding: 20px 22px; background: var(--bg); color: var(--text); box-shadow: var(--shadow-card); }
.secnet-dialog::backdrop { background: #0b162880; }
.secnet-dialog h3 { margin: 0 0 10px; font-size: 16px; font-weight: 600; }
.secnet-dialog-desc { margin: 0; color: var(--muted); font-size: 14px; line-height: 1.6; }
.secnet-result { margin: 12px 0 0; font-size: 14px; font-weight: 500; }
.secnet-result.ok { color: var(--success); }
.secnet-result.bad { color: var(--danger); }
.secnet-dialog-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 20px; }
.repair-list { list-style: none; margin: 16px 0 0; padding: 0; display: flex; flex-direction: column; gap: 0; }
.repair-list li { display: flex; align-items: center; gap: 12px; padding: 11px 0; border-top: 1px solid var(--line); }
.repair-list li:first-child { border-top: 0; }
.repair-name { flex: 1; font-size: 14px; }
.repair-status { flex: none; font-size: 13px; font-weight: 500; }
.repair-status.ok, .repair-status.fixed { color: var(--success); }
.repair-status.problem, .repair-status.failed { color: var(--danger); }
.repair-status.checking, .repair-status.fixing { color: var(--muted); }
.repair-fix { flex: none; min-height: 30px; padding: 0 14px; font-size: 13px; }
@media (max-width: 720px) {
  .check-grid { grid-template-columns: 1fr; }
  .check-health, .check-row { flex-wrap: wrap; }
  .check-row { flex-direction: column; align-items: flex-start; }
  .check-row .pill { max-width: none; text-align: left; }
}
</style>
