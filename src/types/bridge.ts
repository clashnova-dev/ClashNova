import { type PortConflict } from "../lib/port-conflict";
import { type AppPolicy } from "../lib/app-policies";
import { type NetworkSettings } from "../lib/network-settings";
import { type SubscriptionDownload } from "../lib/subscription-info";
import type { Phase } from "../lib/connection";
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
} from "../lib/lan-transfer";
import type { SpeedSample, ProbeDelay } from "./probe";

// Snapshot of the connection core's state.
export type Snapshot = {
  phase: Phase;
  message: string;
  logs?: string[];
  blocked?: boolean;
  guarded?: boolean;
};

// Transport-layer interface for the connection core: desktop goes through Tauri IPC, the browser preview uses an in-memory mock; both implement the same contract.
export type Bridge = {
  state(): Promise<Snapshot>;
  logs(): Promise<string[]>;
  proxyDiagnostics(): Promise<string>;
  inspectProxyPort(): Promise<PortConflict | null>;
  releaseProxyPort(token: string): Promise<void>;
  setApplicationPolicy(
    app: string,
    policy: AppPolicy
  ): Promise<{
    policies: Record<string, AppPolicy>;
    applied: boolean;
    closed: number;
  }>;
  loadNetworkSettings(): Promise<NetworkSettings>;
  saveNetworkSettings(settings: NetworkSettings): Promise<NetworkSettings>;
  saveProfile(config: string): Promise<void>;
  clearProfile(): Promise<void>;
  setConnectionMode(mode: "system" | "tun" | "manual"): Promise<void>;
  probeNode(host: string, port: number): Promise<number>;
  beginProbe(config: string, tags: string[]): Promise<string>;
  probeDelay(session: string, tag: string): Promise<ProbeDelay>;
  probeSpeed(session: string, tag: string): Promise<SpeedSample>;
  probeSpeedUp(session: string, tag: string): Promise<SpeedSample>;
  onSpeedProgress(cb: (p: { mbps: number; bytes: number }) => void): Promise<() => void>;
  onSpeedUpProgress(cb: (p: { mbps: number; bytes: number }) => void): Promise<() => void>;
  endProbe(session: string): Promise<void>;
  fetchSubscription(url: string): Promise<string>;
  fetchSubscriptionDetails(url: string): Promise<SubscriptionDownload>;
  convertSubscription(raw: string): Promise<string>;
  loadSubscriptions(): Promise<string>;
  saveSubscriptions(data: string): Promise<void>;
  connect(mode: "system" | "tun" | "manual"): Promise<Snapshot>;
  disconnect(): Promise<Snapshot>;
  // Hot-switch the exit node on the running core (Clash API selector); keeps the connection alive.
  selectNode(tag: string): Promise<void>;
  lanGetLocalDevice(): Promise<LanLocalDevice>;
  lanAddManualDevice(addr: string): Promise<LanDiscoveredDevice>;
  lanGetSettings(): Promise<LanSettings>;
  lanUpdateSettings(settings: LanSettings): Promise<LanSettings>;
  lanRefreshDevices(): Promise<void>;
  lanScanSubnet(): Promise<void>;
  lanListTransfers(): Promise<LanTransferTask[]>;
  lanSendFiles(fingerprint: string, paths: string[]): Promise<string[]>;
  lanSendText(fingerprint: string, text: string): Promise<LanHistoryEntry>;
  lanRespondIncoming(taskId: string, accept: boolean, saveDir: string | null): Promise<void>;
  lanPair(fingerprint: string): Promise<void>;
  lanRespondPair(requestId: string, accept: boolean): Promise<void>;
  /** 从某设备拉取配置同步：对端确认后返回其配置 payload（不透明 JSON 字符串）。 */
  lanSyncPull(fingerprint: string): Promise<string>;
  /** 响应收到的拉取同步请求：接受时把本机配置 payload 传回。 */
  lanRespondSync(requestId: string, accept: boolean, payload: string): Promise<void>;
  lanListTrusted(): Promise<LanTrustedDevice[]>;
  lanUnpair(fingerprint: string): Promise<void>;
  lanGetConversation(fingerprint: string): Promise<LanHistoryEntry[]>;
  lanOpenPath(path: string): Promise<void>;
  lanRevealPath(path: string): Promise<void>;
  lanDeleteFile(id: string, path: string | null): Promise<void>;
  lanCopyFileToClipboard(path: string): Promise<void>;
  lanClipboardFiles(): Promise<string[]>;
  lanPauseTransfer(taskId: string): Promise<void>;
  lanResumeTransfer(taskId: string): Promise<void>;
  lanCancelTransfer(taskId: string): Promise<void>;
  lanGetHistory(): Promise<LanHistoryEntry[]>;
  lanClearHistory(): Promise<void>;
  lanStart(): Promise<void>;
  lanStop(): Promise<void>;
  lanPickFiles(): Promise<string[]>;
  lanPickSaveDirectory(): Promise<string | null>;
  onLanText?(cb: (message: LanHistoryEntry) => void): Promise<() => void>;
  onLanDevices?(cb: (devices: LanDiscoveredDevice[]) => void): Promise<() => void>;
  onLanIncoming?(cb: (req: LanIncomingRequest) => void): Promise<() => void>;
  onLanTransfer?(cb: (task: LanTransferTask) => void): Promise<() => void>;
  onLanProgress?(cb: (p: LanProgress) => void): Promise<() => void>;
  onLanPair?(cb: (req: LanPairRequest) => void): Promise<() => void>;
  onLanSync?(cb: (req: LanPairRequest) => void): Promise<() => void>;
  onState?(cb: (s: Snapshot) => void): Promise<() => void>;
  onProgress?(cb: (stage: string) => void): Promise<() => void>;
};

// Dependencies the connection store injects into the transport-layer factory (read app policies / produce log lines).
export type BridgeDeps = {
  appPolicies: () => Record<string, AppPolicy>;
  logLine: (value: string) => string;
};
