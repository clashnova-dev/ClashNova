// Per-process network management: each application's (by process) connectivity policy.
//   inherit = follow the rules (default; uses the current rule mode's routing, adds no extra rule)
//   proxy   = force proxy (route through the user's selected proxy node/group, i.e. the select outbound)
//   direct  = direct connection (local network, bypassing the proxy)
//   block   = block connectivity (reject the app's matched connections, sing-box rule action:reject)
// Policies are applied to real routing in buildProfileConfig, which injects a process_path_regex rule per app.

export type AppPolicy = "inherit" | "proxy" | "direct" | "block";
export const APP_POLICIES: AppPolicy[] = ["inherit", "proxy", "direct", "block"];

export function isAppPolicy(value: unknown): value is AppPolicy {
  return value === "inherit" || value === "proxy" || value === "direct" || value === "block";
}

export function parseAppPolicies(value: unknown): Record<string, AppPolicy> {
  if (!value || typeof value !== "object") return {};
  const out: Record<string, AppPolicy> = {};
  for (const [key, policy] of Object.entries(value as Record<string, unknown>)) {
    // inherit is the default and need not be persisted; only store apps that were explicitly changed.
    if (key.length > 0 && key.length <= 256 && isAppPolicy(policy) && policy !== "inherit") out[key] = policy;
  }
  return out;
}

// Clash API appends the owning user to the path ("/path/app (user)").
// A user id by itself is not an identified process and must not become a routing rule.
export function processExecutablePath(value: string): string {
  if (!value.startsWith("/") && !/^[a-z]:[\\/]/i.test(value)) return "";
  return value.replace(/ \([^()\\/]+\)$/, "");
}

// Extract the app name from a process path: the .app bundle name on macOS, the executable name elsewhere.
export function appNameFromPath(path: string): string {
  path = processExecutablePath(path);
  if (!path) return "";
  const appIdx = path.indexOf(".app/");
  if (appIdx >= 0) {
    const seg = path.slice(0, appIdx);
    return seg.slice(seg.lastIndexOf("/") + 1);
  }
  const base = path.split(/[\\/]/).pop() || path;
  return base;
}

// Turn an app name into a regex matching all of its process paths (covers multi-process apps, e.g. a browser's Helper subprocesses).
export function appPathRegex(app: string): string {
  const name = app.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // Match an executable/bundle boundary, never another application's name as a substring.
  // Windows process paths are case-insensitive; macOS/Linux retain case-sensitive names.
  return /\.exe$/i.test(app) ? `(?i)(?:^|[\\\\/])${name}$` : `(?:^|/)${name}(?:\\.app/|$)`;
}

// Build sing-box route rules from the configured policies (injected before the default/China-direct rules so they take precedence).
export function appPolicyRules(policies: Record<string, AppPolicy>): unknown[] {
  const groups: Record<"block" | "proxy" | "direct", string[]> = {
    block: [],
    proxy: [],
    direct: [],
  };
  for (const [app, policy] of Object.entries(policies)) {
    if (app && (policy === "block" || policy === "proxy" || policy === "direct"))
      groups[policy].push(appPathRegex(app));
  }
  const rules: unknown[] = [];
  if (groups.block.length) rules.push({ process_path_regex: groups.block, action: "reject" });
  if (groups.proxy.length) rules.push({ process_path_regex: groups.proxy, outbound: "select" });
  if (groups.direct.length) rules.push({ process_path_regex: groups.direct, outbound: "direct" });
  return rules;
}

// Reserved matchers are installed once by the bundled core, before it starts.
// Their contents can then change without restarting the tunnel or other apps.
export function liveAppPolicyRules(policies: Record<string, AppPolicy>): unknown[] {
  return (["block", "proxy", "direct"] as const).map(policy => ({
    inbound: [`clashnova-app-${policy}`],
    process_path_regex: Object.entries(policies)
      .filter(([, value]) => value === policy)
      .map(([app]) => appPathRegex(app))
      .concat("^$"),
    ...(policy === "block" ? { action: "reject" } : { outbound: policy === "proxy" ? "select" : "direct" }),
  }));
}

export type AppConnection = {
  metadata: { processPath: string };
  download: number;
  upload: number;
  downloadSpeed: number;
  uploadSpeed: number;
};
export type ApplicationGroup<T> = {
  app: string;
  exe: string;
  path: string;
  rows: T[];
  download: number;
  upload: number;
  downloadSpeed: number;
  uploadSpeed: number;
};

/** Configured applications remain manageable even after blocking removes every live connection. */
export function applicationGroups<T extends AppConnection>(
  active: T[],
  closed: T[],
  policies: Record<string, AppPolicy>
): ApplicationGroup<T>[] {
  const groups = new Map<string, ApplicationGroup<T>>();
  const ensure = (app: string, path = "") => {
    let group = groups.get(app);
    if (!group) {
      group = {
        app,
        exe: "",
        path: "",
        rows: [],
        download: 0,
        upload: 0,
        downloadSpeed: 0,
        uploadSpeed: 0,
      };
      groups.set(app, group);
    }
    if (path && !group.path) {
      group.path = path;
      group.exe = processExecutablePath(path).split(/[\\/]/).pop() || "";
    }
    return group;
  };
  for (const [app, policy] of Object.entries(policies)) if (app && policy !== "inherit") ensure(app);
  for (const row of active) {
    const group = ensure(appNameFromPath(row.metadata.processPath), row.metadata.processPath);
    group.rows.push(row);
    group.download += row.download;
    group.upload += row.upload;
    group.downloadSpeed += row.downloadSpeed;
    group.uploadSpeed += row.uploadSpeed;
  }
  for (const row of closed) {
    const app = appNameFromPath(row.metadata.processPath);
    if (groups.has(app)) ensure(app, row.metadata.processPath);
  }
  return [...groups.values()].sort(
    (a, b) => b.downloadSpeed - a.downloadSpeed || b.download - a.download || a.app.localeCompare(b.app)
  );
}

// Use the same process identity as the application groups, never a destination/host substring.
// null means all applications; an empty name is the explicit unknown-process group.
export function connectionsForApp<T extends { metadata: { processPath: string } }>(rows: T[], app: string | null): T[] {
  return app === null ? rows : rows.filter(row => appNameFromPath(row.metadata.processPath) === app);
}
