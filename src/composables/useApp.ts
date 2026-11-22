import { prepareConnection } from "../lib/connection-startup";
import { resolveProxyPort, type PortConflict } from "../lib/port-conflict";
import { createAuthenticatedApi } from "../lib/authenticated-api";
import { confirmation } from "../lib/confirmation";
import { restoreCrashConnection } from "../lib/crash-recovery";
import { ref, computed, watch, nextTick, type Component } from "vue";
import { House, Layers, Rss, Network, ScrollText, Settings, Radar, Route } from "@lucide/vue";
import { reduce, type Phase } from "../lib/connection";
import { notifications, loadNotificationPreference, prepareNotifications, dismissNotice } from "../lib/notifications";
import { ensureTunService, refreshTunService, tunServiceBusy } from "../lib/tun-service";
import { AUTOMATIC_NODE_ID, AUTOMATIC_OUTBOUND_TAG, activeChain, configureSelection, outboundTag } from "../lib/automatic-selection";
import { parseSubscriptionUsage, subscriptionHost, type SubscriptionUsage, type SubscriptionDownload } from "../lib/subscription-info";
import { platform } from "@tauri-apps/plugin-os";
import { DEFAULT_PROXY_PORT, DEFAULT_TUN_MTU, defaultTunSettings, parseRouteExclude, validateProxyPort, validateTunMtu, proxyAddress, type NetworkSettings, type TunSettings } from "../lib/network-settings";
import { preferencesKey, type Preferences } from "../lib/preferences";
import { loadExternalSubscriptions } from "../lib/subscriptions-store";
import { appPoliciesStoreKey, loadStoredPreferences, networkSecurityStoreKey, networkSettingsStoreKey, openSettingsStore, rememberStored, saveStoredPreferences, writeStored, readClientSetting, writeClientSetting, readLegacyClientValue, removeLegacyClientValue, type SettingStore, type StoreStatus } from "../lib/settings-store";
import { defaultNetworkSecurity, parseNetworkSecurity, type NetworkSecurity } from "../lib/network-security";
import { appPolicyRules, parseAppPolicies, type AppPolicy } from "../lib/app-policies";
import { parseSingBoxProfile } from "../lib/profiles";
import { sortByLatency } from "../lib/node-order";
import { regionCode } from "../lib/regions";
import { encode, msg, translator, type Locale, type MessageKey, type Text } from "../i18n";
import { parseDeepLink, redeemedSubscription } from "../lib/deep-link";
import { builtinRouteRules } from "../lib/routing-rules";

export function create() {
const locale = ref<Locale>("en");
const theme = ref<"system" | "light" | "dark">("system");
const { t, tx } = translator(() => locale.value);
// What to show for a failed operation: the reason given (an Error's message, or the code a native command
// rejects with), otherwise the fallback for that operation.
const failure = (error: unknown, fallback: MessageKey): Text => error instanceof Error && error.message ? error.message : typeof error === "string" && error ? error : fallback;
function showError(detail: Text) {
  message.value = detail;
  void notifications.show({ body: tx(detail), kind: "error", native: false, key: `error:${typeof detail === "string" ? detail : "detail"}` });
}
const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
function applyTheme() {
  document.documentElement.dataset.theme = theme.value === "system" ? (colorScheme.matches ? "dark" : "light") : theme.value;
  document.documentElement.lang = locale.value;
}
watch([theme, locale], applyTheme, { immediate: true });
colorScheme.addEventListener("change", applyTheme);

const tab = ref<"home" | "nodes" | "sub" | "connections" | "rules" | "logs" | "network" | "settings" | "help">("home");
const mainContent = ref<HTMLElement | null>(null);
const logViewport = ref<HTMLElement | null>(null);
const followLogs = ref(true);
const logsLoading = ref(false);
const logsError = ref('');
let logsTimer: ReturnType<typeof setTimeout> | undefined;
let logsGeneration = 0;
function onLogScroll() {
  const el = logViewport.value;
  if (el) followLogs.value = el.scrollHeight - el.clientHeight - el.scrollTop < 32;
}
async function scrollLogs() {
  await nextTick();
  if (followLogs.value && logViewport.value) logViewport.value.scrollTop = logViewport.value.scrollHeight;
}
async function pollLogs(generation: number) {
  try { await refreshLogs(); } catch { /* Keep the previous logs on transient IPC failure. */ }
  if (generation === logsGeneration && tab.value === "logs") logsTimer = setTimeout(() => void pollLogs(generation), 1000);
}
watch(tab, async () => {
  clearTimeout(logsTimer);
  const generation = ++logsGeneration;
  if (tab.value === "logs") {
    followLogs.value = true;
    void pollLogs(generation);
    await scrollLogs();
  }
  await nextTick();
  mainContent.value?.scrollTo({ top: 0 });
});
const isNative = "__TAURI_INTERNALS__" in window;
const currentPlatform = isNative ? platform() : "web";
const isMac = currentPlatform === "macos";
const isLinux = currentPlatform === "linux";
const networkSettings = ref<NetworkSettings>({ proxyPort: DEFAULT_PROXY_PORT, allowLan: false, tun: defaultTunSettings() });
const networkSecurity = ref<NetworkSecurity>(defaultNetworkSecurity());
const appPolicies = ref<Record<string, AppPolicy>>({});
const draftProxyPort = ref(String(DEFAULT_PROXY_PORT));
const draftAllowLan = ref(false);
// The virtual device options are edited as a draft and saved with the other network settings.
// MTU and the bypass list are kept as typed until saved, so a half-typed value is not rejected.
const draftTun = ref<TunSettings>(defaultTunSettings());
const draftTunMtu = ref(String(DEFAULT_TUN_MTU));
const draftTunExclude = ref("");
const networkLoaded = ref(false);
const networkError = ref<Text>("");
function storedNetwork(value: unknown): NetworkSettings | null {
  if (!value || typeof value !== "object") return null;
  const item = value as NetworkSettings;
  if (typeof item.proxyPort !== "number" || typeof item.allowLan !== "boolean") return null;
  return { proxyPort: item.proxyPort, allowLan: item.allowLan, tun: { ...defaultTunSettings(), ...(item.tun ?? {}) } };
}
function sameNetwork(left: NetworkSettings, right: NetworkSettings) {
  const a = storedNetwork(left), b = storedNetwork(right);
  return !!a && !!b && a.proxyPort === b.proxyPort && a.allowLan === b.allowLan && JSON.stringify(a.tun) === JSON.stringify(b.tun);
}
function applyNetworkSettings(settings: NetworkSettings) {
  // An older native build does not send the device options.
  const tun = { ...defaultTunSettings(), ...(settings.tun ?? {}) };
  networkSettings.value = { ...settings, tun };
  draftProxyPort.value = String(settings.proxyPort);
  draftAllowLan.value = settings.allowLan;
  draftTun.value = { ...tun, routeExclude: [...tun.routeExclude] };
  draftTunMtu.value = String(tun.mtu);
  draftTunExclude.value = tun.routeExclude.join("\n");
}
const networkDirty = computed(() => {
  const saved = networkSettings.value, tun = saved.tun, draft = draftTun.value;
  return draftProxyPort.value !== String(saved.proxyPort) || draftAllowLan.value !== saved.allowLan
    || draft.stack !== tun.stack || draft.strictRoute !== tun.strictRoute || draft.ipv6 !== tun.ipv6 || draft.autoRedirect !== tun.autoRedirect
    || draft.killSwitch !== tun.killSwitch
    || draft.dnsMode !== tun.dnsMode || draft.dnsRemote !== tun.dnsRemote || draft.dnsDomestic !== tun.dnsDomestic
    || draftTunMtu.value.trim() !== String(tun.mtu) || draftTunExclude.value.trim() !== tun.routeExclude.join("\n");
});
// A rejected save leaves the explanation up; the next edit clears it, including a correction back to the saved value.
watch([draftProxyPort, draftAllowLan, draftTunMtu, draftTunExclude, draftTun], () => { networkError.value = ""; }, { deep: true });
const localProxyAddress = computed(() => proxyAddress(networkSettings.value.proxyPort));
const showTraySpeed = ref(false);
const autostartEnabled = ref(false);
const autostartReady = ref(false);
const autostartBusy = ref(false);
const autostartError = ref<Text>("");
const proxyDiagnostic = ref<Text>("settings.checkingSystemProxy");
const diagnosticBusy = ref(false);
const phase = ref<Phase>("IDLE");
const message = ref<Text>("notice.signInFirst");
const busy = ref(false);
const connectionAction = ref<"connect" | "disconnect" | null>(null);
const connectionProgress = ref("");
const testing = ref(false);
const accountMenu = ref(false);
const localMode = ref(false);
type ExternalProfile = { id: string; name: string; url: string; raw: string; updatedAt: string; usage?: SubscriptionUsage };
const externalProfiles = ref<ExternalProfile[]>([]);
const activeExternalId = ref("");
const newProfileName = ref("");
const profileDialog = ref<HTMLDialogElement | null>(null);
const importDialog = ref<HTMLDialogElement | null>(null);
const importRequest = ref<{ code: string } | { url: string; name: string } | null>(null);
const importError = ref<Text>("");
// A clash:// import shows the subscription host so the user sees what they confirm.
const importTarget = computed(() => {
  const request = importRequest.value;
  if (!request || !("url" in request)) return null;
  let host = request.url;
  try { host = new URL(request.url).host; } catch { /* fall back to the raw address */ }
  return { name: request.name, host };
});
const profileNameInput = ref<HTMLInputElement | null>(null);
const profileError = ref<Text>("");
const nodeSort = ref<"default" | "latency">("default");
const probeSummary = ref<Text>("");
let activeProbeSession: string | null = null;
const newProfileUrl = ref("");
const selectedFile = ref<File | null>(null);
const profileKind = ref<"airport" | "external">("airport");
const currentExternal = computed(() => externalProfiles.value.find(item => item.id === activeExternalId.value));
const installedVersion = ref("0.1.0");
const checkingUpdates = ref(false);
const installingUpdate = ref(false);
const updateOffer = ref<{ version: string; notes: string } | null>(null);
const updateStatus = ref<Text>("");
let pendingUpdate: { downloadAndInstall: () => Promise<void> } | null = null;
// The address is no longer typed at sign-in: it comes from the build (VITE_DEFAULT_SERVER),
// or from what an earlier version saved on this machine.
const defaultServer = (import.meta.env.VITE_DEFAULT_SERVER ?? "").trim();
// When false the account / sign-in system is hidden and the client runs as a
// plain subscription manager (build-time flag, VITE_ENABLE_LOGIN).
const enableLogin = (import.meta.env.VITE_ENABLE_LOGIN ?? "true").trim().toLowerCase() !== "false";
if (!enableLogin) { localMode.value = true; tab.value = "sub"; }
// About page links (project repository, chat group). Empty = the row is hidden.
const githubUrl = (import.meta.env.VITE_GITHUB_URL ?? "https://github.com/clashnova-dev/ClashNova").trim();
async function openExternal(url: string) {
  if (!url) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("open_external", { url });
  } catch (error) {
    showError(failure(error, "native.openFailed"));
  }
}
const baseUrl = ref(defaultServer);
const rememberMe = ref(false);
// The tunnel dropped and the kill switch is holding the network closed until it is back or released.
const guardBlocking = ref(false);
const email = ref("");
const password = ref("");
const access = ref("");
const refreshToken = ref("");
const loginError = ref<Text>("");
const subUrl = ref("");
const logs = ref<string[]>([]);
const activityLogs = ref<string[]>([]);
const selectedNode = ref("");
// Ordered node ids (entry → exit) forming a proxy chain; active with 2+ hops.
const chain = ref<string[]>([]);
// Saved proxy-chain presets for the current profile.
type ChainGroup = { id: string; name: string; nodes: string[] };
const chainGroups = ref<ChainGroup[]>([]);
// End-to-end latency of the chain being edited (entry → … → exit), measured by routing
// a probe through the exit outbound with its detour chain in place.
const chainTesting = ref(false);
const chainDelay = ref<number | null>(null);
const chainError = ref<Text>("");
let chainRun = 0;
const latencies = ref<Record<string, number>>({});
// Rows keep their place while a test runs: the order uses the delays from before the run
// and is re-sorted once, when the run ends, instead of jumping as each result arrives.
const orderDelays = ref<Record<string, number>>({});
watch([latencies, testing], () => { if (!testing.value) orderDelays.value = { ...latencies.value }; }, { deep: true, immediate: true });
const sortedNodes = computed(() => nodeSort.value === "latency" ? sortByLatency(nodes.value, orderDelays.value) : nodes.value);
const probeStates = ref<Record<string, "queued" | "testing" | "success" | "failed">>({});
const probeErrors = ref<Record<string, Text>>({});
const coldLatencies = ref<Record<string, number>>({});
const warmedLatencies = ref<Record<string, boolean>>({});
const probeProgress = ref(0);
const probeTotal = ref(0);
// How many nodes are latency-tested in parallel; user-configurable in Settings (1–64).
const probeConcurrency = ref(8);
const probeCancelling = ref(false);
let probeRun = 0;
const rawProfile = ref("");
const connectionMode = ref<"system" | "tun" | "manual">("system");
const routeMode = ref<"rule" | "global">("rule");
const nodes = ref<{ id: string; name: string; region: string; host: string; port: number; protocol: string; agentOnline: boolean; appliedConfigVersion: number; latestConfigVersion: number }[]>([]);
const activeSub = ref<{ planName: string; usedChargedBytes: number; limitBytes: number; expiresAt?: string | null; subscriptionUrl?: string } | null>(null);
const navigation = computed(() => {
  const items: { key: "home" | "nodes" | "sub" | "connections" | "rules" | "logs" | "network" | "settings" | "help"; label: MessageKey; icon: Component }[] = [
    { key: "home", label: "nav.overview", icon: House }, { key: "nodes", label: "nav.nodes", icon: Layers },
    { key: "sub", label: "nav.subscriptions", icon: Rss }, { key: "connections", label: "nav.connections", icon: Network }, { key: "rules", label: "nav.rules", icon: Route }, { key: "logs", label: "nav.logs", icon: ScrollText },
    { key: "network", label: "nav.networkCheck", icon: Radar },
    { key: "settings", label: "nav.settings", icon: Settings },
  ];
  return items;
});

watch(tab, () => confirmation.cancel(), { flush: "sync" });
const connected = computed(() => phase.value === "CONNECTED" || phase.value === "RECONNECTING");
const chainActive = computed(() => chain.value.length >= 2);
const chainNames = computed(() => chain.value.map((id) => nodes.value.find((n) => n.id === id)?.name || id));
const chainSummary = computed(() => chainNames.value.join(" → "));
const connectionNodeNames = computed(() => Object.fromEntries([["select",t("connections.selector")],["direct",t("connections.direct")],[AUTOMATIC_OUTBOUND_TAG,t("nodes.automatic")],...nodes.value.map(node => [profileKind.value === "external" ? node.id : `node-${node.id.replaceAll("-", "").slice(0, 8)}`, node.name])]));
const title = computed<MessageKey>(() => (({ home: "nav.overview", nodes: "nav.nodes", sub: "nav.subscriptions", connections: "nav.connections", rules: "nav.rules", logs: "nav.logs", network: "nav.networkCheck", settings: "nav.settings", help: "nav.help" } as const)[tab.value]));
const label = computed<MessageKey>(() => (({
  IDLE: "status.idle",
  PREPARING: "status.preparing",
  CONNECTING: "status.connecting",
  CONNECTED: "status.connected",
  RECONNECTING: "status.reconnecting",
  DISCONNECTING: "status.disconnecting",
  FAILED: "status.failed",
} as Record<string, MessageKey>)[phase.value] || "status.unknown"));
const trafficLabel = computed(() => {
  const s = activeSub.value;
  if (!s) return "—";
  return `${fmt(s.usedChargedBytes)} / ${fmt(s.limitBytes)}`;
});
const allLogs = computed(() => [...activityLogs.value, ...logs.value].sort());
watch(allLogs, () => { if (tab.value === "logs") void scrollLogs(); });
// Log lines are stored language-neutral (a key or an encoded message after the time and level)
// and rendered here, so the log follows the language in use.
const logLinePattern = /^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\s+(?:\[[A-Z]+\]\s+)?)([\s\S]*)$/;
const renderLogLine = (line: string) => { const parts = logLinePattern.exec(line); return parts ? parts[1] + tx(parts[2]) : tx(line); };
const logsText = computed(() => (allLogs.value.length ? allLogs.value.map(renderLogLine).join("\n") : t("logs.empty")));
const airportUsage = computed<SubscriptionUsage | undefined>(() => activeSub.value ? { upload: 0, download: activeSub.value.usedChargedBytes, total: activeSub.value.limitBytes, expire: activeSub.value.expiresAt ? Math.floor(new Date(activeSub.value.expiresAt).getTime() / 1000) : undefined } : undefined);
const usagePercent = computed(() => activeSub.value?.limitBytes ? Math.min(100, Math.round(activeSub.value.usedChargedBytes / activeSub.value.limitBytes * 100)) : 0);
const remainingTraffic = computed(() => activeSub.value ? fmt(Math.max(0, activeSub.value.limitBytes - activeSub.value.usedChargedBytes)) : "—");
const automaticSelection = ref<{ tag: string; delay?: number | null } | null>(null);
const automaticNodeName = computed(() => nodes.value.find(node => outboundTag(node.id, profileKind.value === "external") === automaticSelection.value?.tag)?.name || "");
const selectedNodeName = computed(() => selectedNode.value === AUTOMATIC_NODE_ID ? `${t("nodes.automatic")}${automaticNodeName.value ? " · " + automaticNodeName.value : ""}` : nodes.value.find((node) => node.id === selectedNode.value)?.name || "nodes.notSelected");
let automaticTimer: ReturnType<typeof setTimeout> | undefined;
let automaticGeneration = 0;
async function refreshAutomaticSelection() {
  const run = automaticGeneration;
  if (!isNative || phase.value !== "CONNECTED" || selectedNode.value !== AUTOMATIC_NODE_ID) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const result = await invoke<{ tag: string; delay?: number | null } | null>("get_auto_selection");
    if (run === automaticGeneration) automaticSelection.value = result;
  } catch { if (run === automaticGeneration) automaticSelection.value = null; }
  finally { if (run === automaticGeneration) automaticTimer = setTimeout(refreshAutomaticSelection, 5000); }
}
watch([phase, selectedNode, rawProfile], () => {
  automaticGeneration++; clearTimeout(automaticTimer); automaticSelection.value = null;
  void refreshAutomaticSelection();
});
const profileLabel = computed(() => profileKind.value === "external" ? currentExternal.value?.name || "subscriptions.generic" : activeSub.value?.planName || "overview.noActiveSubscription");

type Tone = "good" | "fair" | "bad" | "idle";
function delayTone(delay: number): Tone { return delay <= 100 ? "good" : delay <= 300 ? "fair" : "bad"; }
function latencyTone(id: string): Tone {
  if (id === AUTOMATIC_NODE_ID) return connected.value && automaticSelection.value?.delay ? delayTone(automaticSelection.value.delay) : "idle";
  const state = probeStates.value[id];
  if (state === "failed") return "bad";
  const value = latencies.value[id];
  return value === undefined || state === "queued" || state === "testing" ? "idle" : delayTone(value);
}
const selectedNodeInfo = computed(() => {
  const external = profileKind.value === "external";
  const id = selectedNode.value === AUTOMATIC_NODE_ID ? nodes.value.find(node => outboundTag(node.id, external) === automaticSelection.value?.tag)?.id : selectedNode.value;
  return nodes.value.find(node => node.id === id);
});
const selectedRegionLabel = computed(() => {
  const code = selectedNodeInfo.value ? regionCode(selectedNodeInfo.value.region, selectedNodeInfo.value.name) : "";
  return code || (selectedNode.value === AUTOMATIC_NODE_ID ? "AUTO" : "··");
});
const selectedLatencyText = computed(() => {
  if (!selectedNode.value) return "";
  if (selectedNode.value === AUTOMATIC_NODE_ID) return connected.value && automaticSelection.value?.delay ? `${automaticSelection.value.delay} ms` : "";
  const value = latencies.value[selectedNode.value];
  return value === undefined ? "" : `${value} ms`;
});
const phaseTone = computed<Tone>(() => connectionAction.value ? "fair" : phase.value === "CONNECTED" ? "good" : phase.value === "FAILED" ? "bad" : phase.value === "IDLE" ? "idle" : "fair");
// The header chip replaces the old bottom status bar: state, node and latency.
const statusChipText = computed(() => connectionAction.value ? t(connectionAction.value === "connect" ? "status.connecting" : "status.disconnecting") : [t(label.value), connected.value && selectedNode.value ? tx(selectedNodeName.value) : "", connected.value ? selectedLatencyText.value : ""].filter(Boolean).join(" · "));
// Time since this window first saw the connection up; resets on disconnect.
const connectedSince = ref<number | null>(null);
const clock = ref(Date.now());
let clockTimer: ReturnType<typeof setInterval> | undefined;
watch(connected, (value) => {
  clearInterval(clockTimer);
  if (value) {
    connectedSince.value ??= Date.now();
    clock.value = Date.now();
    clockTimer = setInterval(() => { clock.value = Date.now(); }, 1000);
  } else connectedSince.value = null;
}, { immediate: true });
const connectionDuration = computed(() => {
  if (connectedSince.value === null) return "";
  const total = Math.max(0, Math.floor((clock.value - connectedSince.value) / 1000));
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(Math.floor(total / 3600))}:${pad(Math.floor(total / 60) % 60)}:${pad(total % 60)}`;
});

function fmt(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KiB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MiB`;
  return `${(n / 1024 ** 3).toFixed(2)} GiB`;
}
function logLine(value: string) { return `${new Date().toLocaleString("sv-SE", { hour12: false })}  [INFO] ${value}`; }
function addActivity(text: Text) {
  const value = encode(text);
  const line = value.length > 4096 ? "native.logEntryTooLong" : value;
  activityLogs.value = [...activityLogs.value.slice(-99), logLine(line)];
  if (isNative && line === value) void import("@tauri-apps/plugin-log").then(({ info }) => info(line));
}
function latencyLabel(id: string) {
  if (id === AUTOMATIC_NODE_ID) return !connected.value ? "nodes.testedAfterConnect" : automaticSelection.value?.delay ? `${automaticSelection.value.delay} ms` : "nodes.testing";
  if (id === AUTOMATIC_NODE_ID ? !nodes.value.some(node => node.agentOnline) : !nodes.value.find(node => node.id === id)?.agentOnline) return "nodes.offline";
  const state = probeStates.value[id];
  if (state === "queued") return "nodes.queued";
  if (state === "testing") return "nodes.testing";
  if (state === "failed") return "nodes.timedOut";
  const value = latencies.value[id];
  return value === undefined ? "nodes.notTested" : `${value} ms`;
}

type Snapshot = { phase: Phase; message: string; logs?: string[]; blocked?: boolean };
type Bridge = {
  state(): Promise<Snapshot>;
  logs(): Promise<string[]>;
  proxyDiagnostics(): Promise<string>;
  inspectProxyPort(): Promise<PortConflict | null>;
  releaseProxyPort(token: string): Promise<void>;
  loadNetworkSettings(): Promise<NetworkSettings>;
  saveNetworkSettings(settings: NetworkSettings): Promise<NetworkSettings>;
  saveProfile(config: string): Promise<void>;
  clearProfile(): Promise<void>;
  setConnectionMode(mode: "system" | "tun" | "manual"): Promise<void>;
  probeNode(host: string, port: number): Promise<number>;
  beginProbe(config: string, tags: string[]): Promise<string>;
  probeDelay(session: string, tag: string): Promise<{ delay: number; coldDelay: number; warmed: boolean }>;
  endProbe(session: string): Promise<void>;
  fetchSubscription(url: string): Promise<string>;
  fetchSubscriptionDetails(url: string): Promise<SubscriptionDownload>;
  convertSubscription(raw: string): Promise<string>;
  loadSubscriptions(): Promise<string>;
  saveSubscriptions(data: string): Promise<void>;
  connect(mode: "system" | "tun" | "manual"): Promise<Snapshot>;
  disconnect(): Promise<Snapshot>;
  onState?(cb: (s: Snapshot) => void): Promise<() => void>;
  onProgress?(cb: (stage: string) => void): Promise<() => void>;
};

let previewBridge: Bridge | null = null;
async function bridge(): Promise<Bridge> {
  if ("__TAURI_INTERNALS__" in window) {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    return {
      state: () => invoke("connection_state"),
      logs: () => invoke("read_logs"),
      proxyDiagnostics: () => invoke("proxy_diagnostics"),
      inspectProxyPort: () => invoke("inspect_proxy_port"),
      releaseProxyPort: (token) => invoke("release_proxy_port", { token }),
      loadNetworkSettings: () => invoke("load_network_settings"),
      saveNetworkSettings: (settings) => invoke("save_network_settings", { settings }),
      saveProfile: (config) => invoke("save_profile", { config }),
      clearProfile: () => invoke("clear_profile"),
      setConnectionMode: (mode) => invoke("set_connection_mode", { mode }),
      probeNode: (host, port) => invoke("probe_node", { host, port }),
      beginProbe: (config, tags) => invoke("begin_probe", { config, tags }),
      probeDelay: (session, tag) => invoke("probe_delay", { session, tag }),
      endProbe: (session) => invoke("end_probe", { session }),
      fetchSubscription: (url) => invoke("fetch_subscription", { url }),
      async fetchSubscriptionDetails(url) {
        try { return await invoke<SubscriptionDownload>("fetch_subscription_details", { url }); }
        catch (error) {
          // Development hot reload can update Vue before the native binary restarts.
          if (!/command[^\n]*not found/i.test(String(error))) throw error;
          return { raw: await invoke<string>("fetch_subscription", { url }), userinfo: null, metadataSupported: false };
        }
      },
      convertSubscription: (raw) => invoke("convert_subscription", { raw }),
      loadSubscriptions: () => invoke("load_subscriptions"),
      saveSubscriptions: (data) => invoke("save_subscriptions", { data }),
      connect: (mode) => invoke("connect", { mode }),
      disconnect: () => invoke("disconnect"),
      async onProgress(cb) {
        return listen<{ stage: string }>("connection://progress", event => cb(event.payload.stage));
      },
      async onState(cb) {
        const un = await listen<Snapshot>("connection://state", (e) => cb(e.payload));
        return () => {
          un();
        };
      },
    };
  }
  return previewBridge ??= browserBridge();
}

function browserBridge(): Bridge {
  let previewSettings: NetworkSettings = { proxyPort: DEFAULT_PROXY_PORT, allowLan: false, tun: defaultTunSettings() };
  let current: Phase = "IDLE";
  let note = "preview.noCore";
  let lines: string[] = [];
  return {
    async proxyDiagnostics() { return "settings.proxyCheckDesktopOnly"; },
    async inspectProxyPort() { return null; },
    async releaseProxyPort() {},
    async loadNetworkSettings() { return previewSettings; },
    async saveNetworkSettings(settings) { previewSettings = settings; return settings; },
    async state() {
      return { phase: current, message: note, logs: lines };
    },
    async logs() {
      return lines;
    },
    async saveProfile() {
      note = "preview.profileSaved";
      lines.push(logLine(note));
    },
    async clearProfile() { current = "IDLE"; note = "preview.profileCleared"; lines = []; },
    async setConnectionMode() {},
    async probeNode() { throw new Error("preview.latencyNeedsDesktop"); },
    async beginProbe() { throw new Error("preview.probeNeedsDesktop"); },
    async probeDelay() { throw new Error("preview.probeNeedsDesktop"); },
    async endProbe() {},
    async fetchSubscription(url) { const response = await fetch(url); if (!response.ok) throw new Error(`HTTP ${response.status}`); return response.text(); },
    async fetchSubscriptionDetails(url) { const response = await fetch(url); if (!response.ok) throw new Error(`HTTP ${response.status}`); return { raw: await response.text(), userinfo: response.headers.get("subscription-userinfo") }; },
    async convertSubscription() { throw new Error("error.conversionDesktopOnly"); },
    async loadSubscriptions() { return "[]"; },
    async saveSubscriptions() {},
    async connect(_mode) {
      current = reduce(current === "FAILED" ? "FAILED" : "IDLE", "start");
      current = reduce(current, "core_spawned");
      current = reduce(current, "fail");
      note = "preview.cannotConnect";
      lines.push(logLine(note));
      return { phase: current, message: note, logs: lines };
    },
    async disconnect() {
      if (current !== "FAILED" && current !== "IDLE") current = reduce(current, "stop");
      if (current === "DISCONNECTING") current = reduce(current, "stopped");
      if (current === "FAILED") current = reduce(current, "stop");
      note = "status.disconnected";
      lines.push(logLine(note));
      return { phase: current, message: note, logs: lines };
    },
  };
}

function applySnapshot(snapshot: Snapshot) {
  phase.value = snapshot.phase;
  guardBlocking.value = snapshot.blocked === true;
  if (["CONNECTED", "IDLE", "FAILED"].includes(snapshot.phase)) connectionProgress.value = "";
  message.value = snapshot.phase === "CONNECTED" ? "status.ready" : snapshot.message;
  if (snapshot.logs) logs.value = snapshot.logs;
}

let stopListen: (() => void) | undefined;
let stopProgressListen: (() => void) | undefined;
let stopTrayListen: (() => void) | undefined;
let stopDeepLink: (() => void) | undefined;
let cacheReady = false;
let settingsStore: SettingStore | null = null;
const preferenceStoreStatus: StoreStatus = { useStore: false };

watch(message, value => {
  if (value) addActivity(value);
  if (!cacheReady) return;
  if (value && !["PREPARING", "CONNECTING", "RECONNECTING"].includes(phase.value)) void notifications.show({ body: tx(value), kind: phase.value === "FAILED" ? "error" : "info" });
}, { flush: "sync" });
watch(phase, (current, previous) => {
  if (!cacheReady || current === previous) return;
  const body = current === "CONNECTED" ? "notifications.proxyConnected" : current === "FAILED" ? "notifications.connectionFailed" : current === "IDLE" && previous !== "IDLE" ? "notifications.proxyDisconnected" : "";
  if (body) void notifications.show({ body: t(body), native: true, toast: false, kind: current === "FAILED" ? "error" : "success", key: `connection:${current}` });
});
let selectedNodes: Record<string, string> = {};
let chains: Record<string, string[]> = {};
let chainGroupsByProfile: Record<string, ChainGroup[]> = {};
const profileCacheKey = () => profileKind.value === "external" ? activeExternalId.value : `airport:${baseUrl.value}:${email.value}`;
function cachedNode(candidates: typeof nodes.value) {
  const cached = selectedNodes[profileCacheKey()];
  if (cached === AUTOMATIC_NODE_ID && candidates.some(node => node.agentOnline)) return AUTOMATIC_NODE_ID;
  return candidates.find(node => node.id === cached && node.agentOnline)?.id || candidates.find(node => node.agentOnline)?.id || "";
}
// Restore the saved chain for this profile, dropping any hop that is no longer present.
function cachedChain(candidates: typeof nodes.value) {
  const ids = new Set(candidates.map(node => node.id));
  return (chains[profileCacheKey()] || []).filter(id => ids.has(id));
}
function cachedChainGroups() {
  return (chainGroupsByProfile[profileCacheKey()] || []).map(group => ({ ...group, nodes: [...group.nodes] }));
}
function persistPreferences() {
  if (!cacheReady) return;
  if (selectedNode.value) selectedNodes[profileCacheKey()] = selectedNode.value;
  chains[profileCacheKey()] = chain.value;
  chainGroupsByProfile[profileCacheKey()] = chainGroups.value;
  const value: Preferences = { version: 1, showTraySpeed: showTraySpeed.value, baseUrl: baseUrl.value, email: email.value,
    connectionMode: connectionMode.value, routeMode: routeMode.value,
    activeProfileId: activeExternalId.value, nodeSort: nodeSort.value, probeConcurrency: probeConcurrency.value, locale: locale.value, theme: theme.value, selectedNodes, chains, chainGroups: chainGroupsByProfile };
  void saveStoredPreferences(settingsStore, value, preferenceStoreStatus)
    .catch(() => { message.value = "error.preferencesNotSaved"; });
}
watch([baseUrl, email, connectionMode, routeMode, activeExternalId, selectedNode, chain, chainGroups, nodeSort, probeConcurrency, locale, theme, showTraySpeed], persistPreferences, { deep: true });
// Native menu items cannot look copy up themselves, so their text is sent from the dictionary.
const trayLabels = { "open": "tray.open", "system": "tray.system", "tun": "tray.tun", "route-title": "tray.routeTitle", "global": "tray.global", "rule": "tray.rule", "profile": "tray.profile", "node-list": "tray.nodeList", "connect": "tray.connect", "disconnect": "tray.disconnect", "more": "tray.more", "sub": "tray.subscriptions", "settings": "tray.settings", "help": "tray.help", "quit": "tray.quit", "connected": "tray.connected", "failed": "tray.failed", "idle": "tray.idle" } as const satisfies Record<string, MessageKey>;
async function syncTray() {
  if (!isNative) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("sync_tray", { snapshot: { showTraySpeed: showTraySpeed.value, labels: Object.fromEntries(Object.entries(trayLabels).map(([id, key]) => [id, t(key)])), mode: connectionMode.value, route: routeMode.value,
      node: tx(selectedNodeName.value), selected: selectedNode.value, profile: tx(profileLabel.value),
      nodes: [...(nodes.value.length ? [{ id: AUTOMATIC_NODE_ID, name: t("nodes.automatic"), enabled: nodes.value.some(node => node.agentOnline) }] : []), ...nodes.value.map(node => ({ id: node.id, name: node.name, enabled: node.agentOnline }))], ready: !!rawProfile.value && !!selectedNode.value, busy: busy.value } });
  } catch (error) { addActivity(msg("log.trayUpdateFailed", { error: String(error) })); }
}
watch([phase, busy, connectionMode, routeMode, selectedNodeName, rawProfile, locale, profileLabel, nodes, showTraySpeed], syncTray);
async function trayAction(action: string) {
  if (busy.value) return;
  if (action === "toggle-connection" && rawProfile.value && selectedNode.value) await toggle();
  else if (action === "toggle-route") await setRouteMode(routeMode.value === "rule" ? "global" : "rule");
  else if (action === "connect" && !connected.value && rawProfile.value) await toggle();
  else if (action === "disconnect" && connected.value) await toggle();
  else if (["system", "manual", "tun"].includes(action)) await setConnectionMode(action === connectionMode.value ? "manual" : action as "system" | "manual" | "tun");
  else if (action.startsWith("select-node:")) await selectNode(action.slice("select-node:".length));
  else if (action === "rule" || action === "global") await setRouteMode(action);
  else if (action === "nodes" || action === "settings" || action === "sub" || action === "help") tab.value = action;
  await syncTray();
}

function preventContextMenu(event: MouseEvent) { event.preventDefault(); }

  // The login screen is a card-sized window; the main UI is larger and keeps the size the user gave it.
  const LOGIN_WINDOW = { width: 360, height: 500 };
  const MAIN_WINDOW = { width: 900, height: 670, minWidth: 820, minHeight: 650 };
  const mainWindowKey = "airport.window.main";
  let windowMode: "login" | "main" | "" = "";
  let windowBooted = false;
  // Stay hidden until subscriptions and the session are restored, so startup
  // does not paint the login window and then grow into the main window.
  let windowReady = false;
  let windowQueue: Promise<void> = Promise.resolve();
  let stopWindowResize: (() => void) | undefined;
  let windowSaveTimer: ReturnType<typeof setTimeout> | undefined;
  let rememberedMainWindow = { width: MAIN_WINDOW.width, height: MAIN_WINDOW.height };
  const acceptMainWindow = (value: unknown) => {
    const saved = value as { width?: number; height?: number } | null;
    return Number.isFinite(saved?.width) && Number.isFinite(saved?.height)
      ? { width: Math.max(MAIN_WINDOW.minWidth, Math.round(saved!.width!)), height: Math.max(MAIN_WINDOW.minHeight, Math.round(saved!.height!)) }
      : null;
  };
  function savedMainWindow() { return rememberedMainWindow; }
  // Queued so a sign-in that lands while an earlier resize is still running cannot interleave with it.
  function syncWindowMode() {
    if (!isNative) return windowQueue;
    windowQueue = windowQueue.then(async () => {
      if (!windowReady) return;
      const mode = enableLogin && !access.value && !localMode.value ? "login" : "main";
      const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
      const win = getCurrentWindow();
      if (mode !== windowMode) {
        windowMode = mode;
        if (await win.isMaximized()) await win.unmaximize();
        if (mode === "login") {
          await win.setMinSize(new LogicalSize(LOGIN_WINDOW.width, LOGIN_WINDOW.height));
          await win.setSize(new LogicalSize(LOGIN_WINDOW.width, LOGIN_WINDOW.height));
          await win.setResizable(false);
        } else {
          const size = savedMainWindow();
          await win.setResizable(true);
          await win.setSize(new LogicalSize(size.width, size.height));
          await win.setMinSize(new LogicalSize(MAIN_WINDOW.minWidth, MAIN_WINDOW.minHeight));
        }
        // While starting up the window is still hidden; signing in or out re-centres it.
        if (windowBooted) await win.center();
      }
      // Show only after the size matches this screen, so startup does not flash the other window.
      await win.show();
    }).catch(() => { windowMode = ""; /* Retry on the next change; the window stays usable at its current size. */ });
    return windowQueue;
  }
  async function watchWindowSize() {
    if (!isNative) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    stopWindowResize = await win.onResized(() => {
      clearTimeout(windowSaveTimer);
      windowSaveTimer = setTimeout(async () => {
        try {
          if (windowMode !== "main" || await win.isMaximized() || await win.isFullscreen()) return;
          const size = (await win.innerSize()).toLogical(await win.scaleFactor());
          const next = { width: Math.round(size.width), height: Math.round(size.height) };
          await writeClientSetting("mainWindow", next, mainWindowKey);
          rememberedMainWindow = next;
        } catch { /* Remembering the size is best effort. */ }
      }, 400);
    });
  }
  watch([access, localMode], () => { void syncWindowMode(); });

  const bootstrap = async () => {
  document.addEventListener("contextmenu", preventContextMenu);
  const apiBridge = await bridge();
  void watchWindowSize().catch(() => { /* The window still works without size memory. */ });
  settingsStore = await openSettingsStore();
  let savedPrefs: Preferences | null = null;
  try {
    savedPrefs = await loadStoredPreferences(settingsStore, () => readLegacyClientValue(preferencesKey), preferenceStoreStatus, () => removeLegacyClientValue(preferencesKey));
    rememberedMainWindow = await readClientSetting("mainWindow", rememberedMainWindow, acceptMainWindow, {
      key: mainWindowKey, parse: raw => { try { return acceptMainWindow(JSON.parse(raw)); } catch { return null; } },
    });
    await loadNotificationPreference();
  } catch (error) { message.value = failure(error, "error.preferencesNotSaved"); }
  if (savedPrefs) {
    baseUrl.value = defaultServer || savedPrefs.baseUrl; email.value = savedPrefs.email;
    connectionMode.value = savedPrefs.connectionMode; routeMode.value = savedPrefs.routeMode;
    selectedNodes = savedPrefs.selectedNodes; chains = savedPrefs.chains; chainGroupsByProfile = savedPrefs.chainGroups; nodeSort.value = savedPrefs.nodeSort; probeConcurrency.value = savedPrefs.probeConcurrency;
    locale.value = savedPrefs.locale; theme.value = savedPrefs.theme; showTraySpeed.value = savedPrefs.showTraySpeed;
  }
  // The tray starts with fallback text; give it the real labels as soon as the language is known.
  void syncTray();
  if (isNative) {
    try { installedVersion.value = await (await import("@tauri-apps/api/app")).getVersion(); }
    catch { /* The app version remains visible from the bundle metadata fallback. */ }
  }
  try {
    const nativeNetwork = await apiBridge.loadNetworkSettings();
    const chosen = await rememberStored(settingsStore, networkSettingsStoreKey, nativeNetwork, storedNetwork, preferenceStoreStatus);
    applyNetworkSettings(sameNetwork(chosen, nativeNetwork) ? chosen : await apiBridge.saveNetworkSettings(chosen));
    networkLoaded.value = true;
  } catch (error) { networkError.value = String(error); }
  networkSecurity.value = await rememberStored(settingsStore, networkSecurityStoreKey, defaultNetworkSecurity(), parseNetworkSecurity, preferenceStoreStatus);
  appPolicies.value = await rememberStored(settingsStore, appPoliciesStoreKey, {} as Record<string, AppPolicy>, parseAppPolicies, preferenceStoreStatus);
  if (isNative) {
    try {
      autostartEnabled.value = await (await import("@tauri-apps/plugin-autostart")).isEnabled();
      autostartReady.value = true;
    } catch (error) { autostartError.value = String(error); }
  } else { autostartError.value = "settings.autostartDesktopOnly"; }
  applySnapshot(await apiBridge.state());
  logs.value = await apiBridge.logs();
  try {
    const nativeList = await loadExternalSubscriptions(apiBridge, settingsStore, preferenceStoreStatus);
    const profiles = nativeList.filter((item: any): item is ExternalProfile =>
      typeof item?.id === "string" && typeof item?.name === "string" && typeof item?.url === "string" && typeof item?.raw === "string" && typeof item?.updatedAt === "string");
    externalProfiles.value = profiles;
    if (JSON.stringify(profiles) !== JSON.stringify(nativeList)) await apiBridge.saveSubscriptions(JSON.stringify(profiles));
  } catch (error) { message.value = msg("error.subscriptionListFailed", { error: String(error) }); }
  try {
    // Migrate an existing installation with saved subscriptions once; explicit logout stays logged out.
    const profile = savedPrefs ? externalProfiles.value.find(item => item.id === savedPrefs.activeProfileId) : externalProfiles.value[0];
    if (profile) {
      await activateExternal(profile);
      message.value = "subscriptions.restored";
    }
    await apiBridge.setConnectionMode(connectionMode.value);
  } catch (error) { message.value = msg("error.restoreFailed", { error: String(error) }); }
  await restoreSession();
  windowReady = true;
  await syncWindowMode();
  windowBooted = true;
  void refreshProxyDiagnostics();
  cacheReady = true;
  persistPreferences();
  addActivity("log.clientStarted");
  if (apiBridge.onState) stopListen = await apiBridge.onState(applySnapshot);
  if (apiBridge.onProgress) stopProgressListen = await apiBridge.onProgress(stage => { connectionProgress.value = stage; });
  if (isNative) {
    const { listen } = await import("@tauri-apps/api/event");
    stopTrayListen = await listen<string>("tray://action", event => { void trayAction(event.payload); });
    const { getCurrent, onOpenUrl, register } = await import("@tauri-apps/plugin-deep-link");
    if (currentPlatform === "windows" || currentPlatform === "linux") for (const scheme of ["clashnova", "clash", "clashmeta"]) await register(scheme).catch(() => {});
    stopDeepLink = await onOpenUrl(urls => { void acceptDeepLinks(urls); });
    void acceptDeepLinks(await getCurrent());
    await syncTray();
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await restoreCrashConnection({
        requested: () => invoke<boolean>("take_crash_recovery"),
        canConnect: () => Boolean(rawProfile.value && selectedNode.value && !connected.value && !busy.value),
        tun: connectionMode.value === "tun",
        tunReady: async () => (await refreshTunService()).ready,
        connect: async () => {
          busy.value = true;
          try {
            await apiBridge.saveProfile(buildProfileConfig());
            applySnapshot(await apiBridge.connect(connectionMode.value));
            addActivity(msg("log.connectionState", { state: msg(label.value) }));
          } finally { busy.value = false; }
        },
        report: key => { message.value = key; addActivity(key); },
      });
    } catch (error) { await reconcileConnectionError(error); }
  }
  };

  const teardown = () => {
  stopWindowResize?.(); clearTimeout(windowSaveTimer);
  logsGeneration++; clearTimeout(logsTimer);
  dismissNotice();
  clearInterval(clockTimer);
  automaticGeneration++; clearTimeout(automaticTimer);
  document.removeEventListener("contextmenu", preventContextMenu);
  colorScheme.removeEventListener("change", applyTheme);
  stopListen?.();
  stopProgressListen?.();
  stopTrayListen?.();
  stopDeepLink?.();
  cancelProbes();
  };

let sessionRevision = 0;
const api = createAuthenticatedApi({
  baseUrl: () => baseUrl.value,
  session: () => ({ accessToken: access.value, refreshToken: refreshToken.value, revision: sessionRevision }),
  replaceSession: tokens => { access.value = tokens.accessToken; refreshToken.value = tokens.refreshToken; },
  persistSession,
});

async function login() {
  if (!baseUrl.value) { loginError.value = "error.noServer"; return; }
  busy.value = true;
  loginError.value = "";
  try {
    const data = await api("/api/v1/app/auth/login", {
      method: "POST",
      body: JSON.stringify({ email: email.value, password: password.value }),
    });
    sessionRevision++;
    access.value = data.tokens.accessToken;
    refreshToken.value = data.tokens.refreshToken;
    await persistSession();
    password.value = "";
    dismissNotice();
    addActivity("log.signedIn");
    tab.value = "home";
    await refreshData();
  } catch (error) {
    if (!access.value) loginError.value = failure(error, "error.signInFailed");
    else message.value = failure(error, "error.subscriptionLoadFailed");
  } finally {
    busy.value = false;
  }
}

async function refreshData() {
  if (!access.value) return;
  busy.value = true;
  try {
    const [subs, available] = await Promise.all([api("/api/v1/app/subscriptions"), api("/api/v1/app/nodes")]);
    activeSub.value = subs.find((s: any) => s.status === "active" && s.expiresAt && new Date(s.expiresAt).getTime() > Date.now()) || null;
    nodes.value = available;
    profileKind.value = "airport";
    activeExternalId.value = "";
    addActivity(msg("log.nodesLoaded", { count: available.length, online: available.filter((node: typeof nodes.value[number]) => node.agentOnline).length }));
    cancelProbes(); probeStates.value = {}; latencies.value = {};
    selectedNode.value = cachedNode(nodes.value);
    chain.value = cachedChain(nodes.value);
    chainGroups.value = cachedChainGroups();
    subUrl.value = activeSub.value?.subscriptionUrl || "";
    if (subUrl.value && selectedNode.value) await importSub();
    else if (subUrl.value) { rawProfile.value = ""; message.value = "subscriptions.noReadyNodes"; }
    else { rawProfile.value = ""; message.value = "subscriptions.nonePurchase"; }
  } catch (error) {
    showError(failure(error, "error.loadFailed"));
  } finally { busy.value = false; }
}

async function logout() {
  sessionRevision++;
  busy.value = true;
  try { await (await bridge()).clearProfile(); }
  catch (error) { message.value = msg("error.clearProfileFailed", { error: String(error) }); busy.value = false; return; }
  try { await writeClientSetting("session", { refreshToken: "" }, sessionKey); }
  catch (error) { showError(failure(error, "error.preferencesNotSaved")); busy.value = false; return; }
  access.value = ""; refreshToken.value = ""; password.value = ""; subUrl.value = ""; dismissNotice();
  rawProfile.value = ""; activeSub.value = null; nodes.value = []; selectedNode.value = ""; chain.value = []; chainGroups.value = [];
  cancelProbes(); probeStates.value = {};
  localMode.value = false; activeExternalId.value = ""; profileKind.value = "airport";
  phase.value = "IDLE"; loginError.value = ""; latencies.value = {}; activityLogs.value = []; busy.value = false;
}

const sessionKey = "airport.session";
// Opt-in "remember me": persist only the refresh token (never the password) so a
// relaunch can silently renew the session. Off by default keeps the secure default.
async function persistSession() {
  await writeClientSetting("session", { refreshToken: rememberMe.value ? refreshToken.value : "" }, sessionKey);
}
const acceptSession = (value: unknown): { refreshToken: string } | null => {
  const item = value as { refreshToken?: unknown } | null;
  return typeof item?.refreshToken === "string" && item.refreshToken.length <= 16384 ? { refreshToken: item.refreshToken } : null;
};
async function restoreSession() {
  let saved: { refreshToken: string };
  try {
    saved = await readClientSetting("session", { refreshToken: "" }, acceptSession, {
      key: sessionKey, parse: raw => { try { return acceptSession(JSON.parse(raw)); } catch { return null; } },
    });
  } catch (error) { showError(failure(error, "error.preferencesNotSaved")); return; }
  if (!enableLogin || access.value || activeExternalId.value || !saved.refreshToken || !baseUrl.value) return;
  rememberMe.value = true;
  try {
    const res = await fetch(`${baseUrl.value.replace(/\/$/, "")}/api/v1/app/auth/refresh`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ refreshToken: saved.refreshToken }) });
    if (!res.ok) { await writeClientSetting("session", { refreshToken: "" }, sessionKey); return; }
    const body = await res.json();
    sessionRevision++;
    access.value = body.data.accessToken; refreshToken.value = body.data.refreshToken;
    await persistSession();
    tab.value = "home";
    await refreshData();
  } catch { /* Stay on the login screen after a failed refresh. */ }
}
// Lifting the kill switch is disconnecting: the user chooses to be without the tunnel.
async function releaseGuard() {
  if (busy.value) return;
  busy.value = true;
  try { applySnapshot(await (await bridge()).disconnect()); dismissNotice(); }
  catch (error) { showError(error instanceof Error ? error.message : String(error)); }
  finally { busy.value = false; }
}
function cancelProbes() {
  probeRun++;
  if (testing.value) probeCancelling.value = true;
  const session = activeProbeSession;
  if (session) void bridge().then(client => client.endProbe(session)).catch(() => {});
}

async function testLatencies(only?: string[]) {
  if (testing.value) {
    cancelProbes();
    probeSummary.value = "nodes.stoppingTest";
    return;
  }
  if (!isNative || !rawProfile.value) { message.value = "nodes.testNeedsDesktop"; return; }
  const run = ++probeRun;
  const candidates = nodes.value.filter(node => node.agentOnline && (!only || only.includes(node.id)));
  if (!candidates.length) { probeSummary.value = "nodes.nothingToTest"; return; }
  const external = profileKind.value === "external";
  const tagFor = (id: string) => external ? id : `node-${id.replaceAll("-", "").slice(0, 8)}`;
  testing.value = true; probeCancelling.value = false;
  probeProgress.value = 0; probeTotal.value = candidates.length;
  probeStates.value = Object.fromEntries(candidates.map(node => [node.id, "queued"]));
  latencies.value = {}; probeErrors.value = {}; coldLatencies.value = {}; warmedLatencies.value = {};
  probeSummary.value = "nodes.preparingTest";
  const started = performance.now();
  let session: string | null = null;
  let client: Bridge | null = null;
  try {
    const config = external ? JSON.stringify(parseSingBoxProfile(rawProfile.value).config) : rawProfile.value;
    client = await bridge();
    session = await client.beginProbe(config, candidates.map(node => tagFor(node.id)));
    activeProbeSession = session;
    if (run !== probeRun) return;
    let cursor = 0;
    // Keep at most eight in flight. A finished slot takes the next queued node immediately,
    // so one slow node never holds up a whole batch or blocks interaction with the app.
    const worker = async () => {
      while (run === probeRun && cursor < candidates.length) {
        const node = candidates[cursor++];
        probeStates.value[node.id] = "testing";
        try {
          const result = await client!.probeDelay(session!, tagFor(node.id));
          if (run !== probeRun) return;
          latencies.value[node.id] = result.delay; coldLatencies.value[node.id] = result.coldDelay; warmedLatencies.value[node.id] = result.warmed; probeStates.value[node.id] = "success";
        } catch (error) {
          if (run !== probeRun) return;
          probeStates.value[node.id] = "failed"; probeErrors.value[node.id] = String(error);
        }
        probeProgress.value++;
        probeSummary.value = msg("nodes.testProgress", { done: probeProgress.value, total: candidates.length, count: Object.keys(latencies.value).length });
      }
    };
    await Promise.all(Array.from({ length: Math.min(Math.max(1, Math.min(64, probeConcurrency.value)), candidates.length) }, worker));
    if (run === probeRun) {
      const seconds = ((performance.now() - started) / 1000).toFixed(1);
      const summary = msg("nodes.testSummary", { count: Object.keys(latencies.value).length, total: candidates.length, seconds });
      probeSummary.value = summary;
      addActivity(msg("log.nodeTest", { summary }));
    }
  } catch (error) { if (run === probeRun) probeSummary.value = msg("nodes.testFailed", { error: String(error) }); }
  finally {
    if (session && client) await client.endProbe(session).catch(() => {});
    activeProbeSession = null;
    testing.value = false; probeCancelling.value = false;
    for (const [id, status] of Object.entries(probeStates.value)) {
      if (status === "queued" || status === "testing") delete probeStates.value[id];
    }
  }
}

// Measure the whole chain end to end: apply the detour chain to a probe config, then time a
// single request through the exit outbound — the core routes it entry → … → exit → target,
// so the delay is the real cost of the chain, not any one hop.
async function testChain(ids: string[]) {
  if (chainTesting.value) {
    chainRun++;
    const session = activeProbeSession;
    if (session) void bridge().then(client => client.endProbe(session)).catch(() => {});
    chainTesting.value = false;
    return;
  }
  if (ids.length < 2) return;
  if (testing.value) { message.value = "nodes.testBusy"; return; }
  if (!isNative || !rawProfile.value) { message.value = "nodes.testNeedsDesktop"; return; }
  const external = profileKind.value === "external";
  const tagFor = (id: string) => external ? id : `node-${id.replaceAll("-", "").slice(0, 8)}`;
  const run = ++chainRun;
  chainTesting.value = true; chainDelay.value = null; chainError.value = "";
  let session: string | null = null;
  let client: Bridge | null = null;
  try {
    const cfg: any = external ? parseSingBoxProfile(rawProfile.value).config : JSON.parse(rawProfile.value);
    const tags = ids.map(tagFor);
    const byTag = new Map<string, any>((cfg.outbounds ?? []).map((o: any) => [o.tag, o]));
    if (!tags.every(tag => byTag.has(tag))) throw new Error("native.probeNodeNotFound");
    for (let i = 1; i < tags.length; i++) byTag.get(tags[i]).detour = tags[i - 1];
    const exitTag = tags[tags.length - 1];
    client = await bridge();
    session = await client.beginProbe(JSON.stringify(cfg), [exitTag]);
    activeProbeSession = session;
    if (run !== chainRun) return;
    const result = await client.probeDelay(session, exitTag);
    if (run !== chainRun) return;
    chainDelay.value = result.delay;
  } catch (error) {
    if (run === chainRun) chainError.value = String(error);
  } finally {
    if (session && client) await client.endProbe(session).catch(() => {});
    if (activeProbeSession === session) activeProbeSession = null;
    if (run === chainRun) chainTesting.value = false;
  }
}
// A past end-to-end result no longer describes an edited chain; drop it when the draft changes.
function clearChainTest() { chainDelay.value = null; chainError.value = ""; }

async function windowAction(action: "minimize" | "toggleMaximize" | "close") {
  if (!("__TAURI_INTERNALS__" in window)) { message.value = "error.windowControlsDesktopOnly"; return; }
  try { const { getCurrentWindow } = await import("@tauri-apps/api/window"); await getCurrentWindow()[action](); }
  catch (error) { message.value = msg("error.windowActionFailed", { error: String(error) }); }
}

async function copySubscription() {
  const url = subUrl.value;
  if (!url || !await confirmation.request({ title: "confirmation.exposeTitle", body: "confirmation.exposeBody", confirmLabel: "subscriptions.copySubscriptionUrl" })) return;
  if (subUrl.value !== url) return;
  try { await navigator.clipboard.writeText(url); message.value = "subscriptions.urlCopied"; }
  catch { message.value = "error.copyFailed"; }
}

async function checkUpdates() {
  if (checkingUpdates.value || installingUpdate.value) return;
  checkingUpdates.value = true;
  updateStatus.value = "";
  updateOffer.value = null;
  pendingUpdate = null;
  try {
    if (!isNative) {
      updateStatus.value = "settings.updateDesktopOnly";
      return;
    }
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) {
      updateStatus.value = "settings.updateUpToDate";
      return;
    }
    pendingUpdate = update;
    updateOffer.value = { version: update.version, notes: update.body ?? "" };
  } catch { showError("error.updateCheckFailed"); }
  finally { checkingUpdates.value = false; }
}

async function installUpdate() {
  if (!pendingUpdate || installingUpdate.value) return;
  installingUpdate.value = true;
  if (!await confirmation.request({ title: "confirmation.updateTitle", body: msg("confirmation.updateBody", { version: updateOffer.value?.version || installedVersion.value }), confirmLabel: "settings.installUpdate" })) {
    installingUpdate.value = false;
    return;
  }
  updateStatus.value = "settings.installingUpdate";
  try {
    await pendingUpdate.downloadAndInstall();
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch {
    updateStatus.value = "";
    showError("error.updateInstallFailed");
  } finally { installingUpdate.value = false; }
}

function enterLocal() {
  localMode.value = true;
  tab.value = "sub";
  message.value = "subscriptions.addPrompt";
}

async function normalizeExternalRaw(raw: string): Promise<string> {
  try { parseSingBoxProfile(raw); return raw; }
  catch { const converted = await (await bridge()).convertSubscription(raw); parseSingBoxProfile(converted); return converted; }
}

async function persistExternal(profiles = externalProfiles.value) {
  await (await bridge()).saveSubscriptions(JSON.stringify(profiles));
}

async function activateExternal(profile: ExternalProfile) {
  const parsed = parseSingBoxProfile(profile.raw);
  const client = await bridge();
  if (connected.value) applySnapshot(await client.disconnect());
  localMode.value = true;
  profileKind.value = "external";
  activeExternalId.value = profile.id;
  rawProfile.value = profile.raw;
  subUrl.value = profile.url;
  activeSub.value = null;
  nodes.value = parsed.nodes;
  selectedNode.value = cachedNode(parsed.nodes);
  chain.value = cachedChain(parsed.nodes);
  chainGroups.value = cachedChainGroups();
  latencies.value = {}; probeStates.value = {}; probeSummary.value = ""; cancelProbes();
  await client.saveProfile(buildProfileConfig());
  message.value = msg("subscriptions.nowUsing", { name: profile.name });
  addActivity(msg("log.subscriptionSwitched", { name: profile.name }));
}

async function selectProfile(profile: ExternalProfile) {
  if (busy.value) return;
  busy.value = true;
  try {
    if (connected.value && !await confirmation.request({ title: "confirmation.switchTitle", body: msg("confirmation.switchBody", { name: profile.name }), confirmLabel: "subscriptions.useSubscription" })) return;
    await activateExternal(profile);
  }
  catch (error) { message.value = msg("error.subscriptionSwitchFailed", { error: String(error) }); }
  finally { busy.value = false; }
}
function resetProfileDialog() {
  newProfileName.value = ""; newProfileUrl.value = ""; selectedFile.value = null; profileError.value = "";
  profileDialog.value?.querySelector("form")?.reset();
}
async function openProfileDialog() {
  profileError.value = "";
  profileDialog.value?.showModal();
  await nextTick(); profileNameInput.value?.focus();
}
function closeProfileDialog() { if (!busy.value) profileDialog.value?.close(); }

async function revealMain() {
  if (!isNative) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const win = getCurrentWindow();
  await win.show();
  await win.unminimize();
  await win.setFocus();
}

async function acceptDeepLinks(urls: string[] | null) {
  if (!urls?.length) return;
  await revealMain();
  for (const raw of urls) {
    const link = parseDeepLink(raw);
    if (!link) continue;
    if (link.kind === "nodes" || link.kind === "settings") tab.value = link.kind;
    else if (link.kind === "connect") tab.value = "connections";
    else if (link.kind === "rejected") showError("deepLink.rejected");
    else if (link.kind === "import") { importError.value = ""; tab.value = "sub"; importRequest.value = { code: link.code }; }
    else if (link.kind === "importUrl") { importError.value = ""; tab.value = "sub"; importRequest.value = { url: link.url, name: link.name }; }
  }
}

function dismissImport() {
  if (busy.value) return;
  importRequest.value = null;
  importError.value = "";
}

async function confirmImport() {
  const request = importRequest.value;
  if (!request || busy.value) return;
  if (!("url" in request) && !baseUrl.value) { importError.value = "error.noServer"; return; }
  importError.value = "";
  try {
    if ("url" in request) {
      // The clash:// link carries the address directly; no code to redeem.
      newProfileName.value = request.name || t("deepLink.importedName");
      newProfileUrl.value = request.url;
    } else {
      busy.value = true;
      const redeemed = redeemedSubscription(await api("/api/v1/app/subscription-import-codes/redeem", { method: "POST", body: JSON.stringify({ code: request.code }) }));
      busy.value = false;
      if (!redeemed) throw new Error("deepLink.importFailed");
      newProfileName.value = redeemed.name || t("deepLink.importedName");
      newProfileUrl.value = redeemed.url;
    }
    selectedFile.value = null;
    await addExternal();
    if (profileError.value) {
      newProfileName.value = "";
      newProfileUrl.value = "";
      showError("deepLink.importFailed");
    }
  } catch { showError("deepLink.importFailed"); }
  finally { busy.value = false; importRequest.value = null; }
}
watch(importRequest, async value => {
  await nextTick();
  const dialog = importDialog.value;
  if (!dialog) return;
  if (value) { if (!dialog.open) dialog.showModal(); }
  else if (dialog.open) dialog.close();
});

async function addExternal() {
  if (busy.value) return;
  const name = newProfileName.value.trim();
  const url = newProfileUrl.value.trim();
  if (!name || (!url && !selectedFile.value)) { profileError.value = "subscriptions.formIncomplete"; return; }
  busy.value = true; profileError.value = "";
  try {
    // Deep-link imports already have their own explicit confirmation dialog.
    if (connected.value && !importRequest.value && !await confirmation.request({ title: "confirmation.switchTitle", body: msg("confirmation.switchBody", { name }), confirmLabel: "app.importUse" })) return;
    const downloaded: SubscriptionDownload = selectedFile.value ? { raw: await selectedFile.value.text(), userinfo: null } : await (await bridge()).fetchSubscriptionDetails(url);
    const raw = await normalizeExternalRaw(downloaded.raw);
    const profile = { id: crypto.randomUUID(), name, url: selectedFile.value ? "" : url, raw, updatedAt: new Date().toISOString(), usage: parseSubscriptionUsage(downloaded.userinfo) };
    const profiles = [...externalProfiles.value, profile];
    await persistExternal(profiles);
    externalProfiles.value = profiles;
    newProfileName.value = ""; newProfileUrl.value = ""; selectedFile.value = null;
    await activateExternal(profile);
    profileDialog.value?.close();
    tab.value = "sub";
  } catch (error) { profileError.value = error instanceof Error ? error.message : String(error); }
  finally { busy.value = false; }
}

function chooseFile(event: Event) { selectedFile.value = (event.target as HTMLInputElement).files?.[0] ?? null; }

async function refreshExternal(profile: ExternalProfile) {
  if (!profile.url || busy.value) return;
  busy.value = true;
  try {
    if (connected.value && activeExternalId.value === profile.id && !await confirmation.request({ title: "confirmation.refreshTitle", body: "confirmation.refreshBody", confirmLabel: "subscriptions.updateSubscription" })) return;
    const downloaded = await (await bridge()).fetchSubscriptionDetails(profile.url);
    const raw = await normalizeExternalRaw(downloaded.raw);
    const updated = { ...profile, raw, usage: parseSubscriptionUsage(downloaded.userinfo), updatedAt: new Date().toISOString() };
    const profiles = externalProfiles.value.map(item => item.id === profile.id ? updated : item);
    await persistExternal(profiles);
    externalProfiles.value = profiles;
    if (activeExternalId.value === profile.id) await activateExternal(updated);
    message.value = downloaded.metadataSupported === false ? "subscriptions.updatedRestartForUsage" : msg("subscriptions.updatedNamed", { name: profile.name });
  } catch (error) { message.value = error instanceof Error ? error.message : String(error); }
  finally { busy.value = false; }
}

async function removeExternal(profile: ExternalProfile) {
  if (busy.value) return;
  busy.value = true;
  try {
    if (!await confirmation.request({
      title: "confirmation.removeTitle", body: msg("confirmation.removeBody", { name: profile.name }),
      confirmLabel: "subscriptions.removeSubscription", danger: true,
      details: activeExternalId.value === profile.id ? ["confirmation.activeSubscription"] : [],
    })) return;
    const client = await bridge();
    if (activeExternalId.value === profile.id) {
      if (connected.value) applySnapshot(await client.disconnect());
      await client.clearProfile();
      cancelProbes(); probeStates.value = {};
      activeExternalId.value = ""; rawProfile.value = ""; nodes.value = []; selectedNode.value = ""; chain.value = []; chainGroups.value = [];
      profileKind.value = "airport";
    }
    const remaining = externalProfiles.value.filter(item => item.id !== profile.id);
    await client.saveSubscriptions(JSON.stringify(remaining));
    externalProfiles.value = remaining;
    delete selectedNodes[profile.id];
    persistPreferences();
  } catch (error) {
    message.value = t("error.subscriptionRemoveFailed", { error: String(error) });
  } finally {
    busy.value = false;
  }
}

async function refreshLogs() {
  if (logsLoading.value) return;
  logsLoading.value = true;
  try { logs.value = await (await bridge()).logs(); logsError.value = ''; }
  catch (error) { logsError.value = error instanceof Error ? error.message : String(error); throw error; }
  finally { logsLoading.value = false; }
}

async function importSub() {
  if (!subUrl.value) return;
  busy.value = true;
  try {
    rawProfile.value = await (await bridge()).fetchSubscription(subUrl.value);
    await (await bridge()).saveProfile(buildProfileConfig());
    message.value = "subscriptions.imported";
    addActivity("log.subscriptionImported");
    logs.value = await (await bridge()).logs();
  } catch (error) {
    message.value = failure(error, "error.importFailed");
  } finally {
    busy.value = false;
  }
}

// Rebuild the profile from the current selection/chain and push it to the core,
// bouncing a live connection so the new config (e.g. chain `detour`) takes hold.
async function reapplyProfile() {
  const next = buildProfileConfig();
  const client = await bridge();
  const wasConnected = connected.value;
  if (wasConnected) applySnapshot(await client.disconnect());
  await client.saveProfile(next);
  if (wasConnected) applySnapshot(await client.connect(connectionMode.value));
  logs.value = await client.logs();
}

async function applySelectedNode() {
  if (!rawProfile.value || !selectedNode.value) {
    message.value = "subscriptions.importFirst";
    return;
  }
  const node = selectedNode.value === AUTOMATIC_NODE_ID ? { name: t("nodes.automatic") } : nodes.value.find((n) => n.id === selectedNode.value);
  if (!node) return;
  try {
    await reapplyProfile();
    message.value = msg("nodes.switchedTo", { name: node.name });
    addActivity(msg("log.nodeSwitched", { name: node.name }));
  } catch (error) {
    await reconcileConnectionError(error);
  }
}

// Replace the proxy chain (ordered entry → exit) and apply it live.
async function commitChain(ids: string[]) {
  // The exit becomes the chosen node first, so saving the profile does not
  // treat this chain as stale and throw it away.
  if (ids.length >= 2) selectedNode.value = ids[ids.length - 1];
  chain.value = [...ids];
  if (!rawProfile.value) return;
  // A chain needs two hops to egress; when cleared, fall back to the selected node.
  if (!chainActive.value && !selectedNode.value) return;
  try {
    await reapplyProfile();
    message.value = chainActive.value ? "nodes.chainApplied" : "nodes.chainCleared";
    addActivity(chainActive.value ? msg("log.chainApplied", { chain: chainSummary.value }) : "log.chainCleared");
  } catch (error) {
    await reconcileConnectionError(error);
  }
}

// Create or update a saved chain preset (链路组); returns the group id.
function saveChainGroup(name: string, groupNodes: string[], id?: string): string {
  const gid = id && chainGroups.value.some(g => g.id === id) ? id : `g${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  const existing = chainGroups.value.find(g => g.id === gid);
  if (existing) { existing.name = name; existing.nodes = [...groupNodes]; }
  else chainGroups.value.push({ id: gid, name, nodes: [...groupNodes] });
  chainGroups.value = [...chainGroups.value];
  persistPreferences();
  return gid;
}
function deleteChainGroup(id: string) {
  chainGroups.value = chainGroups.value.filter(g => g.id !== id);
  persistPreferences();
}

// Wire the active chain into [cfg] so each hop dials through the previous one
// via sing-box `detour` (entry → … → exit). Returns the exit node id to select,
// or null when no valid chain applies (fewer than two hops, or a hop missing
// from the profile's outbounds — in which case [cfg] is left untouched).
function applyChain(cfg: any, external: boolean): string | null {
  if (chain.value.length < 2) return null;
  const tags = chain.value.map((id) => outboundTag(id, external));
  const byTag = new Map<string, any>((cfg.outbounds ?? []).map((o: any) => [o.tag, o]));
  if (!tags.every((tag) => byTag.has(tag))) return null;
  for (let i = 1; i < tags.length; i++) byTag.get(tags[i]).detour = tags[i - 1];
  return chain.value[chain.value.length - 1];
}

function buildProfileConfig() {
  const cfg: any = profileKind.value === "external" ? parseSingBoxProfile(rawProfile.value).config : JSON.parse(rawProfile.value);
  // External profiles may expose a local Clash API or other experimental listener.
  // This client owns its inbounds and never carries such listeners into the runtime config.
  delete cfg.experimental;
  delete cfg.services;
  const external = profileKind.value === "external";
  // A node chosen after the chain was applied is the one that must connect.
  // Leaving the old detours in place dials the chain and fails the link check.
  if (chain.value.length && activeChain(chain.value, selectedNode.value).length === 0) chain.value = [];
  const chainExit = applyChain(cfg, external);
  configureSelection(cfg, chainExit ?? selectedNode.value, nodes.value, external);
  cfg.route ??= {};
  cfg.route.final = "select";
  // Keep the built-in policy small: LAN and 500 domestic service/resource domains.
  // The TUN DNS mirrors these direct domains; unmatched traffic uses the proxy.
  cfg.route.rules = routeMode.value === "rule" ? [...builtinRouteRules(true), ...(profileKind.value === "external" && Array.isArray(cfg.route.rules) ? cfg.route.rules : [])] : [];
  // 进程级策略:按应用注入 process 规则,放在最前,优先于默认与中国直连规则。
  cfg.route.rules = [...appPolicyRules(appPolicies.value), ...cfg.route.rules];
  cfg.inbounds = [{ type: "mixed", tag: "mixed-in", listen: networkSettings.value.allowLan ? "0.0.0.0" : "127.0.0.1", listen_port: networkSettings.value.proxyPort }];
  // Rust owns TUN, DNS and permissions; imported profiles cannot create a tunnel.
  return JSON.stringify(cfg);
}

async function reconcileConnectionError(error: unknown) {
  try { applySnapshot(await (await bridge()).state()); }
  catch (stateError) { addActivity(msg("log.stateReadFailed", { error: String(stateError) })); }
  showError(error instanceof Error ? error.message : String(error));
}

async function refreshProxyDiagnostics() {
  if (diagnosticBusy.value) return;
  diagnosticBusy.value = true;
  try { proxyDiagnostic.value = await (await bridge()).proxyDiagnostics(); }
  catch (error) { proxyDiagnostic.value = String(error); }
  finally { diagnosticBusy.value = false; }
}

async function saveNetworkSettings() {
  if (busy.value || connected.value || !networkLoaded.value) return;
  networkError.value = "";
  busy.value = true;
  try {
    const settings: NetworkSettings = {
      proxyPort: validateProxyPort(draftProxyPort.value), allowLan: draftAllowLan.value,
      tun: { ...draftTun.value, mtu: validateTunMtu(draftTunMtu.value), routeExclude: parseRouteExclude(draftTunExclude.value) },
    };
    if (!await confirmation.request({
      title: "confirmation.networkTitle", body: "confirmation.networkBody", confirmLabel: "settings.saveNetworkSettings",
      details: [
        `${t("settings.proxyPort")}: ${settings.proxyPort}`,
        ...(settings.allowLan && !networkSettings.value.allowLan ? ["confirmation.lanWarning"] : []),
        ...(!settings.tun.killSwitch && networkSettings.value.tun.killSwitch ? ["confirmation.guardWarning"] : []),
        ...(!settings.tun.strictRoute || settings.tun.routeExclude.some(prefix => !networkSettings.value.tun.routeExclude.includes(prefix)) ? ["confirmation.routeWarning"] : []),
      ],
    })) return;
    applyNetworkSettings(await (await bridge()).saveNetworkSettings(settings));
    await writeStored(settingsStore, networkSettingsStoreKey, networkSettings.value, preferenceStoreStatus);
    message.value = "settings.networkSaved";
  } catch (error) { networkError.value = error instanceof Error ? error.message : String(error); }
  finally { busy.value = false; }
}

async function setNetworkSecurity(patch: Partial<NetworkSecurity>) {
  const next = { ...networkSecurity.value, ...patch };
  await writeStored(settingsStore, networkSecurityStoreKey, next, preferenceStoreStatus);
  networkSecurity.value = next;
}

// 进程级网络管理:记录某应用的路由策略。inherit(跟随规则)是默认值,不落库。
async function setAppPolicy(app: string, policy: AppPolicy) {
  if (!app) return;
  const next = { ...appPolicies.value };
  if (policy === "inherit") delete next[app];
  else next[app] = policy;
  await writeStored(settingsStore, appPoliciesStoreKey, next, preferenceStoreStatus);
  appPolicies.value = next;
}

// Kill Switch 与 IPv6 的实际生效落在既有 TUN 设置;连接中不可更改(与设置页一致)。
async function setTunProtection(patch: Partial<Pick<TunSettings, "killSwitch" | "ipv6">>) {
  if (busy.value || connected.value || !networkLoaded.value || connectionMode.value !== "tun") return;
  busy.value = true;
  networkError.value = "";
  try {
    if (!await confirmation.request({
      title: "confirmation.networkTitle", body: "confirmation.networkBody",
      details: patch.killSwitch === false ? ["confirmation.guardWarning"] : [],
    })) return;
    const settings: NetworkSettings = { ...networkSettings.value, tun: { ...networkSettings.value.tun, ...patch } };
    applyNetworkSettings(await (await bridge()).saveNetworkSettings(settings));
    await writeStored(settingsStore, networkSettingsStoreKey, networkSettings.value, preferenceStoreStatus);
  } catch (error) { networkError.value = error instanceof Error ? error.message : String(error); }
  finally { busy.value = false; }
}

/** Explicit resync, unlike selecting the already-selected mode (which is a no-op). */
async function repairNetwork() {
  if (busy.value || tunServiceBusy.value) return false;
  busy.value = true;
  try {
    if (!await confirmation.request({ title: "secnet.repairTitle", body: "check.reconnectConfirm" })) return false;
    const client = await bridge();
    const reconnect = connected.value;
    applySnapshot(await client.disconnect());
    await client.setConnectionMode(connectionMode.value);
    if (rawProfile.value) await client.saveProfile(buildProfileConfig());
    if (reconnect) applySnapshot(await client.connect(connectionMode.value));
    logs.value = await client.logs();
    return true;
  } catch (error) {
    await reconcileConnectionError(error);
    return false;
  } finally { busy.value = false; }
}

async function toggleAutostart() {
  if (!autostartReady.value || autostartBusy.value) return;
  autostartBusy.value = true;
  autostartError.value = "";
  try {
    if (!await confirmation.request({ title: "confirmation.startupTitle", body: autostartEnabled.value ? "confirmation.startupDisable" : "confirmation.startupEnable" })) return;
    const plugin = await import("@tauri-apps/plugin-autostart");
    if (autostartEnabled.value) await plugin.disable();
    else await plugin.enable();
    autostartEnabled.value = await plugin.isEnabled();
  } catch (error) { autostartError.value = String(error); }
  finally { autostartBusy.value = false; }
}

async function updateMode() {
  const client = await bridge();
  if (!rawProfile.value) { await client.setConnectionMode(connectionMode.value); return; }
  const wasConnected = connected.value;
  if (wasConnected) applySnapshot(await client.disconnect());
  await client.setConnectionMode(connectionMode.value);
  await client.saveProfile(buildProfileConfig());
  if (wasConnected) applySnapshot(await client.connect(connectionMode.value));
  logs.value = await client.logs();
}
async function setConnectionMode(value: "system" | "tun" | "manual") {
  if (connectionMode.value === value || busy.value || tunServiceBusy.value) return;
  busy.value = true;
  const previous = connectionMode.value;
  const reconnect = connected.value;
  try {
    if (value === "tun") {
      if (connected.value) applySnapshot(await (await bridge()).disconnect());
      await ensureTunService(locale.value);
    }
    connectionMode.value = value;
    await updateMode();
    if (value === "tun" && reconnect) applySnapshot(await (await bridge()).connect(value));
    addActivity(msg("log.connectionMode", { mode: value === "tun" ? "TUN" : msg(value === "manual" ? "settings.manualProxy" : "common.systemProxy") }));
  } catch (error) {
    connectionMode.value = previous;
    await reconcileConnectionError(error);
  } finally { busy.value = false; }
}
async function setRouteMode(value: "rule" | "global") {
  if (routeMode.value === value || busy.value) return;
  busy.value = true;
  routeMode.value = value;
  try { await updateMode(); addActivity(msg("log.routeMode", { mode: msg(value === "rule" ? "common.rule" : "overview.global") })); }
  catch (error) { await reconcileConnectionError(error); }
  finally { busy.value = false; }
}

async function selectNode(id: string) {
  if (busy.value) return;
  if (id === AUTOMATIC_NODE_ID ? !nodes.value.some(node => node.agentOnline) : !nodes.value.find(node => node.id === id)?.agentOnline) { message.value = "nodes.notReady"; return; }
  busy.value = true;
  try {
    selectedNode.value = id;
    if (chain.value.length) chain.value = [];
    await applySelectedNode();
  }
  finally { busy.value = false; }
}

async function toggle() {
  if (busy.value || tunServiceBusy.value) return;
  // Keep the user's intent even if a native disconnect/crash event arrives
  // while awaiting IPC or the port confirmation dialog.
  const disconnecting = connected.value;
  busy.value = true;
  connectionAction.value = disconnecting ? "disconnect" : "connect";
  connectionProgress.value = disconnecting ? "status.disconnecting" : "startup.preparing";
  dismissNotice();
  try {
    const apiBridge = await bridge();
    if (!disconnecting) {
      // Only profile/service readiness gates connecting. Notification consent must not.
      await prepareConnection({
        prepareNotifications,
        ensureTunnel: () => connectionMode.value === "tun" ? ensureTunService(locale.value) : Promise.resolve(),
        saveProfile: () => apiBridge.saveProfile(buildProfileConfig()),
      });
      if (!await resolveProxyPort({
        inspect: () => apiBridge.inspectProxyPort(),
        release: token => apiBridge.releaseProxyPort(token),
        confirm: async conflict => {
          await revealMain();
          return confirmation.request({
            title: "confirmation.portTitle", body: msg("confirmation.portBody", { port: conflict.port }),
            confirmLabel: "confirmation.portRelease", danger: true,
            details: conflict.listeners.map(process => `${process.name} · PID ${process.pid}`),
          });
        },
      })) return;
    }
    const snapshot = disconnecting ? await apiBridge.disconnect() : await apiBridge.connect(connectionMode.value);
    applySnapshot(snapshot);
    addActivity(msg("log.connectionState", { state: msg(label.value) }));
    logs.value = snapshot.logs || (await apiBridge.logs());
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    try { applySnapshot(await (await bridge()).state()); }
    catch { /* Keep the original connection error visible. */ }
    showError(detail);
  } finally {
    connectionAction.value = null;
    connectionProgress.value = "";
    busy.value = false;
    void refreshProxyDiagnostics();
  }
}
  return { connectionAction,connectionProgress,logsLoading,logsError,renderLogLine,enableLogin,githubUrl,openExternal,locale,regionCode,latencyTone,phaseTone,statusChipText,selectedRegionLabel,selectedLatencyText,connectionDuration,rememberMe,guardBlocking,releaseGuard,access,accountMenu,activateExternal,activeExternalId,activeProbeSession,activeSub,activityLogs,addActivity,addExternal,airportUsage,allLogs,api,applySelectedNode,applySnapshot,applyTheme,automaticGeneration,automaticNodeName,automaticSelection,automaticTimer,autostartBusy,autostartEnabled,autostartError,autostartReady,updateOffer,updateStatus,installingUpdate,installUpdate,baseUrl,bridge,browserBridge,buildProfileConfig,busy,cacheReady,cachedNode,chain,chainActive,chainNames,chainSummary,chainGroups,commitChain,saveChainGroup,deleteChainGroup,testChain,clearChainTest,chainTesting,chainDelay,chainError,cancelProbes,checkUpdates,checkingUpdates,chooseFile,closeProfileDialog,importDialog,importError,importTarget,confirmImport,dismissImport,coldLatencies,colorScheme,connected,connectionMode,connectionNodeNames,copySubscription,currentExternal,currentPlatform,diagnosticBusy,draftAllowLan,draftProxyPort,draftTun,draftTunMtu,draftTunExclude,email,enterLocal,externalProfiles,fmt,followLogs,importSub,installedVersion,isLinux,isMac,isNative,label,latencies,latencyLabel,localMode,localProxyAddress,logLine,logViewport,login,loginError,logout,logs,logsGeneration,logsText,logsTimer,mainContent,message,navigation,networkDirty,networkError,networkLoaded,networkSettings,networkSecurity,setNetworkSecurity,setTunProtection,repairNetwork,appPolicies,setAppPolicy,reapplyProfile,newProfileName,newProfileUrl,nodeSort,nodes,normalizeExternalRaw,onLogScroll,openProfileDialog,password,persistExternal,persistPreferences,phase,pollLogs,preventContextMenu,previewBridge,probeCancelling,probeErrors,probeProgress,probeConcurrency,probeRun,probeStates,probeSummary,probeTotal,profileCacheKey,profileDialog,profileError,profileKind,profileLabel,profileNameInput,proxyDiagnostic,rawProfile,reconcileConnectionError,refreshAutomaticSelection,refreshData,refreshExternal,refreshLogs,refreshProxyDiagnostics,refreshToken,remainingTraffic,removeExternal,resetProfileDialog,routeMode,saveNetworkSettings,scrollLogs,selectNode,selectProfile,selectedFile,selectedNode,selectedNodeName,selectedNodes,setConnectionMode,setRouteMode,showTraySpeed,sortedNodes,stopListen,stopTrayListen,subUrl,syncTray,t,tx,tab,testLatencies,testing,theme,title,toggle,toggleAutostart,trafficLabel,trayAction,updateMode,usagePercent,warmedLatencies,windowAction,bootstrap,teardown,AUTOMATIC_NODE_ID,subscriptionHost };
}
export type AppStore = ReturnType<typeof create>;
let store: AppStore | null = null;
export function useApp(): AppStore { return (store ??= create()); }
