<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useApp } from "../composables/useApp";
import ExitInfo from "../components/ExitInfo.vue";
import { usedTraffic } from "../lib/subscription-info";
import { ArrowDown, ArrowUp, ChevronRight, Link, Power, LoaderCircle, Zap } from "@lucide/vue";
import SelectMenu from "../components/ui/SelectMenu.vue";
const { t, tx, locale, tab, phase, phaseTone, connected, busy, connectionAction, connectionProgress, rawProfile, selectedNode, selectedNodeName, selectedRegionLabel, selectedLatencyText, connectionDuration, label, toggle, routeMode, connectionMode, setRouteMode, setConnectionMode, profileKind, profileLabel, activeSub, externalProfiles, activeExternalId, fmt, nodes, selectNode, regionCode, AUTOMATIC_NODE_ID, automaticNodeName, latencyTone, latencyLabel, testLatencies, testing, probeCancelling, probeSummary, automaticSelection, isNative, chain, chainActive, chainGroups, chainSummary, commitChain } = useApp();
const pending = computed(() => connectionAction.value !== null || ["PREPARING", "CONNECTING", "RECONNECTING", "DISCONNECTING"].includes(phase.value));
const connectionHeading = computed(() => connectionAction.value === "disconnect" ? t("status.disconnecting") : pending.value ? t(phase.value === "RECONNECTING" ? "status.reconnecting" : "status.connecting") : t(label.value));
const connectionHint = computed(() => pending.value ? tx(connectionProgress.value || (connectionAction.value === "disconnect" ? "status.disconnecting" : "startup.preparing")) : connected.value ? t("overview.clickToDisconnect") : rawProfile.value ? t("overview.clickToConnect") : t("overview.addASubscriptionFirst"));
// Proxy mode: single node vs a saved chain group (chain groups are kept per subscription).
const activeGroupId = computed(() => chainGroups.value.find(g => g.nodes.length === chain.value.length && g.nodes.every((n, i) => n === chain.value[i]))?.id ?? "");
const chainGroupOptions = computed(() => [
  ...(activeGroupId.value ? [] : [{ value: "", label: t("overview.customChain") }]),
  ...chainGroups.value.map(g => ({ value: g.id, label: g.name })),
]);
async function setProxyMode(mode: "single" | "chain") {
  if (busy.value || chainActive.value === (mode === "chain")) return;
  if (mode === "single") { await commitChain([]); return; }
  const group = chainGroups.value[0];
  if (group) await commitChain([...group.nodes]); else tab.value = "nodes";
}
async function pickChainGroup(id: string) {
  const group = chainGroups.value.find(g => g.id === id);
  if (group) await commitChain([...group.nodes]);
}

// One shape for both subscription kinds, so the usage card always has the same rows.
const usage = computed<{ used?: number; total?: number; expires?: number } | undefined>(() => {
  if (profileKind.value !== "external") {
    const sub = activeSub.value;
    return sub ? { used: sub.usedChargedBytes, total: sub.limitBytes, expires: sub.expiresAt ? new Date(sub.expiresAt).getTime() : undefined } : undefined;
  }
  const info = externalProfiles.value.find(item => item.id === activeExternalId.value)?.usage;
  return info ? { used: usedTraffic(info), total: info.total && info.total > 0 ? info.total : undefined, expires: info.expire ? info.expire * 1000 : undefined } : undefined;
});
const usageRatio = computed(() => usage.value?.used !== undefined && usage.value.total ? Math.min(100, Math.round(usage.value.used / usage.value.total * 100)) : 0);
const usageRemaining = computed(() => usage.value?.used !== undefined && usage.value.total !== undefined ? fmt(Math.max(0, usage.value.total - usage.value.used)) : "");

// Live rates come from the same connection snapshot the Connections page reads.
// Keep a fixed 60-second window so the chart scrolls right-to-left like a traffic monitor.
const RATE_WINDOW = 60;
const CHART_W = 360;
const CHART_H = 72;
const live = ref({ down: 0, up: 0, downTotal: 0, upTotal: 0 });
const history = ref<{ down: number; up: number }[]>([]);
let stopped = false;
let timer: ReturnType<typeof setTimeout> | undefined;
async function sample() {
  if (stopped) return;
  if (!document.hidden) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const snapshot = await invoke<{ uploadTotal: number; downloadTotal: number; active: { uploadSpeed: number; downloadSpeed: number }[] }>("list_connections");
      const down = snapshot.active.reduce((sum, row) => sum + (row.downloadSpeed || 0), 0);
      const up = snapshot.active.reduce((sum, row) => sum + (row.uploadSpeed || 0), 0);
      live.value = { down, up, downTotal: snapshot.downloadTotal || 0, upTotal: snapshot.uploadTotal || 0 };
      history.value = [...history.value.slice(-(RATE_WINDOW - 1)), { down, up }];
    } catch { /* Keep the last sample on a transient IPC failure. */ }
  }
  if (!stopped) timer = setTimeout(sample, 1000);
}
onMounted(() => { if (isNative) void sample(); });
onUnmounted(() => { stopped = true; clearTimeout(timer); });
function rate(value: number) {
  if (value < 1024) return { value: String(Math.round(value)), unit: "B/s" };
  if (value < 1024 ** 2) return { value: (value / 1024).toFixed(value < 10240 ? 1 : 0), unit: "KB/s" };
  return { value: (value / 1024 ** 2).toFixed(1), unit: "MB/s" };
}
const chart = computed(() => {
  // Left-pad so early samples enter from the right and the x-scale stays a full minute.
  const samples = Array.from({ length: RATE_WINDOW }, (_, i) => {
    const index = i - (RATE_WINDOW - history.value.length);
    return index >= 0 ? history.value[index] : { down: 0, up: 0 };
  });
  const max = Math.max(...samples.flatMap(item => [item.down, item.up]), 1);
  const top = 4;
  const bottom = CHART_H - 1;
  const height = bottom - top;
  const step = CHART_W / (RATE_WINDOW - 1);
  const y = (value: number) => top + height - (value / max) * height;
  const coords = samples.map((item, index) => ({
    x: index * step,
    down: y(item.down),
    up: y(item.up),
  }));
  const line = (key: "down" | "up") => coords.map(point => `${point.x.toFixed(1)},${point[key].toFixed(1)}`).join(" ");
  const area = (key: "down" | "up") =>
    `M0,${bottom.toFixed(1)} ${coords.map(point => `L${point.x.toFixed(1)},${point[key].toFixed(1)}`).join(" ")} L${CHART_W},${bottom.toFixed(1)} Z`;
  // Dashed guides matching the reference monitor's horizontal scale.
  const grids = [0.25, 0.5, 0.75, 1].map(ratio => top + height * (1 - ratio));
  return { downLine: line("down"), upLine: line("up"), downArea: area("down"), upArea: area("up"), grids };
});
</script>
<template>
  <div class="ov">
    <section class="card ov-hero" :aria-label="t('common.connectionStatus')">
      <div class="ov-hero-main">
        <button class="ov-power" :class="{ active: connected && !pending, pending }" :disabled="busy || ['PREPARING', 'CONNECTING', 'DISCONNECTING'].includes(phase) || !rawProfile || !selectedNode" :aria-busy="pending" :aria-label="connectionAction ? connectionHeading : connected ? t('overview.disconnect') : t('overview.connect')" @click="toggle"><LoaderCircle v-if="pending" class="connection-spinner" :size="30" :stroke-width="2" /><Power v-else :size="32" :stroke-width="2.2" /></button>
        <div class="ov-state">
          <span v-if="connected && !pending" class="pill" :class="phaseTone">{{ t("overview.proxyRunning") }}<template v-if="connectionDuration"> · {{ connectionDuration }}</template></span>
          <strong>{{ connectionHeading }}</strong>
          <span role="status" aria-live="polite">{{ connectionHint }}</span>
        </div>
        <button class="ov-node" @click="tab = 'nodes'"><span class="ov-node-text"><small>{{ t("overview.currentNode") }}</small><strong :title="chainActive ? chainSummary : tx(selectedNodeName)">{{ chainActive ? chainSummary : tx(selectedNodeName) }}</strong><span class="ov-node-sub">{{ chainActive ? t("overview.chainMode") : t("overview.single") }} · <span v-if="selectedLatencyText" class="ov-ms" :class="latencyTone(selectedNode)">{{ selectedLatencyText }}</span><span v-else>{{ t("overview.latencyPending") }}</span></span></span><ChevronRight :size="18" /></button>
      </div>
      <div class="ov-band">
        <div class="ov-field"><span class="ov-label">{{ t("overview.proxyMode") }}</span><div class="seg" role="group" :aria-label="t('overview.proxyMode')"><button :aria-pressed="!chainActive" :class="{ chosen: !chainActive }" :disabled="busy" @click="setProxyMode('single')">{{ t("overview.single") }}</button><button :aria-pressed="chainActive" :class="{ chosen: chainActive }" :disabled="busy" @click="setProxyMode('chain')">{{ t("overview.chainMode") }}</button></div></div>
        <div v-if="chainActive && chainGroups.length" class="ov-field"><span class="ov-label">{{ t("overview.chainGroup") }}</span><SelectMenu class="ov-group" :model-value="activeGroupId" :icon="Link" :label="t('overview.chainGroup')" :options="chainGroupOptions" @update:model-value="pickChainGroup" /></div>
        <div class="ov-field ov-field-end"><span class="ov-label">{{ t("overview.handling") }}</span><div class="seg" role="group" :aria-label="t('common.proxyHandling')"><button :aria-pressed="connectionMode === 'system'" :class="{ chosen: connectionMode === 'system' }" :disabled="busy" @click="setConnectionMode('system')">{{ t("common.systemProxy") }}</button><button :aria-pressed="connectionMode === 'tun'" :class="{ chosen: connectionMode === 'tun' }" :disabled="busy" @click="setConnectionMode('tun')">TUN</button></div></div>
      </div>
      <div class="ov-routing">
        <div class="ov-routing-text"><strong>{{ t("overview.routing") }}</strong></div>
        <div class="seg" role="group" :aria-label="t('common.routing')"><button :aria-pressed="routeMode === 'rule'" :class="{ chosen: routeMode === 'rule' }" :disabled="busy" @click="setRouteMode('rule')">{{ t("common.rule") }}</button><button :aria-pressed="routeMode === 'global'" :class="{ chosen: routeMode === 'global' }" :disabled="busy" @click="setRouteMode('global')">{{ t("overview.global") }}</button></div>
      </div>
    </section>
    <div class="ov-row">
      <section class="card ov-rates" :aria-label="t('overview.liveRates')">
        <div class="ov-rate-stats">
          <div class="ov-rate" role="group" :aria-label="t('overview.downloadRate')">
            <div class="ov-rate-head"><ArrowDown class="ov-down" :size="14" :stroke-width="2.4" />{{ t("common.download") }}</div>
            <div class="ov-rate-value"><strong>{{ rate(live.down).value }}</strong><span>{{ rate(live.down).unit }}</span></div>
          </div>
          <div class="ov-rate ov-rate-end" role="group" :aria-label="t('overview.uploadRate')">
            <div class="ov-rate-head"><ArrowUp class="ov-up" :size="14" :stroke-width="2.4" />{{ t("overview.upload") }}</div>
            <div class="ov-rate-value"><strong>{{ rate(live.up).value }}</strong><span>{{ rate(live.up).unit }}</span></div>
          </div>
        </div>
        <svg class="ov-spark" role="img" :aria-label="t('overview.liveRates')" :viewBox="`0 0 ${CHART_W} ${CHART_H}`" preserveAspectRatio="none">
          <g class="ov-grid" aria-hidden="true">
            <line v-for="(y, index) in chart.grids" :key="index" x1="0" :y1="y" :x2="CHART_W" :y2="y" />
          </g>
          <path class="ov-down-area" :d="chart.downArea" />
          <path class="ov-up-area" :d="chart.upArea" />
          <polyline class="ov-up" :points="chart.upLine" />
          <polyline class="ov-down" :points="chart.downLine" />
        </svg>
        <div class="ov-rate-foot">
          <span>{{ t("overview.sessionTraffic", { down: fmt(live.downTotal), up: fmt(live.upTotal) }) }}</span>
          <span>{{ t("overview.lastSixtySeconds") }}</span>
        </div>
      </section>
      <section class="card ov-sub" :aria-label="t('overview.subscriptionUsage')">
        <div class="ov-card-head"><div><h2>{{ t("overview.subscriptionUsage") }}</h2><span v-if="profileLabel !== 'overview.noActiveSubscription' && profileLabel !== 'subscriptions.generic'" class="pill tint" :title="tx(profileLabel)">{{ tx(profileLabel) }}</span></div><button class="link-button" @click="tab = 'sub'">{{ t("subscriptions.manage") }}</button></div>
        <div class="ov-sub-value"><strong>{{ usage?.used !== undefined ? fmt(usage.used) : "—" }}</strong><span>/ {{ usage?.total !== undefined ? fmt(usage.total) : t("common.notProvided") }}</span></div>
        <div class="meter" role="progressbar" :aria-label="t('common.trafficUsage')" :aria-valuenow="usageRatio" aria-valuemin="0" aria-valuemax="100"><span :style="{ width: usageRatio + '%' }" /></div>
        <div class="ov-sub-foot"><span>{{ usageRemaining ? t("overview.remaining", { amount: usageRemaining }) : profileKind === "external" ? t("overview.usageIsManagedByYour") : t("overview.noActiveSubscription") }}</span><span v-if="usage?.expires">{{ t("overview.expiresOn", { date: new Date(usage.expires).toLocaleDateString(locale) }) }}</span></div>
      </section>
    </div>
    <div class="ov-row">
      <ExitInfo :locale="locale" :connected="phase === 'CONNECTED'" :network-key="`${phase}:${selectedNode}:${automaticSelection?.tag || ''}:${routeMode}`" />
      <section class="card ov-quick" :aria-label="t('overview.quickAccess')">
        <div class="ov-card-head"><h2>{{ t("overview.quickAccess") }}</h2><div><button class="link-button" :disabled="probeCancelling" @click="testLatencies()">{{ probeCancelling ? t("overview.stopping") : testing ? t("nodes.stop") : t("nodes.test") }}</button><button class="link-button" @click="tab = 'nodes'">{{ t("nodes.all") }}</button></div></div>
        <p v-if="probeSummary" class="probe-home-status" role="status">{{ tx(probeSummary) }}</p>
        <div v-if="nodes.length" class="ov-quick-list" role="radiogroup" :aria-label="t('overview.quickAccess')">
          <button role="radio" :aria-checked="selectedNode === AUTOMATIC_NODE_ID" :class="{ selected: selectedNode === AUTOMATIC_NODE_ID }" :disabled="busy || !nodes.some(node => node.agentOnline)" @click="selectNode(AUTOMATIC_NODE_ID)"><span class="region-chip" aria-hidden="true"><Zap :size="13" /></span><strong :title="automaticNodeName || t('nodes.automatic')">{{ t("nodes.automatic") }}</strong><span v-if="selectedNode === AUTOMATIC_NODE_ID && connected && automaticSelection?.delay" class="ov-ms" :class="latencyTone(AUTOMATIC_NODE_ID)">{{ automaticSelection.delay }} ms</span></button>
          <button v-for="node in nodes.slice(0, 10)" :key="node.id" role="radio" :aria-checked="selectedNode === node.id" :class="{ selected: selectedNode === node.id }" :disabled="!node.agentOnline" @click="selectNode(node.id)"><span class="region-chip" aria-hidden="true">{{ regionCode(node.region, node.name) || "··" }}</span><strong :title="node.name">{{ node.name }}</strong><span v-if="latencyLabel(node.id) !== 'nodes.notTested'" class="ov-ms" :class="latencyTone(node.id)" :title="tx(latencyLabel(node.id))">{{ tx(latencyLabel(node.id)) }}</span></button>
        </div>
        <div v-else class="empty-note">{{ t("nodes.noneAvailable") }} <button class="link-button" @click="tab = 'sub'">{{ t("overview.addASubscription") }}</button></div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.connection-spinner{animation:connection-spin .9s linear infinite}
.ov-power.pending{opacity:1;cursor:progress}
@keyframes connection-spin{to{transform:rotate(360deg)}}
@media(prefers-reduced-motion:reduce){.connection-spinner{animation:none}}
</style>
