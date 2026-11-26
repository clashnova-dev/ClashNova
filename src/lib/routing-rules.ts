import { CHINA_DOMAINS } from "./china-domains.ts";
import { ruleSetConfig, type RuleSetToggles } from "./rule-sets.ts";

const NO_RULE_SETS: RuleSetToggles = { chinaGeosite: false, adBlock: false };

/**
 * Built-in routing precedes provider rules; application policies precede both.
 * Ad blocking (reject) comes first; then private-network direct; China-direct uses the geosite:cn+geoip:cn rule sets (when chinaGeosite is on) or
 * falls back to the built-in China domain-suffix list. The corresponding rule_set definitions are injected into route.rule_set separately by ruleSetConfig(...).defs.
 */
export function builtinRouteRules(enabled: boolean, toggles: RuleSetToggles = NO_RULE_SETS) {
  if (!enabled) return [];
  const { rules } = ruleSetConfig(toggles);
  const ads = rules.filter(r => r.action === "reject");
  const china = rules.filter(r => r.outbound === "direct");
  return [
    ...ads,
    { ip_is_private: true, outbound: "direct" },
    ...(china.length ? china : [{ domain_suffix: [...CHINA_DOMAINS], outbound: "direct" }]),
  ];
}
