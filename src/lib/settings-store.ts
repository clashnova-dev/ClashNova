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
  try {
    return parsePreferences(typeof value === "string" ? value : JSON.stringify(value));
  } catch {
    return null;
  }
}

/** Preferences written to the store. Credential fields are dropped even if a caller sneaks them in. */
export function preferencesForStore(value: Preferences): Preferences | null {
  const raw = { ...value } as Preferences & Record<string, unknown>;
  for (const field of secretFields) delete raw[field];
  return preferencesFromUnknown(raw);
}

/** Desktop files are authoritative. Legacy webview values are read only for migration. */
export async function loadStoredPreferences(
  store: SettingStore | null,
  readLegacy: () => string | null,
  status?: StoreStatus,
  clearLegacy: () => void = () => {}
): Promise<Preferences | null> {
  const mark = (useStore: boolean) => {
    if (status) status.useStore = useStore;
  };
  if (!store) {
    mark(false);
    return null;
  }
  try {
    const current = await store.get(preferencesStoreKey);
    if (current != null) {
      const parsed = preferencesFromUnknown(current);
      if (!parsed) throw new Error("error.preferencesNotSaved");
      mark(true);
      clearLegacy();
      return parsed;
    }
    let migrated: Preferences | null = null;
    try {
      migrated = preferencesFromUnknown(readLegacy());
    } catch {
      /* No readable legacy data. */
    }
    if (!migrated) {
      mark(true);
      return null;
    }
    const clean = preferencesForStore(migrated)!;
    await store.set(preferencesStoreKey, clean);
    await store.save();
    if (JSON.stringify(preferencesFromUnknown(await store.get(preferencesStoreKey))) !== JSON.stringify(clean))
      throw new Error("error.preferencesNotSaved");
    mark(true);
    clearLegacy();
    return clean;
  } catch (error) {
    mark(false);
    throw error;
  }
}

export async function saveStoredPreferences(
  store: SettingStore | null,
  value: Preferences,
  status?: StoreStatus
): Promise<void> {
  const clean = preferencesForStore(value);
  if (!clean || !store || status?.useStore === false) throw new Error("error.preferencesNotSaved");
  try {
    await store.set(preferencesStoreKey, clean);
    await store.save();
    if (status) status.useStore = true;
  } catch (error) {
    if (status) status.useStore = false;
    throw error;
  }
}

/** Uses the stored value when it is present. Otherwise copies the previous copy in once. */
export async function rememberStored<T>(
  store: SettingStore | null,
  key: string,
  legacy: T,
  accept: (value: unknown) => T | null,
  status?: StoreStatus
): Promise<T> {
  if (!store || status?.useStore === false) return legacy;
  let current: unknown;
  try {
    current = await store.get(key);
  } catch {
    if (status) status.useStore = false;
    return legacy;
  }
  const parsed = accept(current);
  if (parsed != null) {
    if (status) status.useStore = true;
    return parsed;
  }
  try {
    await store.set(key, legacy);
    await store.save();
    if (status) status.useStore = true;
  } catch {
    if (status) status.useStore = false;
  }
  return legacy;
}

export async function writeStored(
  store: SettingStore | null,
  key: string,
  value: unknown,
  status?: StoreStatus
): Promise<void> {
  if (!store || status?.useStore === false) throw new Error("error.preferencesNotSaved");
  try {
    await store.set(key, value);
    await store.save();
    if (status) status.useStore = true;
  } catch (error) {
    if (status) status.useStore = false;
    throw error;
  }
}

/** A web preview keeps state in memory; it never writes browser storage. */
const previewValues = new Map<string, unknown>();
const previewStore: SettingStore = {
  async get(key) {
    return previewValues.get(key);
  },
  async set(key, value) {
    previewValues.set(key, structuredClone(value));
  },
  async save() {},
};
let nativeStore: Promise<SettingStore> | undefined;
export async function openSettingsStore(): Promise<SettingStore> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return previewStore;
  return (nativeStore ??= import("@tauri-apps/api/core").then(({ invoke }) => ({
    get: (key: string) => invoke("get_client_setting", { key }),
    set: (key: string, value: unknown) => invoke<void>("set_client_setting", { key, value }),
    save: async () => {}, // Native commits each write atomically.
  })));
}

/** Migration-only access. Failed native reads/writes never fall back to the webview. */
export function readLegacyClientValue(key: string): string | null {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return null;
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}
export function removeLegacyClientValue(key: string): void {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  try {
    window.localStorage.removeItem(key);
  } catch {
    /* Native data is already durable. */
  }
}

export async function migrateClientSetting<T>(
  store: SettingStore,
  key: string,
  fallback: T,
  accept: (value: unknown) => T | null,
  legacy?: {
    read: () => string | null;
    parse: (raw: string) => T | null;
    clear: () => void;
  }
): Promise<T> {
  const current = await store.get(key);
  if (current != null) {
    const parsed = accept(current);
    if (parsed === null) throw new Error("error.preferencesNotSaved");
    legacy?.clear();
    return parsed;
  }
  const raw = legacy?.read();
  const migrated = raw == null ? null : legacy!.parse(raw);
  if (migrated === null) return fallback;
  await store.set(key, migrated);
  await store.save();
  const checked = accept(await store.get(key));
  if (JSON.stringify(checked) !== JSON.stringify(migrated)) throw new Error("error.preferencesNotSaved");
  legacy?.clear();
  return migrated;
}

export async function readClientSetting<T>(
  key: string,
  fallback: T,
  accept: (value: unknown) => T | null,
  legacy?: { key: string; parse: (raw: string) => T | null }
): Promise<T> {
  return migrateClientSetting(
    await openSettingsStore(),
    key,
    fallback,
    accept,
    legacy
      ? {
          read: () => readLegacyClientValue(legacy.key),
          parse: legacy.parse,
          clear: () => removeLegacyClientValue(legacy.key),
        }
      : undefined
  );
}
export async function writeClientSetting(key: string, value: unknown, legacyKey?: string): Promise<void> {
  const store = await openSettingsStore();
  await store.set(key, value);
  await store.save();
  if (legacyKey) removeLegacyClientValue(legacyKey);
}
export const acceptBoolean = (value: unknown): boolean | null => (typeof value === "boolean" ? value : null);
export const legacyBoolean = (raw: string): boolean | null => (raw === "true" ? true : raw === "false" ? false : null);
