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

type Outbound = { type?: string; tag?: string; server?: string; server_port?: number; outbounds?: string[]; default?: string };

// A privileged TUN core rejects rule_set (see the TUN service's safe_config) and sing-box 1.14
// dropped the legacy geoip/geosite databases, so an imported route/dns rule that matches on any
// of them cannot run and would fail the whole connection. Drop those rules and their rule-set
// definitions; the client provides its own China routing in rule mode, so nothing useful is lost.
const UNSUPPORTED_MATCHERS = new Set(["rule_set", "geoip", "geosite"]);
function matchesUnsupported(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(matchesUnsupported);
  if (value && typeof value === "object") return Object.entries(value).some(([key, inner]) => UNSUPPORTED_MATCHERS.has(key) || matchesUnsupported(inner));
  return false;
}
function stripUnsupportedRules(section: unknown): void {
  if (!section || typeof section !== "object") return;
  const block = section as { rule_set?: unknown; rules?: unknown };
  delete block.rule_set;
  if (Array.isArray(block.rules)) block.rules = block.rules.filter(rule => !matchesUnsupported(rule));
}

export function parseSingBoxProfile(raw: string): { config: Record<string, unknown>; nodes: ProfileNode[] } {
  if (raw.length > 2 * 1024 * 1024) throw new Error("error.subscriptionTooLarge");
  let config: Record<string, unknown>;
  try { config = JSON.parse(raw); }
  catch { throw new Error("error.subscriptionNotSingBox"); }
  if (!config || typeof config !== "object" || !Array.isArray(config.outbounds)) throw new Error("error.subscriptionNoOutbounds");
  const outbounds = config.outbounds as Outbound[];
  const tags = new Set<string>();
  for (const outbound of outbounds) {
    if (typeof outbound?.tag !== "string" || !outbound.tag || tags.has(outbound.tag)) throw new Error("error.subscriptionBadTags");
    tags.add(outbound.tag);
  }
  const nodes = outbounds.filter(item => typeof item.server === "string" && item.server && Number.isInteger(item.server_port) && (item.server_port ?? 0) > 0 && (item.server_port ?? 0) <= 65535)
    .map(item => ({ id: item.tag!, name: item.tag!, region: "nodes.customRegion", host: item.server!, port: item.server_port!, protocol: item.type ?? "common.unknown", agentOnline: true, appliedConfigVersion: 0, latestConfigVersion: 0 }));
  if (!nodes.length) throw new Error("error.subscriptionNoNodes");
  if (outbounds.some(item => item.tag === "direct" && item.type !== "direct")) throw new Error("error.subscriptionTagDirect");
  if (!tags.has("direct")) outbounds.push({ type: "direct", tag: "direct" });
  const selector = outbounds.find(item => item.tag === "select" && item.type === "selector");
  if (tags.has("select") && !selector) throw new Error("error.subscriptionTagSelect");
  if (selector) {
    selector.outbounds = nodes.map(node => node.id);
    selector.default = nodes[0].id;
  } else {
    outbounds.push({ type: "selector", tag: "select", outbounds: nodes.map(node => node.id), default: nodes[0].id });
  }
  stripUnsupportedRules(config.route);
  stripUnsupportedRules(config.dns);
  return { config, nodes };
}
