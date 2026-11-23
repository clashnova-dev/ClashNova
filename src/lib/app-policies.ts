// 进程级网络管理:每个应用(按进程)的联网策略。
//   inherit = 跟随规则(默认,用当前规则模式的路由,不额外加规则)
//   proxy   = 指定代理(走用户选择的代理节点/组,即 select 出站)
//   direct  = 直连(本地网络,不经代理)
//   block   = 阻断联网(拒绝该应用匹配到的连接,sing-box 规则 action:reject)
// 策略落到真实路由在 buildProfileConfig 里按应用注入 process_path_regex 规则。

export type AppPolicy = "inherit" | "proxy" | "direct" | "block";
export const APP_POLICIES: AppPolicy[] = ["inherit", "proxy", "direct", "block"];

export function isAppPolicy(value: unknown): value is AppPolicy {
  return value === "inherit" || value === "proxy" || value === "direct" || value === "block";
}

export function parseAppPolicies(value: unknown): Record<string, AppPolicy> {
  if (!value || typeof value !== "object") return {};
  const out: Record<string, AppPolicy> = {};
  for (const [key, policy] of Object.entries(value as Record<string, unknown>)) {
    // inherit 是默认值,无需落库;只保存显式改过的应用。
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

// 从进程路径提取应用名:macOS 取 .app 包名,其它取可执行文件名。
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

// 把应用名转成匹配其全部进程路径的正则(覆盖多进程应用,如浏览器的 Helper 子进程)。
export function appPathRegex(app: string): string {
  const name = app.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // Match an executable/bundle boundary, never another application's name as a substring.
  // Windows process paths are case-insensitive; macOS/Linux retain case-sensitive names.
  return /\.exe$/i.test(app)
    ? `(?i)(?:^|[\\\\/])${name}$`
    : `(?:^|/)${name}(?:\\.app/|$)`;
}

// 由已配置策略生成 sing-box route 规则(注入在默认/中国直连规则之前,优先生效)。
export function appPolicyRules(policies: Record<string, AppPolicy>): unknown[] {
  const groups: Record<"block" | "proxy" | "direct", string[]> = { block: [], proxy: [], direct: [] };
  for (const [app, policy] of Object.entries(policies)) {
    if (app && (policy === "block" || policy === "proxy" || policy === "direct")) groups[policy].push(appPathRegex(app));
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
  return (['block', 'proxy', 'direct'] as const).map(policy => ({
    inbound: [`clashnova-app-${policy}`],
    process_path_regex: Object.entries(policies).filter(([, value]) => value === policy).map(([app]) => appPathRegex(app)).concat('^$'),
    ...(policy === 'block' ? { action: 'reject' } : { outbound: policy === 'proxy' ? 'select' : 'direct' }),
  }));
}

export type AppConnection = {
  metadata: { processPath: string }; download: number; upload: number; downloadSpeed: number; uploadSpeed: number;
};
export type ApplicationGroup<T> = { app: string; exe: string; path: string; rows: T[]; download: number; upload: number; downloadSpeed: number; uploadSpeed: number };

/** Configured applications remain manageable even after blocking removes every live connection. */
export function applicationGroups<T extends AppConnection>(active: T[], closed: T[], policies: Record<string, AppPolicy>): ApplicationGroup<T>[] {
  const groups = new Map<string, ApplicationGroup<T>>();
  const ensure = (app: string, path = '') => {
    let group = groups.get(app);
    if (!group) {
      group = { app, exe: '', path: '', rows: [], download: 0, upload: 0, downloadSpeed: 0, uploadSpeed: 0 };
      groups.set(app, group);
    }
    if (path && !group.path) { group.path = path; group.exe = processExecutablePath(path).split(/[\\/]/).pop() || ''; }
    return group;
  };
  for (const [app, policy] of Object.entries(policies)) if (app && policy !== 'inherit') ensure(app);
  for (const row of active) {
    const group = ensure(appNameFromPath(row.metadata.processPath), row.metadata.processPath);
    group.rows.push(row);
    group.download += row.download; group.upload += row.upload;
    group.downloadSpeed += row.downloadSpeed; group.uploadSpeed += row.uploadSpeed;
  }
  for (const row of closed) {
    const app = appNameFromPath(row.metadata.processPath);
    if (groups.has(app)) ensure(app, row.metadata.processPath);
  }
  return [...groups.values()].sort((a, b) => b.downloadSpeed - a.downloadSpeed || b.download - a.download || a.app.localeCompare(b.app));
}

// Use the same process identity as the application groups, never a destination/host substring.
// null means all applications; an empty name is the explicit unknown-process group.
export function connectionsForApp<T extends { metadata: { processPath: string } }>(rows: T[], app: string | null): T[] {
  return app === null ? rows : rows.filter(row => appNameFromPath(row.metadata.processPath) === app);
}
