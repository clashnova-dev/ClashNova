// Frontend types for LAN file transfer. Fields match the serde camelCase output of the Rust side
// (src-tauri/src/lan_transfer.rs, airport-lan crate). See docs/desktop/局域网文件传输设计.md.

/** Default LAN port; kept in sync with the core layer's airport_lan::protocol::DEFAULT_PORT. */
export const DEFAULT_LAN_PORT = 53317;
/** Maximum size of a single text message (bytes); must match the core layer's text limit. */
export const MAX_TEXT_BYTES = 65536;

export type LanDeviceType = "desktop" | "laptop" | "phone" | "tablet" | "server" | "tv" | "unknown";

export type LanDirection = "send" | "receive";

export type LanTransferStateName =
  "PENDING" | "WAITING_APPROVAL" | "TRANSFERRING" | "PAUSED" | "COMPLETED" | "FAILED" | "CANCELED";

export interface LanLocalDevice {
  alias: string;
  fingerprint: string;
  deviceType: LanDeviceType;
  port: number;
  /** The machine's physical NIC private IPv4 (for QR-code direct connection); null when no suitable NIC exists. */
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

/** A paired (trusted) device. Corresponds to the Rust storage::TrustedRow. */
export interface LanTrustedDevice {
  fingerprint: string;
  alias: string;
  platform: string | null;
  autoAccept: boolean;
}

/** An incoming pairing request (lan://pair event). */
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
