import { parsePreferences, type Preferences } from "./preferences.ts";

/** Private fixed-name settings file; retains the existing plugin-store file for migration. */
export const settingsStoreFile = "airport-settings.json";
export const preferencesStoreKey = "preferences";
export const networkSettingsStoreKey = "networkSettings";
export const networkSecurityStoreKey = "networkSecurity";
export const appPoliciesStoreKey = "appPolicies";
export const subscriptionsStoreKey = "subscriptions";

const secretFields = ["password", "accessToken", "refreshToken", "token", "session"];

export interface SettingStore {
  get(key: string): Promise<unknown>;
  set(key: string, value: unknown): Promise<void>;
  save(): Promise<void>;
}

/** `useStore` is false after a failed read, so a later save cannot overwrite the store with the fallback copy. */
export type StoreStatus = { useStore: boolean };

export function preferencesFromUnknown(value: unknown): Preferences | null {
  if (value == null) return null;
  try { return parsePreferences(typeof value === "string" ? value : JSON.stringify(value)); }
  catch { return null; }
}

/** Preferences written to the store. Credential fields are dropped even if a caller sneaks them in. */
export function preferencesForStore(value: Preferences): Preferences | null {
  const raw = { ...value } as Preferences & Record<string, unknown>;
  for (const field of secretFields) delete raw[field];
  return preferencesFromUnknown(raw);
}

/**
 * Desktop reads the fixed-file native store. A missing key is copied once from the old localStorage value.
 * Browser preview has no plugin and keeps using that localStorage value.
 * A store read failure also falls back, and does not overwrite the store.
 */
export async function loadStoredPreferences(
  store: SettingStore | null,
  readLegacy: () => string | null,
  status?: StoreStatus,
): Promise<Preferences | null> {
  const legacy = () => {
    try { return parsePreferences(readLegacy()); }
    catch { return null; }
  };
  const mark = (useStore: boolean) => { if (status) status.useStore = useStore; };
  if (!store) { mark(false); return legacy(); }
  let current: unknown;
  try { current = await store.get(preferencesStoreKey); }
  catch { mark(false); return legacy(); }
  if (current != null) { mark(true); return preferencesFromUnknown(current); }
  const migrated = legacy();
  if (!migrated) { mark(true); return null; }
  try {
    await store.set(preferencesStoreKey, migrated);
    await store.save();
    mark(true);
  } catch { mark(false); /* The old copy is still there; the next launch can try again. */ }
  return migrated;
}

export async function saveStoredPreferences(
  store: SettingStore | null,
  value: Preferences,
  writeLegacy: (raw: string) => void,
  status?: StoreStatus,
): Promise<void> {
  const clean = preferencesForStore(value);
  if (!clean) throw new Error("error.preferencesNotSaved");
  if (store && status?.useStore !== false) {
    try {
      await store.set(preferencesStoreKey, clean);
      await store.save();
      if (status) status.useStore = true;
      return;
    } catch { if (status) status.useStore = false; /* Plugin unavailable; keep the previous local cache. */ }
  }
  writeLegacy(JSON.stringify(clean));
}

/** Uses the stored value when it is present. Otherwise copies the previous copy in once. */
export async function rememberStored<T>(
  store: SettingStore | null,
  key: string,
  legacy: T,
  accept: (value: unknown) => T | null,
  status?: StoreStatus,
): Promise<T> {
  if (!store || status?.useStore === false) return legacy;
  let current: unknown;
  try { current = await store.get(key); }
  catch { if (status) status.useStore = false; return legacy; }
  const parsed = accept(current);
  if (parsed != null) { if (status) status.useStore = true; return parsed; }
  try {
    await store.set(key, legacy);
    await store.save();
    if (status) status.useStore = true;
  } catch { if (status) status.useStore = false; }
  return legacy;
}

export async function writeStored(
  store: SettingStore | null,
  key: string,
  value: unknown,
  status?: StoreStatus,
): Promise<void> {
  if (!store || status?.useStore === false) return;
  try {
    await store.set(key, value);
    await store.save();
    if (status) status.useStore = true;
  } catch { if (status) status.useStore = false; }
}

/** Null when this is a browser preview or the plugin cannot be loaded. */
export async function openSettingsStore(): Promise<SettingStore | null> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return null;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    // Native owns the filename, key allowlist and private atomic writes.
    await invoke("get_client_setting", { key: preferencesStoreKey });
    return {
      get: key => invoke("get_client_setting", { key }),
      set: (key, value) => invoke<void>("set_client_setting", { key, value }),
      save: async () => {}, // Each native set is committed atomically before returning.
    };
  } catch {
    return null;
  }
}
