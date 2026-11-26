// Type/policy mappings for structured rule editing, plus conversion between sing-box rule objects ↔ simple UI rules.
// Handles only simple rules of the form "single matcher + single value + direct/select/reject"; complex rules (multiple matchers, logical rules,
// pointing at a specific node, etc.) cannot be expressed in the form, so parsing returns null; the caller keeps them as-is and writes them back on save, so no rule is lost.

export type RulePolicy = "direct" | "select" | "reject";
export type UiRule = { id: string; type: string; value: string; policy: RulePolicy };

// matcher key (sing-box field) → display code and i18n name key.
export const RULE_TYPES = [
  { key: "domain", code: "DOMAIN", nameKey: "rules.typeDomain" },
  { key: "domain_suffix", code: "DOMAIN-SUFFIX", nameKey: "rules.typeDomainSuffix" },
  { key: "domain_keyword", code: "DOMAIN-KEYWORD", nameKey: "rules.typeDomainKeyword" },
  { key: "domain_regex", code: "DOMAIN-REGEX", nameKey: "rules.typeDomainRegex" },
  { key: "ip_cidr", code: "IP-CIDR", nameKey: "rules.typeIpCidr" },
  { key: "process_name", code: "PROCESS-NAME", nameKey: "rules.typeProcess" },
  { key: "port", code: "PORT", nameKey: "rules.typePort" },
] as const;

export const RULE_POLICIES = [
  { value: "direct" as const, code: "DIRECT", nameKey: "rules.policyDirect" },
  { value: "select" as const, code: "PROXY", nameKey: "rules.policyProxy" },
  { value: "reject" as const, code: "REJECT", nameKey: "rules.policyReject" },
] as const;

const TYPE_KEYS = RULE_TYPES.map(t => t.key as string);

export function ruleCode(type: string): string {
  return RULE_TYPES.find(t => t.key === type)?.code ?? type.toUpperCase();
}

/** sing-box rule → simple UI rule; returns null for anything not expressible as a single-matcher form (left for the caller to preserve as-is). */
export function ruleToUi(rule: unknown): UiRule | null {
  if (!rule || typeof rule !== "object") return null;
  const obj = rule as Record<string, unknown>;
  const keys = Object.keys(obj);
  const type = TYPE_KEYS.find(k => keys.includes(k));
  if (!type) return null;
  const raw = obj[type];
  let value: string | null = null;
  if (Array.isArray(raw)) {
    if (raw.length === 1 && (typeof raw[0] === "string" || typeof raw[0] === "number")) value = String(raw[0]);
  } else if (typeof raw === "string" || typeof raw === "number") {
    value = String(raw);
  }
  if (value === null) return null;
  let policy: RulePolicy | null = null;
  if (obj.action === "reject") policy = "reject";
  else if (obj.outbound === "direct") policy = "direct";
  else if (obj.outbound === "select") policy = "select";
  if (!policy) return null;
  // Allow only matcher + outbound/action; any other key means a complex rule, which is kept and not edited.
  const allowed = new Set([type, "outbound", "action"]);
  if (keys.some(k => !allowed.has(k))) return null;
  return { id: crypto.randomUUID(), type, value, policy };
}

/** Simple UI rule → sing-box rule object. */
export function uiToRule(rule: UiRule): Record<string, unknown> {
  const value: string | number = rule.type === "port" ? Number(rule.value) : rule.value;
  const out: Record<string, unknown> = { [rule.type]: [value] };
  if (rule.policy === "reject") out.action = "reject";
  else out.outbound = rule.policy;
  return out;
}

/** Basic value validation: non-empty; port must be an integer in 1–65535. */
export function validRuleValue(type: string, value: string): boolean {
  const v = value.trim();
  if (!v) return false;
  if (type === "port") {
    const n = Number(v);
    return Number.isInteger(n) && n >= 1 && n <= 65535;
  }
  return true;
}
