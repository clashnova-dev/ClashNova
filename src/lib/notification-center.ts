export type NoticeKind = 'info' | 'success' | 'error';
export type Notice = { title?: string; body: string; kind?: NoticeKind; native?: boolean; nativeBody?: string; toast?: boolean; key?: string };
export type NotificationAdapter = { permission: () => Promise<boolean>; request: () => Promise<string>; send: (title: string, body: string) => void | Promise<void> };
/** Native failures must never turn a completed network operation into a failure. */
export function createNotificationCenter(adapter: NotificationAdapter, toast: (notice: Notice) => void, enabled: () => boolean, now = Date.now) {
  const recent = new Map<string, number>();
  let requesting: Promise<boolean> | null = null;
  let attempted = false;
  async function permission(request = false) {
    try {
      if (await adapter.permission()) return true;
      if (requesting) return await requesting;
      if (!request || attempted) return false;
      if (!requesting) { attempted = true; requesting = adapter.request().then(result => result === 'granted').catch(() => false).finally(() => { requesting = null; }); }
      return await requesting;
    } catch { return false; }
  }
  return {
    permission,
    async show(notice: Notice) {
      const id = notice.key || `${notice.kind}:${notice.title}:${notice.body}`;
      const timestamp = now();
      if (timestamp - (recent.get(id) ?? -Infinity) < 3000) return "duplicate" as const;
      recent.set(id, timestamp);
      for (const [key, time] of recent) if (timestamp - time > 3000) recent.delete(key);
      if (notice.toast !== false) toast(notice);
      if (!notice.native || !enabled()) return 'in-app' as const;
      if (!await permission()) return 'permission-denied' as const;
      try { await adapter.send(notice.title || 'ClashNova', notice.nativeBody || notice.body); return 'sent' as const; }
      catch { return 'failed' as const; }
    },
  };
}
