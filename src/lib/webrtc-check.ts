import { isPublicIp, normalizeIp, type RtcObservation } from "./privacy-check.ts";
import { WEBRTC_STUN_URL } from "../constants/domains.ts";

/** Only this webview is measured. A browser opened separately has its own ICE policy. */
export function collectWebrtc(
  create: () => RTCPeerConnection = () => new RTCPeerConnection({ iceServers: [{ urls: WEBRTC_STUN_URL }] }),
  timeoutMs = 4000
): { result: Promise<RtcObservation>; cancel: () => void } {
  let cancel = () => {};
  const result = new Promise<RtcObservation>(resolve => {
    let pc: RTCPeerConnection;
    try {
      pc = create();
    } catch {
      resolve({ kind: "off" });
      return;
    }
    const found = new Set<string>();
    let settled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const finish = (fallback: RtcObservation["kind"]) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      pc.onicecandidate = null;
      try {
        pc.close();
      } catch {
        /* Already closed by the platform. */
      }
      const ips = [...found].filter(isPublicIp);
      resolve(ips.length ? { kind: "public", ips } : { kind: fallback === "none" && found.size ? "lan" : fallback });
    };
    cancel = () => finish("none");
    timer = setTimeout(() => finish("timeout"), timeoutMs);
    pc.onicecandidate = event => {
      if (!event.candidate) {
        finish("none");
        return;
      }
      const parts = event.candidate.candidate.trim().replace(/^a=/, "").split(/\s+/);
      if (parts[0]?.startsWith("candidate:") && parts.length >= 6 && !parts[4].endsWith(".local")) {
        // Validation excludes names and malformed candidates, including mDNS labels.
        const ip = parts[4];
        if (normalizeIp(ip)) found.add(ip);
      }
    };
    try {
      pc.createDataChannel("check");
      void pc
        .createOffer()
        .then(offer => {
          if (!settled) return pc.setLocalDescription(offer);
        })
        .catch(() => finish("off"));
    } catch {
      finish("off");
    }
  });
  return { result, cancel: () => cancel() };
}
