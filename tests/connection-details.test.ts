import assert from 'node:assert/strict';
import test from 'node:test';
import { connectionEndpoint, refreshConnectionSelection, type ConnectionRow } from '../src/lib/connection-details.ts';

const row: ConnectionRow = {
  id: 'one', metadata: { host: 'example.test', destinationIP: '::1', destinationPort: '443', processPath: '' },
  upload: 10, download: 20, uploadSpeed: 2, downloadSpeed: 4, start: '2026-11-23T00:00:00Z', chains: ['select'],
};
test('connection endpoints preserve IPv6 boundaries and missing observations', () => {
  assert.equal(connectionEndpoint('::1', '443'), '[::1]:443');
  assert.equal(connectionEndpoint('[::1]', '443'), '[::1]:443');
  assert.equal(connectionEndpoint('example.test', ''), 'example.test');
  assert.equal(connectionEndpoint('', '443'), '—');
});
test('selected connection follows live samples then retains final counters after closing and history eviction', () => {
  const selected = { session: 1, row };
  const live = { ...row, download: 40 };
  const updated = refreshConnectionSelection(selected, { session: 1, active: [live], closed: [] });
  assert.equal(updated?.row.download, 40);
  const final = { ...live, download: 60, downloadSpeed: 0, uploadSpeed: 0 };
  const closed = refreshConnectionSelection(updated, { session: 1, active: [], closed: [final] });
  assert.deepEqual(closed?.row, final);
  assert.deepEqual(refreshConnectionSelection(closed, { session: 1, active: [], closed: [] })?.row, final);
});
test('new core sessions cannot replace selected details with a reused connection ID', () => {
  assert.equal(refreshConnectionSelection({ session: 1, row }, { session: 2, active: [row], closed: [] }), undefined);
  assert.equal(refreshConnectionSelection(undefined, { session: 1, active: [row], closed: [] }), undefined);
});
