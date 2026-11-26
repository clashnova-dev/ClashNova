// geosite/geoip remote rule sets (SagerNet sing-geosite / sing-geoip .srs files, via the trusted jsDelivr mirror).
// Rule-set definitions are bundled with the client and injected; the rule_set shipped with a subscription is not trusted. The TUN validator (tun-service) only allows
// remote https sources on cdn.jsdelivr.net, so changing the mirror requires updating that allowlist too. Not cached to disk; refetched on every connection.

import { RULE_SET_MIRROR } from "../constants/domains.ts";

const MIRROR = RULE_SET_MIRROR;

export type RuleSetDef = { type: "remote"; tag: string; format: "binary"; url: string };

function geosite(name: string): RuleSetDef {
  return {
    type: "remote",
    tag: `geosite-${name}`,
    format: "binary",
    url: `${MIRROR}/sing-geosite@rule-set/geosite-${name}.srs`,
  };
}
function geoip(name: string): RuleSetDef {
  return {
    type: "remote",
    tag: `geoip-${name}`,
    format: "binary",
    url: `${MIRROR}/sing-geoip@rule-set/geoip-${name}.srs`,
  };
}

export const RULE_SETS = {
  geositeCn: geosite("cn"),
  geoipCn: geoip("cn"),
  ads: geosite("category-ads-all"),
} as const;

/** Tag of the geosite domain rule set (used for DNS-side mirroring: domestic domains resolve through domestic resolvers). */
export const GEOSITE_CN_TAG = RULE_SETS.geositeCn.tag;

export type RuleSetToggles = { chinaGeosite: boolean; adBlock: boolean };

/**
 * Returns the route rules to inject (tag references) and their corresponding rule_set definitions (remote objects), based on the toggles.
 * Rule order: ad blocking (reject) first, then China-direct (direct). The caller then appends private-network direct and the rest.
 */
export function ruleSetConfig(toggles: RuleSetToggles): {
  rules: Record<string, unknown>[];
  defs: RuleSetDef[];
} {
  const rules: Record<string, unknown>[] = [];
  const defs: RuleSetDef[] = [];
  if (toggles.adBlock) {
    rules.push({ rule_set: [RULE_SETS.ads.tag], action: "reject" });
    defs.push(RULE_SETS.ads);
  }
  if (toggles.chinaGeosite) {
    rules.push({ rule_set: [RULE_SETS.geositeCn.tag, RULE_SETS.geoipCn.tag], outbound: "direct" });
    defs.push(RULE_SETS.geositeCn, RULE_SETS.geoipCn);
  }
  return { rules, defs };
}
