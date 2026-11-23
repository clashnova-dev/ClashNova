// 轻量 JSON 语法高亮:把 JSON 文本转成带 token span 的 HTML,供编辑器的只读高亮层渲染。
// 不依赖第三方库;无法解析的片段按纯文本转义输出,编辑器照常可用(高亮降级而非出错)。

function escapeHtml(text: string): string {
  return text.replace(/[&<>]/g, ch => (ch === "&" ? "&amp;" : ch === "<" ? "&lt;" : "&gt;"));
}

// 依次匹配:字符串(含转义)、字面量关键字、数字。其余字符(标点、空白)原样转义输出。
const TOKEN = /"(?:\\.|[^"\\])*"|\b(?:true|false|null)\b|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/g;

/** JSON 文本 → 高亮 HTML。调用方负责把结果放进 `white-space: pre` 的只读层。 */
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
      // 字符串后紧跟冒号(允许空白)视为对象键名,配色区别于普通字符串值。
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

/** 可解析时按两空格缩进美化,否则原样返回(例如用户粘贴了 YAML / 分享链接)。 */
export function prettyJson(source: string): string {
  try {
    return JSON.stringify(JSON.parse(source), null, 2);
  } catch {
    return source;
  }
}
