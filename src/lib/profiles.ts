export type ProfileNode = {
  id: string;
  name: string;
  region: string;
  host: string;
  port: number;
  protocol: string;
  agentOnline: boolean;
  appliedConfigVersion: number;
  latestConfigVersion: number;
};

type Outbound = {
  type?: string;
  tag?: string;
  server?: string;
  server_port?: number;
  outbounds?: string[];
  default?: string;
};

// A privileged TUN core rejects rule_set (see the TUN service's safe_config) and sing-box 1.14
// dropped the legacy geoip/geosite databases, so an imported route/dns rule that matches on any
// of them cannot run and would fail the whole connection. Drop those rules and their rule-set
// definitions; the client provides its own China routing in rule mode, so nothing useful is lost.
const UNSUPPORTED_MATCHERS = new Set(["rule_set", "geoip", "geosite"]);
function matchesUnsupported(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(matchesUnsupported);
  if (value && typeof value === "object")
    return Object.entries(value).some(([key, inner]) => UNSUPPORTED_MATCHERS.has(key) || matchesUnsupported(inner));
  return false;
}
function stripUnsupportedRules(section: unknown): void {
  if (!section || typeof section !== "object") return;
  const block = section as { rule_set?: unknown; rules?: unknown };
  delete block.rule_set;
  if (Array.isArray(block.rules)) block.rules = block.rules.filter(rule => !matchesUnsupported(rule));
}

// sing-box 1.12 reshaped dns.servers and removed the legacy forms in 1.14: a server is now
// {type, server, ...} instead of {address}. An imported subscription written for an older core
// carries the legacy shape, which 1.14+ rejects at startup ("legacy DNS server formats ...
// removed"). Convert it in place so any profile runs on the current core. Ported from the mobile
// SingBoxDnsServer parser (airport-mobile .../config/dns_server.dart) to keep both ends identical.
const DNS_DEFAULT_PORTS: Record<string, number> = { udp: 53, tcp: 53, tls: 853, quic: 853, https: 443, h3: 443 };
const DNS_SCHEME_TYPES: Record<string, string> = {
  udp: "udp", tcp: "tcp", tls: "tls", dot: "tls", quic: "quic", doq: "quic",
  https: "https", doh: "https", h3: "h3", http3: "h3",
};
// Fake-IP ranges when the removed top-level dns.fakeip block named none; matches tun.rs.
const DNS_FAKEIP_V4 = "198.18.0.0/15";
const DNS_FAKEIP_V6 = "fc00::/18";

function isIpLiteral(host: string): boolean {
  return /^\d{1,3}(\.\d{1,3}){3}$/.test(host) || host.includes(":");
}

// Split "host", "host:port", "[v6]" or "[v6]:port" into its parts; an unbracketed IPv6 address
// (several colons) is treated as a bare host.
function splitDnsHostPort(input: string): { host: string; port?: number } {
  const value = input.trim();
  if (value.startsWith("[")) {
    const end = value.indexOf("]");
    if (end >= 0) {
      const host = value.slice(1, end);
      const tail = value.slice(end + 1);
      const port = tail.startsWith(":") ? Number.parseInt(tail.slice(1), 10) : NaN;
      return { host, port: Number.isInteger(port) ? port : undefined };
    }
  }
  const colon = value.lastIndexOf(":");
  if (colon >= 0 && value.indexOf(":") === colon) {
    const port = Number.parseInt(value.slice(colon + 1), 10);
    if (Number.isInteger(port)) return { host: value.slice(0, colon), port };
  }
  return { host: value, port: undefined };
}

function remoteDnsServer(
  type: string, tag: string | undefined, host: string, port: number | undefined,
  detour: string | undefined, domainResolver: string | undefined, path: string,
): Record<string, unknown> | null {
  if (!host) return null;
  const isIp = isIpLiteral(host);
  const hasDetour = !!detour;
  return {
    type,
    ...(tag ? { tag } : {}),
    server: host,
    ...(port != null && port !== DNS_DEFAULT_PORTS[type] ? { server_port: port } : {}),
    ...(path ? { path } : {}),
    ...(hasDetour ? { detour } : {}),
    ...(!isIp && !hasDetour && domainResolver ? { domain_resolver: domainResolver } : {}),
  };
}

// Convert one legacy `address` string to a 1.14 server object. Returns null for forms with no
// server equivalent (rcode://, plain http://, fakeip — handled by the caller or dropped).
function legacyDnsAddressToServer(
  address: string, tag: string | undefined, detour: string | undefined, domainResolver: string | undefined,
): Record<string, unknown> | null {
  const input = address.trim();
  if (!input) return null;
  const lower = input.toLowerCase();
  if (lower === "local" || lower === "system" || lower === "local://") return { type: "local", ...(tag ? { tag } : {}) };
  const schemeEnd = input.indexOf("://");
  if (schemeEnd < 0) {
    const { host, port } = splitDnsHostPort(input);
    return remoteDnsServer("udp", tag, host, port, detour, domainResolver, "");
  }
  const scheme = lower.slice(0, schemeEnd);
  let rest = input.slice(schemeEnd + 3);
  if (scheme === "dhcp") {
    const iface = rest.replace(/\//g, "").trim();
    return { type: "dhcp", ...(tag ? { tag } : {}), ...(iface && iface.toLowerCase() !== "auto" ? { interface: iface } : {}) };
  }
  const type = DNS_SCHEME_TYPES[scheme];
  if (!type) return null; // rcode:// / http:// / unknown: no 1.14 server equivalent.
  const hash = rest.indexOf("#");
  if (hash >= 0) rest = rest.slice(0, hash);
  let path = "";
  const slash = rest.indexOf("/");
  const question = rest.indexOf("?");
  let cut = -1;
  if (slash >= 0 && (question < 0 || slash < question)) cut = slash;
  else if (question >= 0) cut = question;
  if (cut >= 0) { path = rest.slice(cut); rest = rest.slice(0, cut); }
  const at = rest.lastIndexOf("@");
  if (at >= 0) rest = rest.slice(at + 1);
  const { host, port } = splitDnsHostPort(rest);
  if (path.startsWith("?")) path = `/dns-query${path}`;
  const keepPath = (type === "https" || type === "h3") && path !== "/dns-query" && path !== "/" ? path : "";
  return remoteDnsServer(type, tag, host, port, detour, domainResolver, keepPath);
}

// Rewrite a profile's legacy dns block to the 1.14 format: convert each `address` server, fold the
// removed top-level `fakeip` block into a {type:"fakeip"} server, and drop the stale block. New-format
// servers and configs without a legacy trace are left untouched.
export function normalizeLegacyDns(dns: unknown): void {
  if (!dns || typeof dns !== "object") return;
  const block = dns as { servers?: unknown; fakeip?: { inet4_range?: string; inet6_range?: string } };
  const servers = Array.isArray(block.servers) ? (block.servers as Record<string, unknown>[]) : null;
  const fakeip = block.fakeip && typeof block.fakeip === "object" ? block.fakeip : null;
  const hasLegacy = !!servers && servers.some(s => s && typeof s === "object" && typeof s.address === "string" && s.type == null);
  if (!hasLegacy && !fakeip) return;
  const v4 = fakeip?.inet4_range ?? DNS_FAKEIP_V4;
  const v6 = fakeip?.inet6_range ?? DNS_FAKEIP_V6;
  if (servers) {
    const out: Record<string, unknown>[] = [];
    for (const server of servers) {
      if (!server || typeof server !== "object") continue;
      if (typeof server.type === "string") { out.push(server); continue; } // already 1.14.
      const address = typeof server.address === "string" ? server.address.trim() : "";
      const tag = typeof server.tag === "string" && server.tag ? server.tag : undefined;
      if (address.toLowerCase() === "fakeip") {
        out.push({ type: "fakeip", ...(tag ? { tag } : {}), inet4_range: v4, inet6_range: v6 });
        continue;
      }
      const detour = typeof server.detour === "string" && server.detour ? server.detour : undefined;
      const domainResolver = typeof server.address_resolver === "string" && server.address_resolver ? server.address_resolver : undefined;
      const converted = legacyDnsAddressToServer(address, tag, detour, domainResolver);
      if (converted) out.push(converted); // Unconvertible legacy forms are dropped, not passed through.
    }
    block.servers = out;
  }
  delete block.fakeip;
}

// Parse the active profile's raw text into a sing-box config object: external profiles go through the
// sing-box profile parser (keeping only its config), native ones are plain JSON.
export function parseProfileConfig(raw: string, external: boolean): any {
  return external ? parseSingBoxProfile(raw).config : JSON.parse(raw);
}

export function parseSingBoxProfile(raw: string): {
  config: Record<string, unknown>;
  nodes: ProfileNode[];
} {
  if (new TextEncoder().encode(raw).byteLength > 2 * 1024 * 1024) throw new Error("error.subscriptionTooLarge");
  let config: Record<string, unknown>;
  try {
    config = JSON.parse(raw);
  } catch {
    throw new Error("error.subscriptionNotSingBox");
  }
  if (!config || typeof config !== "object" || !Array.isArray(config.outbounds))
    throw new Error("error.subscriptionNoOutbounds");
  const outbounds = config.outbounds as Outbound[];
  const tags = new Set<string>();
  for (const outbound of outbounds) {
    if (typeof outbound?.tag !== "string" || !outbound.tag || tags.has(outbound.tag))
      throw new Error("error.subscriptionBadTags");
    tags.add(outbound.tag);
  }
  const nodes = outbounds
    .filter(
      item =>
        typeof item.server === "string" &&
        item.server &&
        Number.isInteger(item.server_port) &&
        (item.server_port ?? 0) > 0 &&
        (item.server_port ?? 0) <= 65535
    )
    .map(item => ({
      id: item.tag!,
      name: item.tag!,
      region: "nodes.customRegion",
      host: item.server!,
      port: item.server_port!,
      protocol: item.type ?? "common.unknown",
      agentOnline: true,
      appliedConfigVersion: 0,
      latestConfigVersion: 0,
    }));
  if (!nodes.length) throw new Error("error.subscriptionNoNodes");
  if (outbounds.some(item => item.tag === "direct" && item.type !== "direct"))
    throw new Error("error.subscriptionTagDirect");
  if (!tags.has("direct")) outbounds.push({ type: "direct", tag: "direct" });
  const selector = outbounds.find(item => item.tag === "select" && item.type === "selector");
  if (tags.has("select") && !selector) throw new Error("error.subscriptionTagSelect");
  if (selector) {
    selector.outbounds = nodes.map(node => node.id);
    selector.default = nodes[0].id;
  } else {
    outbounds.push({
      type: "selector",
      tag: "select",
      outbounds: nodes.map(node => node.id),
      default: nodes[0].id,
    });
  }
  stripUnsupportedRules(config.route);
  stripUnsupportedRules(config.dns);
  normalizeLegacyDns(config.dns);
  return { config, nodes };
}
