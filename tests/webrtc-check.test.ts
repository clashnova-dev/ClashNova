import assert from 'node:assert/strict';
import test from 'node:test';
import { collectWebrtc } from '../src/lib/webrtc-check.ts';

function peer() {
  let closes = 0, descriptions = 0;
  const pc = {
    onicecandidate: null as ((event: any) => void) | null,
    createDataChannel() {},
    async createOffer() { return {}; },
    async setLocalDescription() { descriptions++; },
    close() { closes++; },
  };
  return { pc, factory: () => pc as unknown as RTCPeerConnection,
    get closes() { return closes; }, get descriptions() { return descriptions; },
    candidate(ip: string) { pc.onicecandidate?.({candidate:{candidate:`candidate:1 1 udp 1 ${ip} 1234 typ srflx`}}); },
    end() { pc.onicecandidate?.({candidate:null}); },
  };
}
test('ICE timeout is unknown and closes the peer connection', async () => {
 const fake = peer(), check = collectWebrtc(fake.factory, 5);
 fake.candidate('192.168.1.10');
 assert.deepEqual(await check.result,{kind:'timeout'});
 assert.equal(fake.closes,1); assert.equal(fake.pc.onicecandidate,null);
 check.cancel(); assert.equal(fake.closes,1);
});
test('gathered candidates survive timeout and exclude private addresses', async () => {
 const fake = peer(), check = collectWebrtc(fake.factory,5);
 fake.candidate('192.168.1.2'); fake.candidate('8.8.8.8'); fake.candidate('host.local');
 assert.deepEqual(await check.result,{kind:'public',ips:['8.8.8.8']});
 assert.equal(fake.closes,1);
});
test('cancel settles immediately and prevents late offer work', async () => {
 const fake = peer(), check = collectWebrtc(fake.factory,1000);
 check.cancel(); fake.candidate('9.9.9.9'); fake.end();
 assert.deepEqual(await check.result,{kind:'none'});
 await Promise.resolve();
 assert.equal(fake.closes,1); assert.equal(fake.descriptions,0);
});
test('unavailable RTC and failed offers remain unconfirmed', async () => {
 assert.deepEqual(await collectWebrtc(() => { throw new Error('unsupported'); }).result,{kind:'off'});
 const fake = peer();
 fake.pc.createOffer = async () => { throw new Error('denied'); };
 assert.deepEqual(await collectWebrtc(fake.factory).result,{kind:'off'});
 assert.equal(fake.closes,1);
});
test('completed LAN-only gathering differs from a timeout', async () => {
 const fake = peer(), check = collectWebrtc(fake.factory,1000);
 fake.candidate('192.168.1.2'); fake.end();
 assert.deepEqual(await check.result,{kind:'lan'});
 assert.equal(fake.closes,1);
});
