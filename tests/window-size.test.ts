import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { MAIN_WINDOW, acceptMainWindow } from '../src/lib/window-size.ts';

test('an old remembered width cannot override the new default', () => {
  assert.deepEqual(acceptMainWindow({ width: 1015, height: 670 }), { width: 960, height: 670, defaultWidth: 960 });
  assert.deepEqual(acceptMainWindow({ width: 980, height: 740, defaultWidth: 980 }), { width: 960, height: 740, defaultWidth: 960 });
});

test('manual sizes remain remembered after the default has been applied', () => {
  assert.deepEqual(acceptMainWindow({ width: 1024, height: 720, defaultWidth: 960 }), { width: 1024, height: 720, defaultWidth: 960 });
  const migrated = acceptMainWindow({ width: 1015, height: 670 });
  assert.deepEqual(acceptMainWindow(migrated), migrated);
});

test('invalid sizes are rejected and small sizes respect minimum dimensions', () => {
  for (const value of [null, {}, { width: NaN, height: 670 }, { width: 960, height: Infinity }]) assert.equal(acceptMainWindow(value), null);
  assert.deepEqual(acceptMainWindow({ width: 600, height: 400, defaultWidth: 960 }), { width: 820, height: 650, defaultWidth: 960 });
});

test('base and macOS Tauri configurations agree with the main window defaults', () => {
  for (const name of ['tauri.conf.json', 'tauri.macos.conf.json']) {
    const config = JSON.parse(readFileSync(new URL(`../src-tauri/${name}`, import.meta.url), 'utf8'));
    const window = config.app.windows.find((item: { label: string }) => item.label === 'main');
    assert.equal(window.width, MAIN_WINDOW.width);
    assert.equal(window.height, MAIN_WINDOW.height);
    assert.equal(window.minWidth, MAIN_WINDOW.minWidth);
    assert.equal(window.minHeight, MAIN_WINDOW.minHeight);
  }
});
