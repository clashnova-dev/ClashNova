/** Notification permission is optional and may wait indefinitely for user input. */
export async function prepareConnection(deps: {
  saveProfile(): Promise<unknown>;
  ensureTunnel(): Promise<unknown>;
  prepareNotifications(): Promise<unknown>;
}): Promise<void> {
  // A rejected task must not release the UI while its sibling can still change
  // the saved profile or finish installing the tunnel service.
  const prepared = await Promise.allSettled([deps.saveProfile(), deps.ensureTunnel()]);
  for (const result of prepared) if (result.status === 'rejected') throw result.reason;
  void Promise.resolve().then(() => deps.prepareNotifications()).catch(() => {
    // The connection must work even if the OS notification API is unavailable or denied.
  });
}
