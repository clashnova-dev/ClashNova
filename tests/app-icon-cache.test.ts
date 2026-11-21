import test from 'node:test';
import assert from 'node:assert/strict';
import { createAppIconCache } from '../src/lib/app-icon-cache.ts';

test('concurrent list/detail and helper paths share one native load, including after remount', async () => {
  let calls = 0;
  let finish!: (icon: Blob) => void;
  const icon = new Blob(['png']);
  const get = createAppIconCache(() => {
    calls++;
    return new Promise(resolve => { finish = resolve; });
  });
  const main = '/Applications/Cursor.app/Contents/MacOS/Cursor (user)';
  const helper = '/Applications/Cursor.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper';
  const first = get(main);
  assert.equal(get(helper), first);
  await Promise.resolve();
  assert.equal(calls, 1);
  finish(icon);
  assert.equal(await first, icon);
  assert.equal(await get(helper), icon);
  assert.equal(calls, 1);
});

test('missing icons and rejected requests are cached briefly then retried', async () => {
  for (const fails of [false, true]) {
    let time = 0, calls = 0;
    const get = createAppIconCache(async () => {
      calls++;
      if (fails) throw new Error('native unavailable');
      return null;
    }, () => time);
    assert.equal(await get('/usr/bin/app'), null);
    assert.equal(await get('/usr/bin/app'), null);
    assert.equal(calls, 1);
    time = 60_000;
    await get('/usr/bin/app');
    assert.equal(calls, 2);
  }
});

test('bounded LRU evicts the oldest icon and preserves recently used entries', async () => {
  const calls = new Map<string, number>();
  const get = createAppIconCache(async path => {
    calls.set(path, (calls.get(path) || 0) + 1);
    return new Blob([path]);
  });
  for (let i = 0; i < 64; i++) await get(`/usr/bin/app${i}`);
  await get('/usr/bin/app0');
  await get('/usr/bin/app64');
  await get('/usr/bin/app0');
  await get('/usr/bin/app1');
  assert.equal(calls.get('/usr/bin/app0'), 1);
  assert.equal(calls.get('/usr/bin/app1'), 2);
});

test('unknown processes use fallback and Windows/Linux executable identities remain separate', async () => {
  let calls = 0;
  const get = createAppIconCache(async () => { calls++; return new Blob(['png']); });
  for (const path of ['', '501', 'unknown']) assert.equal(await get(path), null);
  assert.equal(calls, 0);
  await get('C:\\Program Files\\Example\\app.exe (user)');
  await get('C:\\Program Files\\Example\\app.exe (other)');
  await get('/opt/example/app');
  await get('/opt/other/app');
  assert.equal(calls, 3);
});
