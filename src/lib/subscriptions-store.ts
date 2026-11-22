import { subscriptionsStoreKey, type SettingStore, type StoreStatus } from './settings-store.ts';

type SubscriptionFiles = {
  loadSubscriptions(): Promise<string>;
  saveSubscriptions(data: string): Promise<void>;
};

async function clearLegacyCopy(store: SettingStore | null, status?: StoreStatus) {
  if (!store || status?.useStore === false) return;
  try {
    if (await store.get(subscriptionsStoreKey) == null) return;
    await store.set(subscriptionsStoreKey, null);
    await store.save();
  } catch { /* A cleanup failure cannot invalidate the authoritative, durable file. */ }
}

/** The dedicated file wins, including an intentionally empty list. The old setting is migration-only. */
export async function loadExternalSubscriptions(
  files: SubscriptionFiles, legacy: SettingStore | null, status?: StoreStatus,
): Promise<unknown[]> {
  const current: unknown = JSON.parse(await files.loadSubscriptions());
  if (Array.isArray(current)) {
    await clearLegacyCopy(legacy, status);
    return current;
  }
  if (current !== null) throw new Error('native.subscriptionListInvalid');
  // `null` means the file is absent; a failed read must never look like an empty file.
  if (!legacy || status?.useStore === false) throw new Error('error.preferencesNotSaved');
  const previous = await legacy.get(subscriptionsStoreKey);
  if (previous == null) return [];
  if (!Array.isArray(previous)) throw new Error('native.subscriptionListInvalid');
  await files.saveSubscriptions(JSON.stringify(previous));
  const checked: unknown = JSON.parse(await files.loadSubscriptions());
  if (JSON.stringify(checked) !== JSON.stringify(previous)) throw new Error('error.preferencesNotSaved');
  await clearLegacyCopy(legacy, status);
  return previous;
}
