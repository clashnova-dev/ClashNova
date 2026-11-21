<script setup lang="ts">
import { computed, ref, watch, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { CircleCheck, Crosshair, Globe, Network, Power, ShieldAlert, ShieldCheck, SlidersHorizontal, Wifi, Wrench } from "@lucide/vue";
import HelpTip from "../components/ui/HelpTip.vue";
import SelectMenu from "../components/ui/SelectMenu.vue";
import UnlockCheck from "../components/UnlockCheck.vue";
import { useApp } from "../composables/useApp";
import { isPublicIp, webrtcAssessment, type PrivacyTone, type RtcObservation } from '../lib/privacy-check';
import type { Ipv6Policy } from "../lib/network-security";
import type { MessageKey, Text } from "../i18n";

const { t, tx, isNative, networkSecurity, setNetworkSecurity, setIpv6Policy, setTunProtection, networkSettings, connected, busy, networkLoaded, connectionMode, setConnectionMode, selectedNode } = useApp();

const ipv6Options = computed(() => ([
  { value: "follow", label: t("secnet.ipv6Follow") },
  { value: "proxyOnly", label: t("secnet.ipv6ProxyOnly") },
  { value: "block", label: t("secnet.ipv6Block") },
] as { value: Ipv6Policy; label: string }[]));

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
  const proxyHealthy = native.proxy === "ok" || (native.mode === "tun" && native.proxy === "off");
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
  if (repairBusy.value || !isNative || !keys.length) return;
  repairBusy.value = true;
  for (const key of keys) repairChecks.value[key] = "fixing";
  try {
    await setConnectionMode(connectionMode.value);
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
watch([connected, connectionMode, selectedNode], () => { generation++; ran.value = false; status.value = 'idle'; });
async function protectPrivacy() {
  if (busy.value || privacyBusy.value) return;
  privacyBusy.value = true;
  try {
    await setConnectionMode('tun');
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

function addressOf(candidate: string): string {
  const parts = candidate.trim().replace(/^a=/, "").split(/\s+/);
  if (!parts[0]?.startsWith("candidate:") || parts.length < 6) return "";
  const ip = parts[4];
  if (!/^(\d{1,3}\.){3}\d{1,3}$/.test(ip) && !ip.includes(":")) return "";
  return ip;
}

function gatherWebrtc(): Promise<Rtc> {
  cancelRtc?.();
  return new Promise((resolve) => {
    if (typeof RTCPeerConnection === "undefined") {
      resolve({ kind: "off" });
      return;
    }
    let pc: RTCPeerConnection;
    try {
      pc = new RTCPeerConnection({ iceServers: [{ urls: "stun:stun.cloudflare.com:3478" }] });
    } catch {
      resolve({ kind: "off" });
      return;
    }
    const found = new Set<string>();
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      cancelRtc = undefined;
      try { pc.close(); } catch { /* The peer connection is already gone. */ }
      const ips = [...found];
      const pub = ips.find(isPublicIp);
      if (pub) resolve({ kind: "public", ips: ips.filter(isPublicIp) });
      else if (ips.length) resolve({ kind: "lan" });
      else resolve({ kind: "none" });
    };
    const timer = setTimeout(finish, 4000);
    cancelRtc = () => { clearTimeout(timer); finish(); };
    pc.onicecandidate = (event) => {
      if (!event.candidate) {
        clearTimeout(timer);
        finish();
        return;
      }
      const ip = addressOf(event.candidate.candidate);
      if (ip && !ip.endsWith(".local")) found.add(ip);
    };
    try { pc.createDataChannel("check"); } catch { /* Gathering can continue without a channel. */ }
    pc.createOffer()
      .then((offer) => pc.setLocalDescription(offer))
      .catch(() => { clearTimeout(timer); if (!settled) { settled = true; cancelRtc = undefined; try { pc.close(); } catch {} resolve({ kind: "off" }); } });
  });
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
  const ipv6Bypass = native.connected && !!native.ipv6Ip && !(native.mode === "tun" && native.tunIpv6);
  if (!native.ipv6Ip) { ipv6Leak.value = "check.ipv6None"; ipv6Tone.value = "ok"; }
  else if (!native.connected) { ipv6Leak.value = "check.ipv6Present"; ipv6Tone.value = "idle"; }
  else if (ipv6Bypass) { ipv6Leak.value = "check.ipv6Bypass"; ipv6Tone.value = "bad"; }
  else { ipv6Leak.value = "check.ipv6Through"; ipv6Tone.value = "ok"; }
  applyWebrtc(rtc, native);
}

async function run() {
  if (status.value === "running") return;
  const runId = ++generation;
  status.value = "running";
  error.value = "";
  const [nativeResult, rtcResult] = await Promise.allSettled([
    isNative ? invoke<Native>("network_check") : Promise.reject(new Error("check.desktopOnly")),
    gatherWebrtc(),
  ]);
  if (runId !== generation) return;
  const rtc: Rtc = rtcResult.status === "fulfilled" ? rtcResult.value : { kind: "off" };
  ran.value = true;
  status.value = "done";
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
      <label class="sr-label"><SlidersHorizontal class="sr-icon" :size="17" :stroke-width="1.8" />{{ t("secnet.mode") }}<HelpTip :text="t('secnet.modeHint')" /></label>
      <div class="seg" role="group" :aria-label="t('secnet.mode')">
        <button type="button" :aria-pressed="networkSecurity.protectionMode === 'compat'" :class="{ chosen: networkSecurity.protectionMode === 'compat' }" @click="setNetworkSecurity({ protectionMode: 'compat' })">{{ t("secnet.modeCompat") }}</button>
        <button type="button" :aria-pressed="networkSecurity.protectionMode === 'balanced'" :class="{ chosen: networkSecurity.protectionMode === 'balanced' }" @click="setNetworkSecurity({ protectionMode: 'balanced' })">{{ t("secnet.modeBalanced") }}</button>
        <button type="button" :aria-pressed="networkSecurity.protectionMode === 'strict'" :class="{ chosen: networkSecurity.protectionMode === 'strict' }" @click="setNetworkSecurity({ protectionMode: 'strict' })">{{ t("secnet.modeStrict") }}</button>
      </div>
    </div>
    <div class="setting-row">
      <span class="sr-label"><Globe class="sr-icon" :size="17" :stroke-width="1.8" />{{ t("secnet.dnsLeak") }}<HelpTip :text="t('secnet.dnsLeakHint')" /></span>
      <button type="button" role="switch" :aria-checked="connectionMode === 'tun'" :aria-label="t('secnet.dnsLeak')" class="toggle-switch" :class="{ enabled: connectionMode === 'tun' }" :disabled="busy || privacyBusy || connectionMode === 'tun'" @click="protectPrivacy"><span /></button>
    </div>
    <div class="secnet-field secnet-sub">
      <label class="sr-label"><Network class="sr-icon" :size="17" :stroke-width="1.8" />{{ t("secnet.ipv6") }}</label>
      <SelectMenu :model-value="networkSecurity.ipv6Policy" :options="ipv6Options" :label="t('secnet.ipv6')" @update:model-value="setIpv6Policy" />
    </div>
    <div class="setting-row">
      <span class="sr-label"><Power class="sr-icon" :size="17" :stroke-width="1.8" />{{ t("secnet.killSwitch") }}<HelpTip :text="t('secnet.killSwitchHint')" /></span>
      <button type="button" role="switch" :aria-checked="networkSettings.tun.killSwitch" :aria-label="t('secnet.killSwitch')" class="toggle-switch" :class="{ enabled: networkSettings.tun.killSwitch }" :disabled="busy || connected || !networkLoaded" @click="setTunProtection({ killSwitch: !networkSettings.tun.killSwitch })"><span /></button>
    </div>
    <div class="setting-row">
      <span class="sr-label"><ShieldAlert class="sr-icon" :size="17" :stroke-width="1.8" />{{ t("secnet.malicious") }}<HelpTip :text="t('secnet.maliciousHint')" /></span>
      <button type="button" role="switch" :aria-checked="networkSecurity.maliciousDomainBlock" :aria-label="t('secnet.malicious')" class="toggle-switch" :class="{ enabled: networkSecurity.maliciousDomainBlock }" @click="setNetworkSecurity({ maliciousDomainBlock: !networkSecurity.maliciousDomainBlock })"><span /></button>
    </div>
    <div class="setting-row">
      <span class="sr-label"><Wrench class="sr-icon" :size="17" :stroke-width="1.8" />{{ t("secnet.repair") }}<HelpTip :text="t('secnet.repairHint')" /></span>
      <button type="button" class="secnet-repair" @click="openRepair">{{ t("secnet.repairOpen") }}</button>
    </div>
  </section>
  <dialog ref="repairDialog" class="secnet-dialog">
    <h3>{{ t("secnet.repairTitle") }}</h3>
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
      <p v-if="!detecting && !problemKeys.length" class="secnet-result ok">{{ t("secnet.repairNoIssue") }}</p>
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
      <button class="primary" :disabled="status === 'running'" @click="run"><Crosshair :size="16" :stroke-width="2" />{{ status === "running" ? t("check.running") : ran ? t("check.again") : t("check.start") }}</button>
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
    <p v-if="error" class="form-error" role="alert">{{ tx(error) }}</p>
  </section>
  <UnlockCheck />
  </div>
</template>

<style scoped>
.check-stack { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
.check-page { display: flex; flex-direction: column; gap: 0; }
.check-page h3, .check-label { display: inline-flex; align-items: center; white-space: nowrap; }
.check-page h3 { margin: 0; font-size: 15px; font-weight: 600; }
.check-label { font-size: 15px; font-weight: 400; }
.check-health, .check-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
.check-health button { flex: none; min-height: 40px; padding: 0 16px; }
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
