// A normal launch and a manual disconnect never request recovery. In particular, automatic
// recovery must not install a privileged service or request notification permissions.
export async function restoreCrashConnection(deps: {
  requested(): Promise<boolean>;
  canConnect(): boolean;
  tun: boolean;
  tunReady(): Promise<boolean>;
  connect(): Promise<void>;
  report(key: "recovery.restoring" | "recovery.profileUnavailable" | "recovery.tunUnavailable"): void;
}): Promise<void> {
  if (!await deps.requested()) return;
  if (!deps.canConnect()) { deps.report("recovery.profileUnavailable"); return; }
  if (deps.tun && !await deps.tunReady()) { deps.report("recovery.tunUnavailable"); return; }
  deps.report("recovery.restoring");
  await deps.connect();
}
