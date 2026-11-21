// Web pages wake the desktop app with clashnova:// links; our own import link
// carries a short-lived one-time code, never a subscription address. We also
// accept Clash's clash://install-config?url=...&name=... links, where the
// subscription address is in the link itself — those always go through the
// confirmation dialog and are limited to https (or http on localhost).

export type DeepLink =
  | { kind: "open" }
  | { kind: "nodes" }
  | { kind: "settings" }
  | { kind: "connect" }
  | { kind: "import"; code: string }
  | { kind: "importUrl"; url: string; name: string }
  | { kind: "rejected" };

const CODE = /^[A-Za-z0-9_-]{16,64}$/;

export function parseDeepLink(raw: string): DeepLink | null {
  let url: URL;
  try { url = new URL(raw); }
  catch { return null; }
  if (url.protocol === "clash:" || url.protocol === "clashmeta:") return parseClashLink(url);
  if (url.protocol !== "clashnova:") return null;
  const path = url.pathname === "/" ? "" : url.pathname.replace(/\/$/, "");
  const host = url.hostname;
  if (!path && (host === "open" || host === "nodes" || host === "settings" || host === "connect")) return { kind: host };
  if (host === "subscription" && path === "/import") return readImportCode(url);
  return null;
}

// Clash clients use clash://install-config?url=<subscription>&name=<label>.
// The action may sit in the host (clash://install-config?…) or, without the
// authority slashes, in the path (clash:install-config?…).
function parseClashLink(url: URL): DeepLink {
  const action = (url.hostname || url.pathname.replace(/^\/+/, "").replace(/\/+$/, "")).toLowerCase();
  if (action !== "install-config") return { kind: "rejected" };
  const target = url.searchParams.get("url") ?? "";
  if (!isSubscriptionAddress(target)) return { kind: "rejected" };
  return { kind: "importUrl", url: target, name: (url.searchParams.get("name") ?? "").trim() };
}

function readImportCode(url: URL): DeepLink {
  const names: string[] = [];
  url.searchParams.forEach((_value, key) => { names.push(key); });
  if (names.length !== 1 || (names[0] !== "code" && names[0] !== "token")) return { kind: "rejected" };
  const code = url.searchParams.get(names[0]) ?? "";
  return CODE.test(code) ? { kind: "import", code } : { kind: "rejected" };
}

export function redeemedSubscription(data: unknown): { name: string; url: string } | null {
  if (!data || typeof data !== "object") return null;
  const record = data as { name?: unknown; url?: unknown };
  if (typeof record.url !== "string" || !isSubscriptionAddress(record.url)) return null;
  const name = typeof record.name === "string" ? record.name.trim() : "";
  return { name, url: record.url };
}

function isSubscriptionAddress(value: string): boolean {
  try {
    const url = new URL(value);
    if (url.username || url.password) return false;
    if (url.protocol === "https:") return true;
    return url.protocol === "http:" && (url.hostname === "localhost" || url.hostname === "127.0.0.1");
  } catch { return false; }
}
