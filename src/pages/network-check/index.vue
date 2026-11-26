<script setup lang="ts">
  import { computed, ref, watch, onUnmounted } from "vue";
  import { invoke } from "@tauri-apps/api/core";
  import {
    CircleCheck,
    Crosshair,
    Gauge,
    Globe,
    Network,
    Power,
    ExternalLink,
    ShieldAlert,
    ShieldCheck,
    Wifi,
    Wrench,
  } from "@lucide/vue";
  import HelpTip from "../../components/ui/HelpTip.vue";
  import ToggleSwitch from "../../components/ui/ToggleSwitch.vue";
  import BaseButton from "../../components/ui/BaseButton.vue";
  import UnlockCheck from "../../components/UnlockCheck.vue";
  import SpeedTestDialog from "../../components/SpeedTestDialog.vue";
  import { useApp } from "../../composables/useApp";
  import { BROWSERLEAKS_BASE } from "../../constants/domains";
  import { ipv6Assessment, webrtcAssessment, type PrivacyTone, type RtcObservation } from "../../lib/privacy-check";
  import { collectWebrtc } from "../../lib/webrtc-check";
  import { confirmation } from "../../lib/confirmation";
  import type { MessageKey, Text } from "../../i18n";

  const {
    t,
    tx,
    isNative,
    setTunProtection,
    repairNetwork,
    networkError,
    networkSettings,
    connected,
    guardArmed,
    busy,
    networkLoaded,
    connectionMode,
    setConnectionMode,
    selectedNode,
    routeMode,
    phase,
    tab,
    openExternal,
  } = useApp();
  const speedDialog = ref<InstanceType<typeof SpeedTestDialog>>();
  const modeOptions = computed(() => [
    { value: "system" as const, label: t("common.systemProxy") },
    { value: "tun" as const, label: "TUN" },
    { value: "manual" as const, label: t("settings.manualProxy") },
  ]);
  const resolverName = computed(() =>
    networkSettings.value.tun.dnsDomestic === "dnspod" ? "DNSPod DoH" : "AliDNS DoH"
  );
  const remoteName = computed(
    () =>
      ({ cloudflare: "Cloudflare DoH", google: "Google DoH", quad9: "Quad9 DoH" })[networkSettings.value.tun.dnsRemote]
  );
  async function changeMode(value: "system" | "tun" | "manual") {
    const previous = connectionMode.value;
    if (busy.value || value === previous) return;
    if (
      !(await confirmation.request({
        title: "confirmation.proxyTitle",
        body: "confirmation.proxyBody",
      }))
    )
      return;
    if (!busy.value && connectionMode.value === previous) await setConnectionMode(value);
  }

  // Network repair: auto-detect on open, report status per item, support per-item fixes and one-click repair.
  type RepairKey = "connection" | "systemProxy" | "dns";
  type RepairStatus = "checking" | "ok" | "problem" | "fixing" | "fixed" | "failed";
  const REPAIR_KEYS: RepairKey[] = ["connection", "systemProxy", "dns"];
  const REPAIR_LABEL: Record<RepairKey, MessageKey> = {
    connection: "secnet.itemConnection",
    systemProxy: "secnet.itemProxy",
    dns: "secnet.itemDns",
  };
  const REPAIR_STATUS_LABEL: Record<RepairStatus, MessageKey> = {
    checking: "secnet.statusChecking",
    ok: "secnet.statusOk",
    problem: "secnet.statusProblem",
    fixing: "secnet.statusFixing",
    fixed: "secnet.statusFixed",
    failed: "secnet.statusFailed",
  };
  const repairDialog = ref<HTMLDialogElement | null>(null);
  const repairChecks = ref<Record<RepairKey, RepairStatus>>({
    connection: "checking",
    systemProxy: "checking",
    dns: "checking",
  });
  const repairBusy = ref(false);
  const repairUnavailable = ref(false);
  const problemKeys = computed(() => REPAIR_KEYS.filter(key => repairChecks.value[key] === "problem"));
  const detecting = computed(() => REPAIR_KEYS.some(key => repairChecks.value[key] === "checking"));

  function evaluateNative(native: Native): Record<RepairKey, "ok" | "problem"> {
    const proxyHealthy = native.proxy === "ok";
    return {
      connection: native.connected ? "ok" : "problem",
      systemProxy: proxyHealthy ? "ok" : "problem",
      dns: native.dnsOk ? "ok" : "problem",
    };
  }
  async function detectRepairs() {
    if (!isNative) {
      repairUnavailable.value = true;
      return;
    }
    repairUnavailable.value = false;
    for (const key of REPAIR_KEYS) repairChecks.value[key] = "checking";
    try {
      const result = evaluateNative(await invoke<Native>("network_check"));
      for (const key of REPAIR_KEYS) repairChecks.value[key] = result[key];
    } catch {
      for (const key of REPAIR_KEYS) repairChecks.value[key] = "failed";
    }
  }
  // The actual repair action: re-sync the system proxy (reapply the current connection mode), then re-check each item.
  async function runRepair(keys: RepairKey[]) {
    if (repairBusy.value || busy.value || !isNative || !keys.length) return;
    repairBusy.value = true;
    for (const key of keys) repairChecks.value[key] = "fixing";
    try {
      closeRepair();
      const repaired = await repairNetwork();
      repairDialog.value?.showModal();
      if (!repaired) {
        await detectRepairs();
        return;
      }
      const result = evaluateNative(await invoke<Native>("network_check"));
      for (const key of REPAIR_KEYS) {
        if (keys.includes(key)) repairChecks.value[key] = result[key] === "ok" ? "fixed" : "failed";
        else if (repairChecks.value[key] !== "fixed") repairChecks.value[key] = result[key];
      }
    } catch {
      for (const key of keys) repairChecks.value[key] = "failed";
    } finally {
      repairBusy.value = false;
    }
  }
  function openRepair() {
    repairDialog.value?.showModal();
    void detectRepairs();
  }
  function closeRepair() {
    repairDialog.value?.close();
  }

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
  onUnmounted(() => {
    generation++;
    cancelRtc?.();
  });
  const privacyBusy = ref(false);
  watch([phase, connectionMode, selectedNode, routeMode, networkSettings], () => {
    generation++;
    cancelRtc?.();
    resetResults();
    status.value = "idle";
  });
  async function protectPrivacy() {
    if (busy.value || privacyBusy.value) return;
    privacyBusy.value = true;
    try {
      await changeMode("tun");
      if (connectionMode.value === "tun" && connected.value) await run();
    } finally {
      privacyBusy.value = false;
    }
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

  const checkedAt = ref("");
  function resetResults() {
    ran.value = false;
    checkedAt.value = "";
    error.value = "";
    for (const value of [localText, proxyText, dnsText, exitText]) value.value = "check.pending";
    for (const value of [dnsLeak, ipv6Leak, webrtcLeak]) value.value = "check.notRun";
    for (const tone of [localTone, proxyTone, dnsTone, exitTone, dnsToneRow, ipv6Tone, webrtcTone]) tone.value = "idle";
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
    proxyText.value =
      native.proxy === "ok" ? "check.proxyOk" : native.proxy === "fail" ? "check.proxyFail" : "check.proxyOff";
    proxyTone.value = native.proxy === "ok" ? "ok" : native.proxy === "fail" ? "bad" : "idle";
    dnsText.value = native.dnsOk ? "check.dnsOk" : "check.dnsFail";
    dnsTone.value = native.dnsOk ? "ok" : "bad";
    exitText.value = native.exitIp || "check.exitFail";
    exitTone.value = native.exitIp ? "ok" : "bad";
    if (!native.connected) {
      dnsLeak.value = "check.dnsLeakNeedConnection";
      dnsToneRow.value = "idle";
    } else if (native.mode === "tun") {
      dnsLeak.value = "check.dnsLeakTun";
      dnsToneRow.value = "warn";
    } else {
      dnsLeak.value = "check.dnsLeakSystem";
      dnsToneRow.value = "warn";
    }
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
        <span class="sr-label">{{ t("common.proxyHandling") }}</span>
        <div class="seg" role="group" :aria-label="t('common.proxyHandling')">
          <BaseButton
            v-for="mode in modeOptions"
            :key="mode.value"
            :aria-pressed="connectionMode === mode.value"
            :class="{ chosen: connectionMode === mode.value }"
            :disabled="busy || privacyBusy"
            @click="changeMode(mode.value)"
          >
            {{ mode.label }}
          </BaseButton>
        </div>
      </div>
      <p class="scope-note">
        {{ t(connectionMode === "tun" ? "check.tunScope" : "check.proxyScope") }}
      </p>
      <template v-if="connectionMode === 'tun'">
        <div class="setting-row">
          <span class="sr-label"
            ><Network class="sr-icon" :size="17" />{{ t("settings.tunIpv6")
            }}<HelpTip :text="t('settings.tunIpv6Help')"
          /></span>
          <ToggleSwitch
            :model-value="networkSettings.tun.ipv6"
            :label="t('settings.tunIpv6')"
            :disabled="busy || connected || !networkLoaded"
            @update:model-value="setTunProtection({ ipv6: $event })"
          />
        </div>
        <div class="setting-row">
          <span class="sr-label"
            ><Power class="sr-icon" :size="17" />{{ t("secnet.killSwitch")
            }}<HelpTip :text="t('secnet.killSwitchHint')"
          /></span>
          <ToggleSwitch
            :model-value="networkSettings.tun.killSwitch"
            :label="t('secnet.killSwitch')"
            :disabled="busy || connected || !networkLoaded"
            @update:model-value="setTunProtection({ killSwitch: $event })"
          />
        </div>
        <p
          v-if="connected && networkSettings.tun.killSwitch"
          class="secnet-guard"
          :class="guardArmed ? 'ok' : 'bad'"
          role="status"
        >
          <component :is="guardArmed ? ShieldCheck : ShieldAlert" :size="15" :stroke-width="2" />{{
            t(guardArmed ? "secnet.killSwitchActive" : "secnet.killSwitchInactive")
          }}
        </p>
        <p v-if="connected" class="scope-note">
          {{ t("settings.disconnectToChangeNetworkSettings") }}
        </p>
        <div class="dns-path">
          <span>{{ t("check.dnsConfiguration") }}</span>
          <strong
            >{{ resolverName }} ·
            {{ networkSettings.tun.dnsMode === "fakeip" ? t("settings.dnsFakeip") : remoteName }}</strong
          >
          <span v-if="networkSettings.tun.routeExclude.length">{{
            t("check.routeExceptions", { count: networkSettings.tun.routeExclude.length })
          }}</span>
          <BaseButton @click="tab = 'settings'">{{ t("settings.dnsSettings") }}</BaseButton>
        </div>
      </template>
      <p v-else class="scope-note secnet-guard-na">
        <ShieldAlert :size="14" :stroke-width="1.9" />{{ t("secnet.killSwitchTunOnly") }}
      </p>
      <p v-if="networkError" class="form-error" role="alert">{{ tx(networkError) }}</p>
      <div class="setting-row">
        <span class="sr-label"
          ><Wrench class="sr-icon" :size="17" :stroke-width="1.8" />{{ t("secnet.repair")
          }}<HelpTip :text="t('secnet.repairHint')"
        /></span>
        <BaseButton class="secnet-repair" @click="openRepair">
          {{ t("secnet.repairOpen") }}
        </BaseButton>
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
            <span class="repair-status" :class="repairChecks[key]">{{
              t(REPAIR_STATUS_LABEL[repairChecks[key]])
            }}</span>
            <BaseButton
              v-if="repairChecks[key] === 'problem'"
              class="repair-fix"
              :disabled="repairBusy"
              @click="runRepair([key])"
            >
              {{ t("secnet.fix") }}
            </BaseButton>
          </li>
        </ul>
        <p
          v-if="!detecting && REPAIR_KEYS.every(key => ['ok', 'fixed'].includes(repairChecks[key]))"
          class="secnet-result ok"
        >
          {{ t("secnet.repairNoIssue") }}
        </p>
      </template>
      <div class="secnet-dialog-actions">
        <BaseButton :disabled="detecting || repairBusy || repairUnavailable" @click="detectRepairs">
          {{ t("secnet.repairRecheck") }}
        </BaseButton>
        <BaseButton @click="closeRepair">{{ t("common.close") }}</BaseButton>
        <BaseButton
          variant="primary"
          :disabled="repairBusy || detecting || !problemKeys.length"
          @click="runRepair(problemKeys)"
        >
          <Crosshair :size="16" :stroke-width="2" />{{ t("secnet.repairRun") }}
        </BaseButton>
      </div>
    </dialog>
    <section class="card check-page" :aria-label="t('nav.networkCheck')">
      <div class="check-health">
        <h3>{{ t("check.health") }}<HelpTip :text="t('check.healthHint')" /></h3>
        <div class="check-health-actions">
          <BaseButton class="check-speed-btn" @click="speedDialog?.open()">
            <Gauge :size="16" :stroke-width="2" />{{ t("check.speedTest") }}
          </BaseButton>
          <BaseButton variant="primary" :disabled="status === 'running' || busy" @click="run">
            <Crosshair :size="16" :stroke-width="2" />{{
              status === "running" ? t("check.running") : ran ? t("check.again") : t("check.start")
            }}
          </BaseButton>
        </div>
      </div>
      <div class="check-grid">
        <div class="check-item">
          <span><Wifi :size="16" :stroke-width="1.8" />{{ t("check.local") }}</span
          ><strong :class="localTone">{{ tx(localText) }}</strong>
        </div>
        <div class="check-item">
          <span><CircleCheck :size="16" :stroke-width="1.8" />{{ t("check.proxy") }}</span
          ><strong :class="proxyTone">{{ tx(proxyText) }}</strong>
        </div>
        <div class="check-item">
          <span><Globe :size="16" :stroke-width="1.8" />{{ t("check.dns") }}</span
          ><strong :class="dnsTone">{{ tx(dnsText) }}</strong>
        </div>
        <div class="check-item">
          <span><Crosshair :size="16" :stroke-width="1.8" />{{ t("check.exitIp") }}</span
          ><strong :class="exitTone">{{ tx(exitText) }}</strong>
        </div>
      </div>
      <div class="check-split" />
      <h3>{{ t("check.privacy") }}</h3>
      <div class="check-row">
        <span class="check-label">{{ t("check.dnsLeak") }}<HelpTip :text="t('check.dnsLeakHint')" /></span
        ><span class="pill" :class="dnsToneRow">{{ tx(dnsLeak) }}</span>
      </div>
      <div class="check-row">
        <span class="check-label">{{ t("check.ipv6Leak") }}<HelpTip :text="t('check.ipv6LeakHint')" /></span
        ><span class="pill" :class="ipv6Tone">{{ tx(ipv6Leak) }}</span>
      </div>
      <div class="check-row">
        <span class="check-label">{{ t("check.webrtcLeak") }}<HelpTip :text="t('check.webrtcLeakHint')" /></span
        ><span class="pill" :class="webrtcTone">{{ tx(webrtcLeak) }}</span>
      </div>
      <div v-if="ran && connected && connectionMode !== 'tun'" class="privacy-repair">
        <p>{{ t("check.privacyRepairHint") }}</p>
        <BaseButton :disabled="busy || privacyBusy || status === 'running'" @click="protectPrivacy">
          {{ t("check.privacyRepair") }}
        </BaseButton>
      </div>
      <p class="scope-note">{{ t("check.webviewScope") }}</p>
      <div class="manual-checks" :aria-label="t('check.browserChecks')">
        <span>{{ t("check.browserChecks") }}</span>
        <BaseButton v-for="kind in ['dns', 'webrtc', 'ip']" :key="kind" @click="openExternal(BROWSERLEAKS_BASE + kind)">
          <ExternalLink :size="13" />{{ kind === "dns" ? "DNS" : kind === "webrtc" ? "WebRTC" : "IP / IPv6" }}
        </BaseButton>
      </div>
      <p v-if="checkedAt" class="checked-time">{{ t("check.checkedAt", { time: checkedAt }) }}</p>
      <p v-if="error" class="form-error" role="alert">{{ tx(error) }}</p>
    </section>
    <UnlockCheck />
    <SpeedTestDialog ref="speedDialog" />
  </div>
</template>
<style scoped lang="scss" src="./index.scss"></style>
