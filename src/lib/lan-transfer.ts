// 局域网文件传输的前端类型。字段与 Rust（src-tauri/src/lan_transfer.rs、
// airport-lan crate）的 serde camelCase 输出一致。见 docs/desktop/局域网文件传输设计.md。

/** 默认局域网端口，与核心层 airport_lan::protocol::DEFAULT_PORT 保持一致。 */
export const DEFAULT_LAN_PORT = 53317;
/** 单条文本消息体积上限（字节），需与核心层文本上限保持一致。 */
export const MAX_TEXT_BYTES = 65536;

export type LanDeviceType = "mobile" | "desktop" | "web" | "headless" | "server";

export type LanDirection = "send" | "receive";

export type LanTransferStateName =
  | "PENDING"
  | "WAITING_APPROVAL"
  | "TRANSFERRING"
  | "PAUSED"
  | "COMPLETED"
  | "FAILED"
  | "CANCELED";

export interface LanLocalDevice {
  alias: string;
  fingerprint: string;
  deviceType: LanDeviceType;
  port: number;
  /** 本机物理网卡私网 IPv4（供二维码直连）；无合适网卡时为 null。 */
  ip?: string | null;
}

export interface LanDiscoveredDevice {
  alias: string;
  fingerprint: string;
  address: string;
  port: number;
  deviceType: LanDeviceType | null;
  trusted: boolean;
}

export interface LanTransferTask {
  id: string;
  direction: LanDirection;
  peerAlias: string;
  peerFingerprint: string;
  fileName: string;
  size: number;
  state: LanTransferStateName;
  savePath?: string | null;
}

export interface LanHistoryEntry {
  id: string;
  direction: LanDirection;
  peerAlias: string;
  peerFingerprint: string;
  fileName: string;
  size: number;
  state: LanTransferStateName;
  finishedAt: number | null;
  savePath: string | null;
  text?: string | null;
}

/** 已配对（可信）设备。对应 Rust storage::TrustedRow。 */
export interface LanTrustedDevice {
  fingerprint: string;
  alias: string;
  platform: string | null;
  autoAccept: boolean;
}

/** 收到的配对请求（lan://pair 事件）。 */
export interface LanPairRequest {
  requestId: string;
  peerAlias: string;
  peerFingerprint: string;
}

export interface LanSettings {
  alias: string;
  discoverable: boolean;
  acceptIncoming: boolean;
  autoAcceptTrusted: boolean;
  defaultSaveDir: string;
  maxConcurrent: number;
}

export interface LanIncomingFile {
  name: string;
  size: number;
}

export interface LanIncomingRequest {
  taskId: string;
  peerAlias: string;
  files: LanIncomingFile[];
  totalSize: number;
  text?: string | null;
}

export interface LanProgress {
  taskId: string;
  bytesDone: number;
  bytesTotal: number;
  speedBps: number;
  etaSecs: number | null;
}
