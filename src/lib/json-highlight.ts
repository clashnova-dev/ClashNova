// Lightweight JSON syntax highlighting: turns JSON text into HTML with token spans for the editor's read-only highlight layer.
// No third-party dependency; unparseable fragments are escaped as plain text so the editor keeps working (highlighting degrades rather than fails).

function escapeHtml(text: string): string {
  return text.replace(/[&<>]/g, ch => (ch === "&" ? "&amp;" : ch === "<" ? "&lt;" : "&gt;"));
}

// Matches, in order: strings (including escapes), literal keywords, numbers. Other characters (punctuation, whitespace) are escaped as-is.
const TOKEN = /"(?:\\.|[^"\\])*"|\b(?:true|false|null)\b|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/g;

/** JSON text → highlighted HTML. The caller is responsible for placing the result into a `white-space: pre` read-only layer. */
export function highlightJson(source: string): string {
  let html = "";
  let last = 0;
  let match: RegExpExecArray | null;
  TOKEN.lastIndex = 0;
  while ((match = TOKEN.exec(source))) {
    html += escapeHtml(source.slice(last, match.index));
    const token = match[0];
    let cls: string;
    if (token[0] === '"') {
      // A string immediately followed by a colon (whitespace allowed) is treated as an object key, colored differently from a plain string value.
      cls = /^\s*:/.test(source.slice(match.index + token.length)) ? "tok-key" : "tok-str";
    } else if (token === "true" || token === "false") cls = "tok-bool";
    else if (token === "null") cls = "tok-null";
    else cls = "tok-num";
    html += `<span class="${cls}">${escapeHtml(token)}</span>`;
    last = match.index + token.length;
  }
  html += escapeHtml(source.slice(last));
  return html;
}

/** Pretty-prints with two-space indentation when parseable, otherwise returns the input as-is (e.g. the user pasted YAML / a share link). */
export function prettyJson(source: string): string {
  try {
    return JSON.stringify(JSON.parse(source), null, 2);
  } catch {
    return source;
  }
}
