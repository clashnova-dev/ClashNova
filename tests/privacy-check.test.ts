import assert from 'node:assert/strict';
import test from 'node:test';
import { isPublicIp, normalizeIp, webrtcAssessment, ipv6Assessment, dnsLeakAssessment } from '../src/lib/privacy-check.ts';
const native = { connected:true, proxy:'ok', mode:'system',exitIp:'8.8.8.8',directIp:'9.9.9.9',ipv6Ip:'' };
test('WebRTC public proxy candidates are not automatically reported as leaks', () => {
 assert.equal(webrtcAssessment({kind:'public',ips:['8.8.8.8']},native).tone,'ok');
 assert.equal(webrtcAssessment({kind:'public',ips:['8.8.8.8','9.9.9.9']},native).tone,'bad');
 assert.equal(webrtcAssessment({kind:'public',ips:['1.1.1.1']},native).tone,'warn');
 assert.equal(webrtcAssessment({kind:'public',ips:['9.9.9.9']},{...native,mode:'tun'}).tone,'warn');
 assert.equal(webrtcAssessment({kind:'public',ips:['8.8.8.8']},{...native,proxy:'fail'}).tone,'warn');
 assert.equal(webrtcAssessment({kind:'none'},native).tone,'idle');
 assert.equal(webrtcAssessment({kind:'off'},native).tone,'idle');
});
test('IP parsing excludes local, shared, fake, invalid and multicast addresses', () => {
 for (const ip of ['10.1.1.1','100.64.1.1','198.18.0.2','127.0.0.1','169.254.1.1','999.2.2.2','224.0.0.1','::1','fe80::1234','fd00::1','host.local','bad:ip']) assert.equal(isPublicIp(ip),false,ip);
 assert.equal(normalizeIp('2606:4700:4700:0000:0000:0000:0000:1111'),normalizeIp('2606:4700:4700::1111'));
 assert.equal(webrtcAssessment({kind:'public',ips:['2606:4700:4700:0:0:0:0:1111']},{...native,exitIp:'2606:4700:4700::1111'}).tone,'ok');
});

test('missing IPv6 evidence is never a green protection claim', () => {
 for (const mode of ['system','tun','manual']) {
  assert.equal(ipv6Assessment({...native, mode}).tone,'idle');
  assert.equal(ipv6Assessment({...native, mode}).text,'check.ipv6NoExit');
  assert.equal(ipv6Assessment({...native, mode, ipv6Ip:'2606:4700::1111'}).tone,'warn');
  assert.equal(ipv6Assessment({...native, mode, ipv6Ip:'2606:4700::1111',proxyIpv6Ip:'2606:4700:0:0:0:0:0:1111'}).tone,'ok');
  assert.equal(ipv6Assessment({...native, mode, proxy:'fail',ipv6Ip:'2606:4700::1111',proxyIpv6Ip:'2606:4700::1111'}).tone,'warn');
 }
});
test('IPv6 off with no route is closed, and a proxy-only address is not a bypass', () => {
 assert.equal(ipv6Assessment({...native, mode:'tun', tunIpv6:false, ipv6Reachable:false}).text,'check.ipv6Closed');
 assert.equal(ipv6Assessment({...native, mode:'tun', tunIpv6:true, ipv6Reachable:false}).text,'check.ipv6NoExit');
 assert.equal(ipv6Assessment({...native, mode:'tun', proxyIpv6Ip:'2606:4700::1111'}).text,'check.ipv6ViaProxy');
 assert.equal(ipv6Assessment({...native, connected:false, ipv6Reachable:false}).text,'check.ipv6LocalNone');
});
test('DNS capture is judged from the answer, and the resolver address from the identity query', () => {
 const tun = {...native, mode:'tun', dnsMode:'fakeip', exitIp:'8.8.8.8'};
 assert.equal(dnsLeakAssessment({...tun, dnsAnswers:['198.18.4.5']}).text,'check.dnsTunCaptured');
 assert.equal(dnsLeakAssessment({...tun, dnsAnswers:['1.1.1.1']}).tone,'bad');
 assert.equal(dnsLeakAssessment({...tun, dnsMode:'remote', dnsResolver:'8.8.8.8'}).text,'check.dnsExitMatch');
 assert.equal(dnsLeakAssessment({...tun, dnsMode:'remote', dnsResolver:'203.0.113.9'}).text.key,'check.dnsExitDiff');
 assert.equal(dnsLeakAssessment({...native, dnsResolver:'203.0.113.8'}).text.key,'check.dnsSystemSeen');
 assert.equal(dnsLeakAssessment({...native, dnsResolver:'9.9.9.9', directIp:'9.9.9.9'}).tone,'bad');
 assert.equal(dnsLeakAssessment({...native, connected:false}).text,'check.dnsLeakNeedConnection');
});
test('dual-stack WebRTC compares both verified proxy exits', () => {
 const dual = {...native, ipv6Ip:'2606:4700::1111',proxyIpv6Ip:'2606:4700::1111'};
 assert.equal(webrtcAssessment({kind:'public',ips:['8.8.8.8','2606:4700::1111']},dual).tone,'ok');
 assert.equal(webrtcAssessment({kind:'timeout'},native).tone,'idle');
 assert.equal(webrtcAssessment({kind:'timeout'},native).text,'check.webrtcTimeout');
 assert.equal(webrtcAssessment({kind:'public',ips:['::ffff:909:909']},native).tone,'bad');
});
test('IPv4-mapped IPv6 cannot hide public or private addresses', () => {
 assert.equal(normalizeIp('::ffff:8.8.8.8'),'8.8.8.8');
 assert.equal(isPublicIp('::ffff:8.8.8.8'),true);
 assert.equal(isPublicIp('::ffff:192.168.1.2'),false);
 assert.equal(isPublicIp('2001:db8::1'),false);
});
