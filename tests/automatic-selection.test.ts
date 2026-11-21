import assert from 'node:assert/strict';
import test from 'node:test';
import { AUTOMATIC_NODE_ID, AUTOMATIC_OUTBOUND_TAG, activeChain, configureSelection, outboundTag } from '../src/lib/automatic-selection.ts';
const profile=()=>({outbounds:[{type:'shadowsocks',tag:'a'},{type:'vmess',tag:'b'},{type:'selector',tag:'select',outbounds:['a','b'],default:'a'}]});
test('automatic group only probes online eligible nodes and preserves existing sessions',()=>{
 const config:any=profile();configureSelection(config,AUTOMATIC_NODE_ID,[{id:'a',agentOnline:true},{id:'b',agentOnline:false}],true);
 assert.equal(config.outbounds[2].default,AUTOMATIC_OUTBOUND_TAG);
 const group=config.outbounds[3];assert.equal(group.type,'urltest');assert.deepEqual(group.outbounds,['a']);assert.equal(group.interval,'3m');assert.equal(group.tolerance,50);assert.equal(group.interrupt_exist_connections,false);
});
test('manual selection remains manual; bad selections and tag collisions fail',()=>{
 const config:any=profile();configureSelection(config,'b',[],true);assert.equal(config.outbounds[2].default,'b');assert.equal(config.outbounds.length,3);
 assert.throws(()=>configureSelection(profile(),'missing',[],true));
 assert.throws(()=>configureSelection(profile(),AUTOMATIC_NODE_ID,[{id:'a',agentOnline:false}],true));
 config.outbounds.push({type:'direct',tag:AUTOMATIC_OUTBOUND_TAG});assert.throws(()=>configureSelection(config,AUTOMATIC_NODE_ID,[{id:'a',agentOnline:true}],true));
 assert.equal(outboundTag('abcd-1234-1111',false),'node-abcd1234');
});
test('choosing another node drops a chain that exits elsewhere',()=>{
 assert.deepEqual(activeChain(['sg','us','br'],'sg06'),[]);
 assert.deepEqual(activeChain(['sg','us','br'],'br'),['sg','us','br']);
 assert.deepEqual(activeChain(['sg','us','br'],''),['sg','us','br']);
 assert.deepEqual(activeChain(['only'],'only'),[]);
});
