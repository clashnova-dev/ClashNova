/** Notification permission is optional and may wait indefinitely for user input. */
export async function prepareConnection(deps: {
  saveProfile(): Promise<unknown>;
  ensureTunnel(): Promise<unknown>;
  prepareNotifications(): Promise<unknown>;
}): Promise<void> {
  await Promise.all([deps.saveProfile(), deps.ensureTunnel()]);
  void Promise.resolve().then(() => deps.prepareNotifications()).catch(() => {
    // The connection must work even if the OS notification API is unavailable or denied.
  });
}
