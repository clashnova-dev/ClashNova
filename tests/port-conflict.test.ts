import assert from 'node:assert/strict';
import test from 'node:test';
import {resolveProxyPort, type PortConflict} from '../src/lib/port-conflict.ts';

const conflict: PortConflict = {port:2080,token:'one-shot',listeners:[{pid:4242,name:'example-server'}]};
test('a free proxy port proceeds without prompting or terminating a process',async()=>{
 assert.equal(await resolveProxyPort({inspect:async()=>null,confirm:async()=>{throw Error('unexpected prompt');},release:async()=>{throw Error('unexpected kill');}}),true);
});
test('declining port termination cancels connection without releasing anything',async()=>{
 let released=false;
 assert.equal(await resolveProxyPort({inspect:async()=>conflict,confirm:async shown=>{assert.deepEqual(shown,conflict);return false;},release:async()=>{released=true;}}),false);
 assert.equal(released,false);
});
test('connection only proceeds after approval and completion of the exact release token',async()=>{
 const steps:string[]=[];
 assert.equal(await resolveProxyPort({inspect:async()=>conflict,confirm:async()=>{steps.push('approved');return true;},release:async token=>{assert.equal(token,conflict.token);await Promise.resolve();steps.push('released');}}),true);
 assert.deepEqual(steps,['approved','released']);
});
test('unknown owners, permission failures and changed owners do not proceed to connect',async()=>{
 await assert.rejects(resolveProxyPort({inspect:async()=>{throw Error('unknown owner');},confirm:async()=>true,release:async()=>{}}),/unknown owner/);
 for(const error of ['permission denied','owner changed'])await assert.rejects(resolveProxyPort({inspect:async()=>conflict,confirm:async()=>true,release:async()=>{throw Error(error);}}),new RegExp(error));
});
