import assert from "node:assert/strict";
import test from "node:test";
import { validateProxyPort, proxyAddress } from "../src/lib/network-settings.ts";

test("代理端口校验覆盖边界、小数、空白和非十进制输入", () => {
  for (const value of ["", " ", "0", "1023", "65536", "1.5", "2080.0", "2e3", "0x800", "abc", "-2080", NaN]) {
    assert.throws(() => validateProxyPort(value), /settings\.portInvalid/);
  }
  for (const value of [1024, 2080, 65535]) assert.equal(validateProxyPort(` ${value} `), value);
  assert.equal(proxyAddress(3456), "127.0.0.1:3456");
});

import { defaultTunSettings, isCidr, parseRouteExclude, validateTunMtu } from "../src/lib/network-settings.ts";

test("TUN device settings: defaults, MTU bounds and bypass prefixes", () => {
  assert.deepEqual(defaultTunSettings(), { stack: "mixed", mtu: 1500, strictRoute: true, ipv6: false, autoRedirect: false, routeExclude: [], killSwitch: false, dnsMode: "fakeip", dnsRemote: "cloudflare", dnsDomestic: "local" });
  for (const value of ["", "1279", "9001", "1500.0", "1e3", "abc", "-1500"]) assert.throws(() => validateTunMtu(value), /settings\.tunMtuInvalid/);
  for (const value of [1280, 1500, 9000]) assert.equal(validateTunMtu(` ${value} `), value);
  for (const ok of ["0.0.0.0/0", "10.0.0.0/8", "192.168.1.1/32", "::/0", "fc00::/7", "fe80::1/128", "2001:db8:0:0:0:0:0:1/64", "::ffff:192.168.0.1/128"]) assert.ok(isCidr(ok), ok);
  for (const bad of ["", "10.0.0.0", "10.0.0.0/", "10.0.0.0/33", "256.0.0.0/8", "::/129", "a/8", "10.0.0.0/8/8", "fc00::/", "1::2::3/64", "12345::/16", "10.0.0/8"]) assert.ok(!isCidr(bad), bad);
  // One per line; commas and spaces also separate; duplicates are dropped.
  assert.deepEqual(parseRouteExclude(" 192.168.0.0/16\n10.0.0.0/8, fc00::/7\n\n192.168.0.0/16 "), ["192.168.0.0/16", "10.0.0.0/8", "fc00::/7"]);
  assert.deepEqual(parseRouteExclude("  \n "), []);
  assert.throws(() => parseRouteExclude("192.168.0.1"), /settings\.tunExcludeInvalid/);
  assert.throws(() => parseRouteExclude(Array.from({ length: 65 }, (_, i) => `10.${i}.0.0/16`).join("\n")), /settings\.tunExcludeInvalid/);
});
