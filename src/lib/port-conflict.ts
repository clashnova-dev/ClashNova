export type PortConflict = { port: number; token: string; listeners: { pid: number; name: string }[] };

/** Foreign processes can only be stopped after the user sees and approves this exact snapshot. */
export async function resolveProxyPort(deps: {
  inspect(): Promise<PortConflict | null>;
  confirm(conflict: PortConflict): Promise<boolean>;
  release(token: string): Promise<void>;
}): Promise<boolean> {
  const conflict = await deps.inspect();
  if (!conflict) return true;
  if (!await deps.confirm(conflict)) return false;
  await deps.release(conflict.token);
  return true;
}
