export type SearchOptions = { caseSensitive: boolean; wholeWord: boolean; regex: boolean };
export function connectionSearch(query: string, options: SearchOptions) {
  const text = query.trim();
  if (!text) return { error: false, matches: (_value: string) => true };
  try {
    let pattern = options.regex ? text : text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    // Unicode word boundaries also support Chinese and non-Latin process names.
    if (options.wholeWord) pattern = `(?:^|[^\\p{L}\\p{N}_])(?:${pattern})(?=$|[^\\p{L}\\p{N}_])`;
    const expression = new RegExp(pattern, options.caseSensitive ? "u" : "iu");
    return { error: false, matches: (value: string) => expression.test(value) };
  } catch {
    return { error: true, matches: (_value: string) => false };
  }
}
