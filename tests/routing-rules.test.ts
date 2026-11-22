import assert from 'node:assert/strict';
import test from 'node:test';
import { CHINA_DOMAINS } from '../src/lib/china-domains.ts';
import { builtinRouteRules } from '../src/lib/routing-rules.ts';

test('compact domestic routing has 500 distinct suffixes without broad TLD/IP bypasses', () => {
  assert.equal(CHINA_DOMAINS.length, 500);
  assert.equal(new Set(CHINA_DOMAINS).size, 500);
  for (const domain of CHINA_DOMAINS) {
    assert.match(domain, /^[a-z0-9-]+(?:\.[a-z0-9-]+)+$/);
    assert.ok(!CHINA_DOMAINS.some(parent => parent !== domain && domain.endsWith('.' + parent)));
  }
  const rules = builtinRouteRules(true);
  assert.equal(rules.length, 2);
  assert.deepEqual(rules[0], { ip_is_private: true, outbound: 'direct' });
  assert.deepEqual(rules[1], { domain_suffix: CHINA_DOMAINS, outbound: 'direct' });
  assert.ok(new TextEncoder().encode(JSON.stringify(rules)).length < 8192);
  for (const domain of ['baidu.com', 'qq.com', 'taobao.com', 'bilibili.com', 'jd.com', 'alicdn.com', 'hdslb.com', 'deepseek.com']) {
    assert.ok(CHINA_DOMAINS.includes(domain), domain);
  }
  function direct(host: string) {
    return CHINA_DOMAINS.some(domain => host === domain || host.endsWith('.' + domain));
  }
  assert.equal(direct('www.baidu.com'), true);
  assert.equal(direct('api.bilibili.com'), true);
  for (const host of ['google.com', 'github.com', 'unlisted.example.cn', 'notbaidu.com', 'baidu.com.example.net']) {
    assert.equal(direct(host), false, host);
  }
  assert.deepEqual(builtinRouteRules(false), []);
});
