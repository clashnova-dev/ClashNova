// Probe / speed-test related types.

// A single speed-test sample (download or upload).
export type SpeedSample = {
  mbps: number;
  peakMbps: number;
  bytes: number;
  millis: number;
};

// Latency probe result for a single node.
export type ProbeDelay = {
  delay: number;
  coldDelay: number;
  warmed: boolean;
};

// The outbound tag and latency currently selected by automatic selection.
export type AutomaticSelection = {
  tag: string;
  delay?: number | null;
};

// Per-node status during a batch probe.
export type ProbeState = "queued" | "testing" | "success" | "failed";
