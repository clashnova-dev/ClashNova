import { connectionSearch, type SearchOptions } from "./connection-search.ts";

export type LogLevel = "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR";
export type LogSource = "client" | "core" | "tun";
export type LogEntry = {
  id: string;
  time: string;
  level: LogLevel;
  source: LogSource;
  message: string;
  text: string;
};
export const LOG_WINDOW_LIMIT = 5000;

/** Remove common credentials before showing or sharing a diagnostic line. */
export function redactLog(text: string): string {
  return text
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/\b(?:ss|ssr|vmess|vless|trojan|hy2|hysteria2):\/\/[^\s<>"']+/gi, "[REDACTED]")
    .replace(/(https?:\/\/)[^\s/@]+:[^\s/@]+@/gi, "$1[REDACTED]@")
    .replace(/(\/(?:subscribe|subscription|sub|token)\/)[^\s/?#"']+/gi, "$1[REDACTED]")
    .replace(
      /([?&](?:token|access_token|refresh_token|password|secret|key|api_key|code|uuid|auth)=)[^\s&#"']+/gi,
      "$1[REDACTED]"
    )
    .replace(/(\b(?:Authorization\s*[:=]\s*)?(?:Bearer|Basic)\s+)[^\s,"'}]+/gi, "$1[REDACTED]")
    .replace(
      /((?:["']?)(?:password|passwd|accessToken|refreshToken|access_token|refresh_token|token|secret|api_key|private_key|uuid)["']?\s*[:=]\s*)("(?:[^"\\]|\\.)*"|'[^']*'|[^\s,}]+)/gi,
      (_, key: string, value: string) =>
        key + (value.startsWith('"') ? '"[REDACTED]"' : value.startsWith("'") ? "'[REDACTED]'" : "[REDACTED]")
    );
}

export function logEntries(client: string[], runtime: string[], render: (line: string) => string): LogEntry[] {
  const rows = [...client.map(raw => ({ raw, client: true })), ...runtime.map(raw => ({ raw, client: false }))];
  return rows
    .map(({ raw, client }, index) => {
      const text = redactLog(render(raw));
      const time = text.match(/^\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:\.\d+)?/)?.[0] || "";
      const content = text.slice(time.length).trim();
      const detected =
        content
          .match(
            /\[(TRACE|DEBUG|INFO|WARN|WARNING|ERROR|FATAL|PANIC)\]|\b(TRACE|DEBUG|INFO|WARN|WARNING|ERROR|FATAL|PANIC)(?=\[)|^(TRACE|DEBUG|INFO|WARN|WARNING|ERROR|FATAL|PANIC)\b/
          )
          ?.slice(1)
          .find(Boolean) || "INFO";
      const level: LogLevel =
        detected === "WARNING" ? "WARN" : ["FATAL", "PANIC"].includes(detected) ? "ERROR" : (detected as LogLevel);
      const source: LogSource = client
        ? "client"
        : /\[TUN\]/.test(raw)
          ? "tun"
          : /\b(?:TRACE|DEBUG|INFO|WARN|ERROR|FATAL|PANIC)\[|\b(?:inbound|outbound)\//.test(raw)
            ? "core"
            : "client";
      return {
        id: `${index}:${raw}`,
        time,
        level,
        source,
        message: content.replace(/^\[(?:TRACE|DEBUG|INFO|WARN|WARNING|ERROR|FATAL|PANIC)\]\s*/, ""),
        text,
      };
    })
    .sort((a, b) => a.time.localeCompare(b.time))
    .slice(-LOG_WINDOW_LIMIT);
}

export function filterLogs(
  entries: LogEntry[],
  query: string,
  level: string,
  source: string,
  options: SearchOptions = { caseSensitive: false, wholeWord: false, regex: false }
): LogEntry[] {
  const search = connectionSearch(query, options);
  return entries.filter(
    entry =>
      (level === "all" || entry.level === level) &&
      (source === "all" || entry.source === source) &&
      search.matches(entry.text)
  );
}

export function logRange(count: number, scrollTop: number, height: number) {
  const start = Math.max(0, Math.min(count, Math.floor(scrollTop / 28) - 12));
  const end = Math.min(count, start + Math.ceil(height / 28) + 24);
  return { start, end, before: start * 28, after: (count - end) * 28 };
}
