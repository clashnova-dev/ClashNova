import type { Locale } from "../i18n";

export type Preferences = {
  version: 1;
  locale: Locale;
  showTraySpeed: boolean;
  theme: "system" | "light" | "dark";
  baseUrl: string;
  email: string;
  connectionMode: "system" | "manual" | "tun";
  routeMode: "rule" | "global";
  activeProfileId: string;
  nodeSort: "default" | "latency";
  // How many nodes are latency-tested at once (1–64).
  probeConcurrency: number;
  selectedNodes: Record<string, string>;
  // Ordered node ids (entry → exit) forming the applied proxy chain, keyed by profile.
  chains: Record<string, string[]>;
  // Saved proxy-chain presets, keyed by profile.
  chainGroups: Record<string, { id: string; name: string; nodes: string[] }[]>;
};

export const preferencesKey = "airport.preferences.v1";

// Explicitly allow only non-secret preferences; never serialize application state.
export function parsePreferences(raw: string | null): Preferences | null {
  try {
    const value = JSON.parse(raw || "null");
    if (value?.version !== 1) return null;
    const selectedNodes = Object.fromEntries(Object.entries(value.selectedNodes || {})
      .filter(([key, node]) => key.length <= 512 && typeof node === "string" && node.length <= 512)) as Record<string, string>;
    const chains = Object.fromEntries(Object.entries(value.chains || {})
      .filter(([key, ids]) => key.length <= 512 && Array.isArray(ids) && ids.every((id) => typeof id === "string" && id.length <= 512))
      .map(([key, ids]) => [key, (ids as string[]).slice(0, 16)])) as Record<string, string[]>;
    const chainGroups = Object.fromEntries(Object.entries(value.chainGroups || {})
      .filter(([key, list]) => key.length <= 512 && Array.isArray(list))
      .map(([key, list]) => [key, (list as unknown[])
        .filter((g): g is { id: string; name: string; nodes: string[] } => !!g && typeof (g as { id: unknown }).id === "string" && typeof (g as { name: unknown }).name === "string" && Array.isArray((g as { nodes: unknown }).nodes))
        .slice(0, 50)
        .map((g) => ({ id: g.id.slice(0, 64), name: g.name.slice(0, 80), nodes: g.nodes.filter((n) => typeof n === "string" && n.length <= 512).slice(0, 16) }))])) as Record<string, { id: string; name: string; nodes: string[] }[]>;
    return {
      version: 1,
      locale: (["zh-CN", "en", "ja", "ko", "de", "fr", "ru"] as Locale[]).includes(value.locale) ? value.locale as Locale : "en",
      showTraySpeed: value.showTraySpeed === true,
      theme: value.theme === "light" || value.theme === "dark" ? value.theme : "system",
      baseUrl: typeof value.baseUrl === "string" ? value.baseUrl : "",
      email: typeof value.email === "string" ? value.email : "",
      connectionMode: ["system", "manual", "tun"].includes(value.connectionMode) ? value.connectionMode : "system",
      routeMode: value.routeMode === "global" ? "global" : "rule",
      activeProfileId: typeof value.activeProfileId === "string" ? value.activeProfileId : "",
      nodeSort: value.nodeSort === "latency" ? "latency" : "default",
      probeConcurrency: Number.isInteger(value.probeConcurrency) ? Math.min(64, Math.max(1, value.probeConcurrency)) : 8,
      selectedNodes,
      chains,
      chainGroups,
    };
  } catch { return null; }
}
