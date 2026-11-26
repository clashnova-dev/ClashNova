import { reduce, type Phase } from "./connection";
import { DEFAULT_PROXY_PORT, defaultTunSettings, type NetworkSettings } from "./network-settings";
import { type SubscriptionDownload } from "./subscription-info";
import type {
  LanLocalDevice,
  LanDiscoveredDevice,
  LanTransferTask,
  LanHistoryEntry,
  LanSettings,
  LanIncomingRequest,
  LanProgress,
  LanTrustedDevice,
  LanPairRequest,
} from "./lan-transfer";
import { DEFAULT_LAN_PORT } from "./lan-transfer";
import { writeClientSetting } from "./settings-store";
import type { Bridge, Snapshot, BridgeDeps } from "../types/bridge";

// bridge: transport-layer abstraction over the core. On desktop it reaches the Rust core directly via Tauri IPC (invoke/listen),
// while the browser preview environment is simulated by browserBridge using in-memory state; both implement the same Bridge interface.
// Pure transport, no business state; it only needs the connection store to inject the appPolicies / logLine dependencies.

export function createBridge(deps: BridgeDeps) {
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
        releaseProxyPort: token => invoke("release_proxy_port", { token }),
        setApplicationPolicy: (app, policy) => invoke("set_application_policy", { app, policy }),
        loadNetworkSettings: () => invoke("load_network_settings"),
        saveNetworkSettings: settings => invoke("save_network_settings", { settings }),
        saveProfile: config => invoke("save_profile", { config }),
        clearProfile: () => invoke("clear_profile"),
        setConnectionMode: mode => invoke("set_connection_mode", { mode }),
        probeNode: (host, port) => invoke("probe_node", { host, port }),
        beginProbe: (config, tags) => invoke("begin_probe", { config, tags }),
        probeDelay: (session, tag) => invoke("probe_delay", { session, tag }),
        probeSpeed: (session, tag) => invoke("probe_speed", { session, tag }),
        probeSpeedUp: (session, tag) => invoke("probe_speed_up", { session, tag }),
        async onSpeedProgress(cb) {
          return listen<{ mbps: number; bytes: number }>("probe://speed", e => cb(e.payload));
        },
        async onSpeedUpProgress(cb) {
          return listen<{ mbps: number; bytes: number }>("probe://speed-up", e => cb(e.payload));
        },
        endProbe: session => invoke("end_probe", { session }),
        fetchSubscription: url => invoke("fetch_subscription", { url }),
        async fetchSubscriptionDetails(url) {
          try {
            return await invoke<SubscriptionDownload>("fetch_subscription_details", { url });
          } catch (error) {
            // Development hot reload can update Vue before the native binary restarts.
            if (!/command[^\n]*not found/i.test(String(error))) throw error;
            return {
              raw: await invoke<string>("fetch_subscription", { url }),
              userinfo: null,
              metadataSupported: false,
            };
          }
        },
        convertSubscription: raw => invoke("convert_subscription", { raw }),
        loadSubscriptions: () => invoke("load_subscriptions"),
        saveSubscriptions: data => invoke("save_subscriptions", { data }),
        connect: mode => invoke("connect", { mode }),
        disconnect: () => invoke("disconnect"),
        lanGetLocalDevice: () => invoke("lan_get_local_device"),
        lanAddManualDevice: addr => invoke("lan_add_manual_device", { addr }),
        lanGetSettings: () => invoke("lan_get_settings"),
        lanUpdateSettings: settings => invoke("lan_update_settings", { settings }),
        lanRefreshDevices: () => invoke("lan_refresh_devices"),
        lanListTransfers: () => invoke("lan_list_transfers"),
        lanSendFiles: (fingerprint, paths) => invoke("lan_send_files", { fingerprint, paths }),
        lanSendText: (fingerprint, text) => invoke("lan_send_text", { fingerprint, text }),
        lanRespondIncoming: (taskId, accept, saveDir) =>
          invoke("lan_respond_incoming", {
            task: taskId,
            accept,
            dir: saveDir,
          }),
        lanPair: fingerprint => invoke("lan_pair", { fingerprint }),
        lanRespondPair: (requestId, accept) => invoke("lan_respond_pair", { request: requestId, accept }),
        lanListTrusted: () => invoke("lan_list_trusted"),
        lanUnpair: fingerprint => invoke("lan_unpair", { fingerprint }),
        lanGetConversation: fingerprint => invoke("lan_get_conversation", { fingerprint }),
        lanOpenPath: path => invoke("lan_open_path", { path }),
        lanRevealPath: path => invoke("lan_reveal_path", { path }),
        lanDeleteFile: (id, path) => invoke("lan_delete_file", { id, path }),
        lanCopyFileToClipboard: path => invoke("lan_copy_file_to_clipboard", { path }),
        lanClipboardFiles: () => invoke("lan_clipboard_files"),
        lanPauseTransfer: taskId => invoke("lan_pause_transfer", { task: taskId }),
        lanResumeTransfer: taskId => invoke("lan_resume_transfer", { task: taskId }),
        lanCancelTransfer: taskId => invoke("lan_cancel_transfer", { task: taskId }),
        lanGetHistory: () => invoke("lan_get_history"),
        lanClearHistory: () => invoke("lan_clear_history"),
        lanStart: () => invoke("lan_start"),
        lanStop: () => invoke("lan_stop"),
        lanPickFiles: () => invoke("lan_pick_files"),
        lanPickSaveDirectory: () => invoke("lan_pick_save_directory"),
        async onLanText(cb) {
          return listen<LanHistoryEntry>("lan://text", e => cb(e.payload));
        },
        async onLanDevices(cb) {
          return listen<LanDiscoveredDevice[]>("lan://devices", e => cb(e.payload));
        },
        async onLanIncoming(cb) {
          return listen<LanIncomingRequest>("lan://incoming", e => cb(e.payload));
        },
        async onLanTransfer(cb) {
          return listen<LanTransferTask>("lan://transfer", e => cb(e.payload));
        },
        async onLanProgress(cb) {
          return listen<LanProgress>("lan://progress", e => cb(e.payload));
        },
        async onLanPair(cb) {
          return listen<LanPairRequest>("lan://pair", e => cb(e.payload));
        },
        async onProgress(cb) {
          return listen<{ stage: string }>("connection://progress", event => cb(event.payload.stage));
        },
        async onState(cb) {
          const un = await listen<Snapshot>("connection://state", e => cb(e.payload));
          return () => {
            un();
          };
        },
      };
    }
    return (previewBridge ??= browserBridge());
  }

  function browserBridge(): Bridge {
    let previewSettings: NetworkSettings = {
      proxyPort: DEFAULT_PROXY_PORT,
      allowLan: false,
      tun: defaultTunSettings(),
    };
    let current: Phase = "IDLE";
    let note = "preview.noCore";
    let lines: string[] = [];
    return {
      async lanGetLocalDevice() {
        return {
          alias: "ClashNova",
          fingerprint: "",
          deviceType: "desktop",
          port: DEFAULT_LAN_PORT,
          ip: null,
        };
      },
      async lanAddManualDevice(addr) {
        return {
          alias: addr,
          fingerprint: addr,
          address: addr,
          port: DEFAULT_LAN_PORT,
          deviceType: "desktop",
          trusted: false,
        };
      },
      async lanGetSettings() {
        return {
          alias: "ClashNova",
          discoverable: true,
          acceptIncoming: true,
          autoAcceptTrusted: false,
          defaultSaveDir: "",
          maxConcurrent: 2,
        };
      },
      async lanUpdateSettings(settings) {
        return settings;
      },
      async lanRefreshDevices() {},
      async lanListTransfers() {
        return [];
      },
      async lanSendFiles() {
        return [];
      },
      async lanSendText() {
        throw new Error("native.lanNotImplemented");
      },
      async lanRespondIncoming() {},
      async lanPair() {},
      async lanRespondPair() {},
      async lanListTrusted() {
        return [];
      },
      async lanUnpair() {},
      async lanGetConversation() {
        return [];
      },
      async lanOpenPath() {},
      async lanRevealPath() {},
      async lanDeleteFile() {},
      async lanCopyFileToClipboard() {},
      async lanClipboardFiles() {
        return [];
      },
      async lanPauseTransfer() {},
      async lanResumeTransfer() {},
      async lanCancelTransfer() {},
      async lanGetHistory() {
        return [];
      },
      async lanClearHistory() {},
      async lanStart() {},
      async lanStop() {},
      async lanPickFiles() {
        return [];
      },
      async lanPickSaveDirectory() {
        return null;
      },
      async proxyDiagnostics() {
        return "settings.proxyCheckDesktopOnly";
      },
      async inspectProxyPort() {
        return null;
      },
      async releaseProxyPort() {},
      async setApplicationPolicy(app, policy) {
        const policies = { ...deps.appPolicies() };
        if (policy === "inherit") delete policies[app];
        else policies[app] = policy;
        await writeClientSetting("appPolicies", policies);
        return { policies, applied: false, closed: 0 };
      },
      async loadNetworkSettings() {
        return previewSettings;
      },
      async saveNetworkSettings(settings) {
        previewSettings = settings;
        return settings;
      },
      async state() {
        return { phase: current, message: note, logs: lines };
      },
      async logs() {
        return lines;
      },
      async saveProfile() {
        note = "preview.profileSaved";
        lines.push(deps.logLine(note));
      },
      async clearProfile() {
        current = "IDLE";
        note = "preview.profileCleared";
        lines = [];
      },
      async setConnectionMode() {},
      async probeNode() {
        throw new Error("preview.latencyNeedsDesktop");
      },
      async beginProbe() {
        throw new Error("preview.probeNeedsDesktop");
      },
      async probeDelay() {
        throw new Error("preview.probeNeedsDesktop");
      },
      async probeSpeed() {
        throw new Error("preview.probeNeedsDesktop");
      },
      async probeSpeedUp() {
        throw new Error("preview.probeNeedsDesktop");
      },
      async onSpeedProgress() {
        return () => {};
      },
      async onSpeedUpProgress() {
        return () => {};
      },
      async endProbe() {},
      async fetchSubscription(url) {
        const response = await fetch(url);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        return response.text();
      },
      async fetchSubscriptionDetails(url) {
        const response = await fetch(url);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        return {
          raw: await response.text(),
          userinfo: response.headers.get("subscription-userinfo"),
        };
      },
      async convertSubscription() {
        throw new Error("error.conversionDesktopOnly");
      },
      async loadSubscriptions() {
        return "[]";
      },
      async saveSubscriptions() {},
      async connect(_mode) {
        current = reduce(current === "FAILED" ? "FAILED" : "IDLE", "start");
        current = reduce(current, "core_spawned");
        current = reduce(current, "fail");
        note = "preview.cannotConnect";
        lines.push(deps.logLine(note));
        return { phase: current, message: note, logs: lines };
      },
      async disconnect() {
        if (current !== "FAILED" && current !== "IDLE") current = reduce(current, "stop");
        if (current === "DISCONNECTING") current = reduce(current, "stopped");
        if (current === "FAILED") current = reduce(current, "stop");
        note = "status.disconnected";
        lines.push(deps.logLine(note));
        return { phase: current, message: note, logs: lines };
      },
    };
  }

  return { bridge, browserBridge };
}
