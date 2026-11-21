import assert from "node:assert/strict";
import test from "node:test";
import { loadStoredPreferences, networkSettingsStoreKey, preferencesStoreKey, rememberStored, saveStoredPreferences, subscriptionsStoreKey, type SettingStore } from "../src/lib/settings-store.ts";

const legacyPrefs = JSON.stringify({
  version: 1,
  password: "secret",
  accessToken: "access-secret",
  refreshToken: "refresh-secret",
  locale: "en",
  theme: "dark",
  connectionMode: "tun",
  routeMode: "global",
  email: "user@example.com",
  activeProfileId: "profile-1",
});

function memoryStore(initial: Record<string, unknown> = {}): SettingStore & { saved: Record<string, unknown> } {
  const saved = { ...initial };
  return {
    saved,
    async get(key) { return saved[key]; },
    async set(key, value) { saved[key] = value; },
    async save() {},
  };
}

test("旧偏好迁入 store 时不写入密码和令牌", async () => {
  const store = memoryStore();
  const loaded = await loadStoredPreferences(store, () => legacyPrefs);
  assert.equal(loaded?.locale, "en");
  assert.equal(loaded?.theme, "dark");
  assert.equal(loaded?.connectionMode, "tun");
  assert.equal(loaded?.routeMode, "global");
  assert.equal(loaded?.activeProfileId, "profile-1");
  const written = JSON.stringify(store.saved);
  assert.equal(Object.keys(store.saved).join(), preferencesStoreKey);
  assert.ok(!written.includes("secret"));
  assert.ok(!written.includes("password"));
  assert.ok(!written.includes("refreshToken"));
  assert.ok(!written.includes("accessToken"));
  assert.ok(!written.includes("airport.session"));
});

test("没有 store 时回退到 localStorage", async () => {
  let legacyReads = 0;
  const loaded = await loadStoredPreferences(null, () => {
    legacyReads += 1;
    return legacyPrefs;
  });
  assert.equal(legacyReads, 1);
  assert.equal(loaded?.locale, "en");
  assert.equal(loaded?.connectionMode, "tun");
  assert.ok(loaded);
  let written = "";
  await saveStoredPreferences(null, Object.assign({}, loaded, { password: "secret", refreshToken: "refresh-secret" }), (raw) => {
    written = raw;
  });
  assert.equal(JSON.parse(written).locale, "en");
  assert.ok(!written.includes("secret"));
  assert.ok(!written.includes("refreshToken"));
  assert.ok(!written.includes("password"));
});

test("store 已有偏好时不再迁移旧缓存", async () => {
  const store = memoryStore({
    [preferencesStoreKey]: { version: 1, locale: "fr", theme: "light", connectionMode: "manual", routeMode: "rule" },
  });
  const loaded = await loadStoredPreferences(store, () => { throw new Error("legacy should not be read"); });
  assert.equal(loaded?.locale, "fr");
  assert.equal(loaded?.connectionMode, "manual");
  assert.equal(store.saved.refreshToken, undefined);
});

test("store 读失败时回退且不覆盖 store", async () => {
  let writes = 0;
  const store: SettingStore = {
    async get() { throw new Error("plugin missing"); },
    async set() { writes += 1; },
    async save() { writes += 1; },
  };
  const status = { useStore: true };
  const loaded = await loadStoredPreferences(store, () => JSON.stringify({ version: 1, locale: "ko", routeMode: "global" }), status);
  assert.equal(loaded?.locale, "ko");
  assert.equal(status.useStore, false);
  assert.equal(writes, 0);
  assert.ok(loaded);
  await saveStoredPreferences(store, loaded, () => {}, status);
  assert.equal(writes, 0);
});

test("已有订阅和网络设置时不再用旧文件覆盖", async () => {
  const store = memoryStore({
    [subscriptionsStoreKey]: [{ id: "kept" }],
    [networkSettingsStoreKey]: { proxyPort: 2080, allowLan: false },
  });
  const subscriptions = await rememberStored(store, subscriptionsStoreKey, [{ id: "legacy" }], (value) => Array.isArray(value) ? value : null);
  const network = await rememberStored(store, networkSettingsStoreKey, { proxyPort: 1, allowLan: true }, (value) => value && typeof value === "object" ? value as { proxyPort: number } : null);
  assert.equal((subscriptions as { id: string }[])[0].id, "kept");
  assert.equal(network.proxyPort, 2080);
  assert.equal((store.saved[subscriptionsStoreKey] as { id: string }[])[0].id, "kept");
});
