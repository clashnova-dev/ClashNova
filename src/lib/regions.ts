// Regions are shown as a short code chip rather than a flag emoji, which renders
// inconsistently on Windows. The node name is a fallback for generic regions.
const regionCodes: [RegExp, string][] = [
  [/香港|hong\s?kong|(?:^|[^a-z])hk(?:[^a-z]|$)/, "HK"], [/日本|japan|(?:^|[^a-z])jp(?:[^a-z]|$)/, "JP"],
  [/美国|united states|(?:^|[^a-z])usa?(?:[^a-z]|$)/, "US"], [/新加坡|singapore|(?:^|[^a-z])sg(?:[^a-z]|$)/, "SG"],
  [/台湾|taiwan|(?:^|[^a-z])tw(?:[^a-z]|$)/, "TW"], [/韩国|korea|(?:^|[^a-z])kr(?:[^a-z]|$)/, "KR"],
  [/英国|united kingdom|(?:^|[^a-z])(?:uk|gb)(?:[^a-z]|$)/, "UK"], [/德国|germany|(?:^|[^a-z])de(?:[^a-z]|$)/, "DE"],
];
export function regionCode(...sources: string[]) {
  for (const source of sources) {
    const value = (source || "").toLowerCase();
    for (const [pattern, code] of regionCodes) if (pattern.test(value)) return code;
  }
  return "";
}
