import test from 'node:test';
import assert from 'node:assert/strict';
import { appNameFromPath, processExecutablePath, appPathRegex, appPolicyRules, liveAppPolicyRules, applicationGroups, connectionsForApp } from '../src/lib/app-policies.ts';
test('uses executable identity without the Clash API user annotation', () => {
 assert.equal(appNameFromPath('/Applications/Google Chrome.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper (clashnova-dev)'), 'Google Chrome');
 assert.equal(appNameFromPath('C:\\Program Files\\Example\\example.exe (1000)'), 'example.exe');
 assert.equal(processExecutablePath('/usr/bin/tool (dev)/worker (1000)'), '/usr/bin/tool (dev)/worker');
 for (const raw of ['', '501', 'unknown']) assert.equal(appNameFromPath(raw), '');
 assert.deepEqual(appPolicyRules({[appNameFromPath('501')]: 'proxy'}), []);
});

test('configured blocked apps remain visible with zero live connections, after disconnect and after restart', () => {
 const row = {metadata:{processPath:'/Applications/Cursor.app/Contents/MacOS/Cursor'},download:2048,upload:32,downloadSpeed:256,uploadSpeed:4};
 const policies = {Cursor:'block'} as const;
 const live = applicationGroups([row],[],policies);
 assert.equal(live.length,1);assert.equal(live[0].rows.length,1);
 const blocked = applicationGroups([],[row],policies);
 assert.equal(blocked.length,1);assert.equal(blocked[0].app,'Cursor');assert.equal(blocked[0].rows.length,0);
 assert.equal(blocked[0].path,row.metadata.processPath);assert.equal(blocked[0].downloadSpeed,0);
 assert.equal(blocked[0].uploadSpeed,0);
 const restarted = applicationGroups([],[],policies);
 assert.equal(restarted[0].app,'Cursor');assert.equal(restarted[0].path,'');assert.equal(restarted[0].rows.length,0);
 assert.deepEqual(applicationGroups([],[],{}),[]);
});

test('all configured policy types can be edited offline without resurrecting unrelated connection history', () => {
 const closed = {metadata:{processPath:'/usr/bin/unconfigured'},download:10,upload:0,downloadSpeed:10,uploadSpeed:0};
 const groups = applicationGroups([],[closed],{Browser:'block',Editor:'proxy',Downloader:'direct'});
 assert.deepEqual(groups.map(g=>g.app),['Browser','Downloader','Editor']);
 assert.ok(groups.every(group=>group.rows.length===0 && group.download===0 && group.upload===0));
});

test('live policy slots are present even before the first policy and contain only literal app boundaries', () => {
 const empty = liveAppPolicyRules({}) as any[];
 assert.deepEqual(empty.map(rule=>rule.inbound[0]),['clashnova-app-block','clashnova-app-proxy','clashnova-app-direct']);
 assert.ok(empty.every(rule=>rule.process_path_regex.length===1 && rule.process_path_regex[0]==='^$'));
 const configured = liveAppPolicyRules({'Acme [Pro]':'block'}) as any[];
 const expression = new RegExp(configured[0].process_path_regex[0]);
 assert.ok(expression.test('/Applications/Acme [Pro].app/Contents/MacOS/Acme'));
 assert.ok(!expression.test('/Applications/NotAcme [Pro].app/Contents/MacOS/Acme'));
});

test('application policies match the whole bundle/executable instead of a name substring', () => {
 const cursor = new RegExp(appPathRegex('Cursor'));
 assert.equal(cursor.test('/Applications/Cursor.app/Contents/Frameworks/Cursor Helper.app/Contents/MacOS/Cursor Helper'), true);
 assert.equal(cursor.test('/usr/bin/Cursor'), true);
 for (const path of ['/Applications/NotCursor.app/Contents/MacOS/NotCursor', '/usr/bin/Cursor-server', '/opt/Cursor/project/bin/other']) assert.equal(cursor.test(path), false, path);
 const literal = new RegExp(appPathRegex('Acme [Pro] (Beta)+'));
 assert.equal(literal.test('/Applications/Acme [Pro] (Beta)+.app/Contents/MacOS/Acme'), true);
 const re2 = appPathRegex('Chrome.exe');
 assert.ok(re2.startsWith('(?i)'));
 const windows = new RegExp(re2.slice(4), 'i');
 for (const path of ['C:\\Program Files\\Chrome\\chrome.exe', 'C:/Apps/CHROME.EXE']) assert.equal(windows.test(path), true, path);
 for (const path of ['C:\\Apps\\NotChrome.exe', 'C:\\Apps\\Chrome.exe-helper', 'C:\\Chrome.exe\\other.exe']) assert.equal(windows.test(path), false, path);
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
