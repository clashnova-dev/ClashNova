import test from 'node:test';
import assert from 'node:assert/strict';
import { appNameFromPath, processExecutablePath, appPolicyRules, connectionsForApp } from '../src/lib/app-policies.ts';
test('uses executable identity without the Clash API user annotation', () => {
 assert.equal(appNameFromPath('/Applications/Google Chrome.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper (clashnova-dev)'), 'Google Chrome');
 assert.equal(appNameFromPath('C:\\Program Files\\Example\\example.exe (1000)'), 'example.exe');
 assert.equal(processExecutablePath('/usr/bin/tool (dev)/worker (1000)'), '/usr/bin/tool (dev)/worker');
 for (const raw of ['', '501', 'unknown']) assert.equal(appNameFromPath(raw), '');
 assert.deepEqual(appPolicyRules({[appNameFromPath('501')]: 'proxy'}), []);
});

test('application connection filter isolates active/history and unknown groups', () => {
 const row = (id: number, path: string, host = '') => ({id, metadata: { processPath: path, host }});
 const active = [row(1, '/Applications/Cursor.app/Contents/MacOS/Cursor'), row(2, '/Applications/Chrome.app/Contents/MacOS/Chrome', 'cursor.example'), row(3, ''), row(4, '/Applications/Cursor.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper (user)')];
 const closed = [row(5, '/Applications/Cursor.app/Contents/MacOS/Cursor'), row(6, '/Applications/Chrome.app/Contents/MacOS/Chrome')];
 assert.deepEqual(connectionsForApp(active, 'Cursor').map(row => row.id), [1,4]);
 assert.deepEqual(connectionsForApp(closed, 'Cursor').map(row => row.id), [5]);
 assert.deepEqual(connectionsForApp(active, '').map(row => row.id), [3]);
 assert.equal(connectionsForApp(active, null).length, 4);
 assert.deepEqual(connectionsForApp(active, 'Missing'), []);
});
