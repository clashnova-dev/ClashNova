// Central registry of external domains / endpoints. Each constant reads a build-time
// env var (VITE_*) with a sane default, so self-hosting or switching mirrors only needs
// the env var set in .env — no code change. Keys are documented in .env.example.

// —— About-page links (empty hides the corresponding entry) ——

// Project repository (GitHub).
const env = import.meta.env ?? {};

export const githubUrl = (env.VITE_GITHUB_URL ?? "https://github.com/clashnova-dev/ClashNova").trim();

// Social (X / Twitter).
export const xUrl = (env.VITE_X_URL ?? "https://x.com/ClashNova2048").trim();

// —— Functional endpoints ——

// Connectivity probe for the automatic (urltest) selection; a 204 response means reachable.
export const CONNECTIVITY_CHECK_URL = (
  env.VITE_CONNECTIVITY_CHECK_URL ?? "https://www.gstatic.com/generate_204"
).trim();

// Remote mirror root for rule sets (geosite / geoip rule_set). Note: the backend root
// validator only allows jsdelivr, so switching mirrors also needs that allowlist widened,
// otherwise remote rule sets are rejected.
export const RULE_SET_MIRROR = (env.VITE_RULE_SET_MIRROR ?? "https://cdn.jsdelivr.net/gh/SagerNet").trim();

// STUN server used by the WebRTC leak check.
export const WEBRTC_STUN_URL = (env.VITE_WEBRTC_STUN_URL ?? "stun:stun.cloudflare.com:3478").trim();

// Base URL for the network-check "view leak details" external link (kind is appended, e.g. webrtc / dns).
export const BROWSERLEAKS_BASE = (env.VITE_BROWSERLEAKS_BASE ?? "https://browserleaks.com/").trim();
