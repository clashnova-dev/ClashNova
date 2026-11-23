// 结构化规则编辑所用的类型/策略映射,以及 sing-box 规则对象 ↔ 简单 UI 规则的互转。
// 只处理「单匹配 + 单值 + direct/select/reject」这类简单规则;复杂规则(多匹配、逻辑规则、
// 指向具体节点等)无法用表单表达,解析时返回 null,由调用方原样保留、保存时放回,确保不丢规则。

export type RulePolicy = "direct" | "select" | "reject";
export type UiRule = { id: string; type: string; value: string; policy: RulePolicy };

// matcher key(sing-box 字段)→ 展示用 code 与 i18n 名称键。
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

/** sing-box 规则 → 简单 UI 规则;无法表达为单匹配表单的返回 null(交由调用方原样保留)。 */
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
  // 只允许 matcher + outbound/action;出现其它键说明是复杂规则,保留不编辑。
  const allowed = new Set([type, "outbound", "action"]);
  if (keys.some(k => !allowed.has(k))) return null;
  return { id: crypto.randomUUID(), type, value, policy };
}

/** 简单 UI 规则 → sing-box 规则对象。 */
export function uiToRule(rule: UiRule): Record<string, unknown> {
  const value: string | number = rule.type === "port" ? Number(rule.value) : rule.value;
  const out: Record<string, unknown> = { [rule.type]: [value] };
  if (rule.policy === "reject") out.action = "reject";
  else out.outbound = rule.policy;
  return out;
}

/** 内容基础校验:非空;port 必须是 1–65535 的整数。 */
export function validRuleValue(type: string, value: string): boolean {
  const v = value.trim();
  if (!v) return false;
  if (type === "port") {
    const n = Number(v);
    return Number.isInteger(n) && n >= 1 && n <= 65535;
  }
  return true;
}
