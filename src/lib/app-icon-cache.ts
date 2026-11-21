import { processExecutablePath } from './app-policies.ts';

const MAX_ENTRIES = 64;
const MISS_TTL_MS = 60_000;

// Cache native bytes as Blobs; each mounted image owns its own object URL.
// The pending map also merges simultaneous list/detail requests for an app.
export function createAppIconCache(
  load: (path: string) => Promise<Blob | null>,
  now: () => number = Date.now,
) {
  const entries = new Map<string, { icon: Blob | null; expires: number }>();
  const pending = new Map<string, Promise<Blob | null>>();
  return function get(path: string): Promise<Blob | null> {
    const executable = processExecutablePath(path);
    if (!executable) return Promise.resolve(null);
    const bundleEnd = executable.indexOf('.app/');
    const key = bundleEnd >= 0 ? executable.slice(0, bundleEnd + 4) : executable;
    const cached = entries.get(key);
    if (cached && cached.expires > now()) {
      entries.delete(key);
      entries.set(key, cached);
      return Promise.resolve(cached.icon);
    }
    entries.delete(key);
    const inflight = pending.get(key);
    if (inflight) return inflight;
    const request = Promise.resolve().then(() => load(path)).catch(() => null).then(icon => {
      pending.delete(key);
      entries.set(key, { icon, expires: icon ? Infinity : now() + MISS_TTL_MS });
      if (entries.size > MAX_ENTRIES) entries.delete(entries.keys().next().value!);
      return icon;
    });
    pending.set(key, request);
    return request;
  };
}
