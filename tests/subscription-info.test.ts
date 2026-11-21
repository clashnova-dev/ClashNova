import test from 'node:test';
import assert from 'node:assert/strict';
import { parseSubscriptionUsage, usedTraffic, trafficPercent, subscriptionHost } from '../src/lib/subscription-info.ts';

test('subscription-userinfo exposes provider usage and expiry', () => {
  const info = parseSubscriptionUsage('upload=1048576; download=9437184; total=104857600; expire=1800000000');
  assert.equal(usedTraffic(info), 10485760);
  assert.equal(trafficPercent(info), 10);
  assert.equal(info?.expire, 1800000000);
});
test('missing, malformed and unknown totals do not invent traffic', () => {
  assert.equal(parseSubscriptionUsage(null), undefined);
  assert.equal(usedTraffic(parseSubscriptionUsage('download=9')), undefined);
  assert.equal(trafficPercent(parseSubscriptionUsage('upload=0; download=0; total=0')), undefined);
  assert.deepEqual(parseSubscriptionUsage('upload=-1;download=NaN;total=9007199254740992;expire=99999999999999'), undefined);
  assert.equal(parseSubscriptionUsage('upload=1; upload=2')?.upload, undefined);
});
test('over-quota usage remains accurate while progress is capped', () => {
  const info = parseSubscriptionUsage('upload=200;download=300;total=100');
  assert.equal(usedTraffic(info), 500); assert.equal(trafficPercent(info), 100);
});
test('source labels never include credentials, paths or subscription tokens', () => {
  assert.equal(subscriptionHost('https://user:password@example.com/sub/secret?token=secret'), 'example.com');
  assert.equal(subscriptionHost(''), '');
});
