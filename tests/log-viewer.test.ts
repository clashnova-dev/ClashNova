import test from 'node:test';
import assert from 'node:assert/strict';
import { filterLogs, logEntries, logRange, redactLog, LOG_WINDOW_LIMIT } from '../src/lib/log-viewer.ts';

test('parses app, native core and TUN severity without treating message words as levels', () => {
  const entries = logEntries(['2026-11-21 09:00:00  [INFO] log.started'], [
    '2026-11-21 09:00:01  \x1b[31mERROR[0001] outbound/hysteria2: timeout\x1b[0m',
    '2026-11-21 09:00:02  [TUN] +0000 2026-11-21 01:00:02 WARN[0002] dns: timeout',
    '2026-11-21 09:00:03  [DEBUG] request contains ERROR text',
    '2026-11-21 09:00:04  request contains ERROR text',
    '2026-11-21 09:00:05  [PANIC] unexpected exit',
  ], line => line.replace('log.started', 'Client started'));
  assert.deepEqual(entries.map(x => x.level), ['INFO','ERROR','WARN','DEBUG','INFO','ERROR']);
  assert.deepEqual(entries.slice(0,3).map(x => x.source), ['client','core','tun']);
  assert.equal(entries[0].message,'Client started');
  assert.ok(!entries[1].text.includes('\x1b'));
  assert.equal(entries[2].time,'2026-11-21 09:00:02');
});

test('redacts URL credentials, subscription tokens, JSON secrets and authorization before sharing', () => {
  const samples = [
    'url=https://alice:sensitive@example.net/subscribe/path-secret?token=query-secret&mode=clash',
    '{"password":"json-secret with spaces","accessToken":"bearer-secret","uuid":"uuid-secret"}',
    'Authorization: Bearer header-secret',
    'Authorization: Basic base64-secret',
    'secret=plain-secret token=plain-token',
    'vless://proxy-secret@example.net:443?security=tls',
  ];
  for (const raw of samples) {
    const safe = redactLog(raw);
    assert.ok(safe.includes('[REDACTED]'));
    assert.ok(!/sensitive|path-secret|query-secret|json-secret|bearer-secret|uuid-secret|header-secret|base64-secret|plain-secret|plain-token|proxy-secret/.test(safe),safe);
  }
  assert.equal(redactLog('https://example.net:443/check?mode=clash timeout=5s'), 'https://example.net:443/check?mode=clash timeout=5s');
});

test('combined filters match translated text and retain duplicate events in chronological order', () => {
  const rows = logEntries(['2026-11-21 11:00:02 [INFO] restart'], [
    '2026-11-21 11:00:01 ERROR[01] DNS timeout',
    '2026-11-21 11:00:01 ERROR[01] DNS timeout',
    '2026-11-21 11:00:03 [TUN] WARN[03] dns timeout',
  ], x=>x.replace('restart','Client RESTART'));
  assert.equal(rows.length,4);
  assert.notEqual(rows[0].id,rows[1].id);
  assert.equal(filterLogs(rows,' dns ','ERROR','core').length,2);
  assert.equal(filterLogs(rows,'restart','all','client').length,1);
  assert.equal(filterLogs(rows,'missing','all','all').length,0);
  assert.equal(filterLogs(rows,'','all','all').length,4);
  assert.equal(filterLogs(rows,'dns','WARN','tun').length,1);
});

test('caps the recent view and virtualizes thousands of records with stable spacer heights', () => {
  const entries=logEntries([],Array.from({length:7000},(_,i)=>`2026-11-21 09:00:00 INFO[01] entry ${i}`),x=>x);
  assert.equal(entries.length,LOG_WINDOW_LIMIT);
  assert.ok(entries[0].text.endsWith('entry 2000'));
  for(const position of [0,28,50_000,140_000]) {
    const range=logRange(5000,position,420);
    assert.ok(range.start>=0 && range.end<=5000 && range.end>=range.start);
    assert.ok(range.end-range.start<=39);
    assert.equal(range.before+(range.end-range.start)*28+range.after,5000*28);
  }
  assert.deepEqual(logRange(0,0,400),{start:0,end:0,before:0,after:0});
});

test('log search supports case, whole words, regex and recoverable invalid expressions', () => {
  const rows = logEntries([], [
    '2026-11-21 09:00:00.123 ERROR[1] DNS timeout api1.example.net',
    '2026-11-21 09:00:01 INFO[2] dns cache dnscache api2.example.net',
    '2026-11-21 09:00:02 INFO[3] dnscache only',
  ], x => x);
  const base = { caseSensitive: false, wholeWord: false, regex: false };
  assert.equal(rows[0].time, '2026-11-21 09:00:00.123');
  assert.equal(filterLogs(rows, 'DNS', 'all', 'all', base).length, 3);
  assert.equal(filterLogs(rows, 'DNS', 'all', 'all', { ...base, caseSensitive: true }).length, 1);
  assert.equal(filterLogs(rows, 'dns', 'all', 'all', { ...base, wholeWord: true }).length, 2);
  assert.equal(filterLogs(rows, 'api[12]\\.example', 'all', 'all', { ...base, regex: true }).length, 2);
  assert.equal(filterLogs(rows, '[', 'all', 'all', { ...base, regex: true }).length, 0);
  assert.equal(filterLogs(rows, '[', 'all', 'all', base).length, 3);
  assert.equal(filterLogs(rows, '', 'ERROR', 'core', { ...base, regex: true }).length, 1);
});
