import assert from 'node:assert/strict';
import test from 'node:test';
import { captureShortcut, formatShortcut } from '../src/lib/shortcut-keys.ts';
const event={code:'KeyA',ctrlKey:false,altKey:false,shiftKey:false,metaKey:false};
test('captures portable physical key codes without registering ordinary typing',()=>{
 assert.equal(captureShortcut(event),null);assert.equal(captureShortcut({...event,shiftKey:true}),null);
 assert.equal(captureShortcut({...event,ctrlKey:true,altKey:true}),'Control+Alt+KeyA');
 assert.equal(captureShortcut({...event,metaKey:true,shiftKey:true}),'Shift+Super+KeyA');
 assert.equal(captureShortcut({...event,ctrlKey:true,code:'ControlLeft'}),null);
 assert.equal(formatShortcut('CommandOrControl+Shift+A',true),'⌘ + Shift + A');
 assert.equal(formatShortcut('Control+Alt+KeyA',false),'Ctrl + Alt + A');
});
