import assert from 'node:assert/strict';
import test from 'node:test';
import { loadExternalSubscriptions } from '../src/lib/subscriptions-store.ts';
import type { SettingStore } from '../src/lib/settings-store.ts';

const old = [{ id: 'kept', raw: 'old' }, { id: 'deleted', raw: 'removed' }];
function legacy(value: unknown = old): SettingStore {
  return { async get() { return value; }, async set() { throw new Error('must not rewrite legacy'); }, async save() {} };
}
function file(value: unknown) {
  let contents = JSON.stringify(value), writes = 0;
  return {
    async loadSubscriptions() { return contents; },
    async saveSubscriptions(data: string) { contents = data; writes++; },
    get writes() { return writes; },
  };
}

test('updated dedicated subscriptions win over stale mirrored settings', async () => {
  const updated = [{ id: 'kept', raw: 'updated' }];
  const files = file(updated);
  assert.deepEqual(await loadExternalSubscriptions(files, legacy()), updated);
  assert.equal(files.writes, 0);
});
test('deleting the final subscription does not resurrect the old list', async () => {
  const files = file([]);
  assert.deepEqual(await loadExternalSubscriptions(files, legacy()), []);
  assert.equal(files.writes, 0);
});
test('valid dedicated files remain usable even when general preferences fail', async () => {
  const files = file([{ id: 'native' }]);
  assert.deepEqual(await loadExternalSubscriptions(files, null, { useStore: false }), [{ id: 'native' }]);
});
test('a missing dedicated file migrates and verifies the old list once', async () => {
  const files = file(null);
  assert.deepEqual(await loadExternalSubscriptions(files, legacy()), old);
  assert.equal(files.writes, 1);
  assert.deepEqual(await loadExternalSubscriptions(files, legacy([{ id: 'stale' }])), old);
  assert.equal(files.writes, 1);
});
test('the stale shadow is cleared only after a verified dedicated copy exists', async () => {
  let shadow: unknown = structuredClone(old);
  const store: SettingStore = { async get() { return shadow; }, async set(_key, value) { shadow = value; }, async save() {} };
  const files = file(null);
  await loadExternalSubscriptions(files, store);
  assert.equal(shadow, null);
  assert.deepEqual(JSON.parse(await files.loadSubscriptions()), old);
  shadow = old;
  const broken = { async loadSubscriptions() { return 'null'; }, async saveSubscriptions() {} };
  await assert.rejects(loadExternalSubscriptions(broken, store), /preferencesNotSaved/);
  assert.deepEqual(shadow, old);
});
test('failed migration preserves the only old copy and reports failure', async () => {
  const files = { async loadSubscriptions() { return 'null'; }, async saveSubscriptions() { throw new Error('disk full'); } };
  await assert.rejects(loadExternalSubscriptions(files, legacy()), /disk full/);
  assert.deepEqual(await legacy().get('subscriptions'), old);
  const unreadable = { ...files, async saveSubscriptions() {} };
  await assert.rejects(loadExternalSubscriptions(unreadable, legacy()), /preferencesNotSaved/);
});
test('read errors or malformed dedicated data never fall back to a stale mirror', async () => {
  for (const data of ['broken', '{}', '"wrong type"']) {
    const files = { async loadSubscriptions() { return data; }, async saveSubscriptions() { throw new Error('must not overwrite'); } };
    await assert.rejects(loadExternalSubscriptions(files, legacy()));
  }
  const files = { async loadSubscriptions(): Promise<string> { throw new Error('read denied'); }, async saveSubscriptions() {} };
  await assert.rejects(loadExternalSubscriptions(files, legacy()), /read denied/);
});
test('missing files cannot overwrite unreadable or invalid legacy data with defaults', async () => {
  const files = file(null);
  await assert.rejects(loadExternalSubscriptions(files, null), /preferencesNotSaved/);
  await assert.rejects(loadExternalSubscriptions(files, legacy(), { useStore: false }), /preferencesNotSaved/);
  await assert.rejects(loadExternalSubscriptions(files, legacy({ bad: true })), /subscriptionListInvalid/);
  assert.equal(files.writes, 0);
  assert.deepEqual(await loadExternalSubscriptions(files, legacy(null)), []);
  assert.equal(files.writes, 0);
});
