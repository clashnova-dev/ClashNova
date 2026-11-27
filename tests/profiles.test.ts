import assert from "node:assert/strict";
import test from "node:test";
import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { parseSingBoxProfile, normalizeLegacyDns } from "../src/lib/profiles.ts";

test("通用 sing-box 配置建立可切换节点与本地直连", () => {
  const raw = JSON.stringify({ outbounds: [
    { type: "socks", tag: "Tokyo", server: "127.0.0.1", server_port: 1081 },
    { type: "socks", tag: "Hong Kong", server: "127.0.0.1", server_port: 1082 },
  ] });
  const parsed = parseSingBoxProfile(raw);
  assert.deepEqual(parsed.nodes.map(node => node.name), ["Tokyo", "Hong Kong"]);
  const outbounds = parsed.config.outbounds as { tag: string; outbounds?: string[] }[];
  assert.deepEqual(outbounds.find(item => item.tag === "select")?.outbounds, ["Tokyo", "Hong Kong"]);
  assert.ok(outbounds.some(item => item.tag === "direct"));
});

test("拒绝重复标签和不支持的格式", () => {
  assert.throws(() => parseSingBoxProfile("proxies: []"), /error\.subscriptionNotSingBox/);
  assert.throws(() => parseSingBoxProfile(JSON.stringify({ outbounds: [
    { type: "socks", tag: "same", server: "127.0.0.1", server_port: 1081 },
    { type: "socks", tag: "same", server: "127.0.0.1", server_port: 1082 },
  ] })), /error\.subscriptionBadTags/);
});

test("剥除导入订阅里依赖 rule_set/geoip/geosite 的规则,保留其余", () => {
  const raw = JSON.stringify({
    outbounds: [{ type: "socks", tag: "N1", server: "127.0.0.1", server_port: 1081 }],
    route: {
      rule_set: [{ tag: "geoip-cn", type: "remote", url: "https://x/geoip-cn.srs" }],
      rules: [
        { rule_set: "geoip-cn", outbound: "direct" },
        { geoip: ["cn"], outbound: "direct" },
        { geosite: ["cn"], outbound: "direct" },
        { type: "logical", mode: "and", rules: [{ geoip: ["cn"] }, { port: 443 }], outbound: "direct" },
        { domain_suffix: ["example.com"], outbound: "direct" },
      ],
    },
    dns: { rule_set: [{ tag: "x" }], rules: [{ geosite: ["cn"], server: "local" }, { domain_suffix: ["a.com"], server: "local" }] },
  });
  const { config } = parseSingBoxProfile(raw);
  const route = config.route as { rule_set?: unknown; rules: unknown[] };
  const dns = config.dns as { rule_set?: unknown; rules: unknown[] };
  assert.equal(route.rule_set, undefined);
  assert.deepEqual(route.rules, [{ domain_suffix: ["example.com"], outbound: "direct" }]);
  assert.equal(dns.rule_set, undefined);
  assert.deepEqual(dns.rules, [{ domain_suffix: ["a.com"], server: "local" }]);
});

test("本地 Clash YAML 转换后可建立 sing-box 节点选择器", (t) => {
  const triple = process.platform === "win32" ? "x86_64-pc-windows-msvc.exe"
    : process.platform === "darwin" ? `${process.arch === "arm64" ? "aarch64" : "x86_64"}-apple-darwin` : process.platform === "linux" ? `${process.arch === "arm64" ? "aarch64" : "x86_64"}-unknown-linux-gnu` : "";
  const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
  const converter = resolve(root, "src-tauri", "binaries", `sing-box-sub-${triple}`);
  if (!triple || !existsSync(converter)) return t.skip("先运行订阅转换器准备脚本");
  const source = resolve(root, "testdata", "example-clash.yaml");
  const outbounds = JSON.parse(execFileSync(converter, [source, "--only-nodes", "--out", "-"], { encoding: "utf8" }));
  const parsed = parseSingBoxProfile(JSON.stringify({ outbounds }));
  assert.deepEqual(parsed.nodes.map(node => node.name), ["Local SOCKS", "Local Trojan"]);
  assert.equal(parsed.nodes[1].protocol, "trojan");
});


test("拒绝把代理节点伪装成直连保留标签", () => {
  assert.throws(() => parseSingBoxProfile(JSON.stringify({ outbounds: [
    { type: "socks", tag: "direct", server: "127.0.0.1", server_port: 1081 },
  ] })), /error\.subscriptionTagDirect/);
});

test("把旧版 dns.servers 转成 sing-box 1.14 新格式(覆盖各协议与端口)", () => {
  const dns: any = {
    servers: [
      { tag: "ali", address: "223.5.5.5" },
      { tag: "ali-5353", address: "223.5.5.5:5353" },
      { tag: "google-dot", address: "tls://dns.google" },
      { tag: "cf-doh", address: "https://1.1.1.1/dns-query", detour: "proxy" },
      { tag: "doh-path", address: "https://doh.example/custom", detour: "proxy" },
      { tag: "doq", address: "quic://dns.adguard.com" },
      { tag: "sys", address: "local" },
      { tag: "dhcp-if", address: "dhcp://en0" },
      { tag: "named", address: "https://dns.google/dns-query", address_resolver: "ali" },
    ],
    rules: [{ domain_suffix: ["a.com"], server: "ali" }],
    final: "cf-doh",
  };
  normalizeLegacyDns(dns);
  assert.deepEqual(dns.servers, [
    { type: "udp", tag: "ali", server: "223.5.5.5" },
    { type: "udp", tag: "ali-5353", server: "223.5.5.5", server_port: 5353 },
    // A host-name server keeps no domain_resolver unless the legacy entry named one.
    { type: "tls", tag: "google-dot", server: "dns.google" },
    { type: "https", tag: "cf-doh", server: "1.1.1.1", detour: "proxy" },
    { type: "https", tag: "doh-path", server: "doh.example", path: "/custom", detour: "proxy" },
    { type: "quic", tag: "doq", server: "dns.adguard.com" },
    { type: "local", tag: "sys" },
    { type: "dhcp", tag: "dhcp-if", interface: "en0" },
    { type: "https", tag: "named", server: "dns.google", domain_resolver: "ali" },
  ]);
  // Rules and final keep their server-tag references.
  assert.equal(dns.final, "cf-doh");
  assert.deepEqual(dns.rules, [{ domain_suffix: ["a.com"], server: "ali" }]);
});

test("折叠被移除的顶层 dns.fakeip 块进 fakeip server,丢弃无对应的旧地址", () => {
  const dns: any = {
    servers: [
      { tag: "dns-local", address: "223.5.5.5" },
      { tag: "fakeip", address: "fakeip" },
      { tag: "rc", address: "rcode://refused" },
    ],
    rules: [{ query_type: ["A", "AAAA"], server: "fakeip" }],
    fakeip: { enabled: true, inet4_range: "198.18.0.0/15", inet6_range: "2001:2::/48" },
  };
  normalizeLegacyDns(dns);
  assert.equal(dns.fakeip, undefined); // top-level block removed.
  assert.deepEqual(dns.servers, [
    { type: "udp", tag: "dns-local", server: "223.5.5.5" },
    { type: "fakeip", tag: "fakeip", inet4_range: "198.18.0.0/15", inet6_range: "2001:2::/48" },
    // rcode:// has no 1.14 server equivalent and is dropped.
  ]);
});

test("已是新格式的 dns 配置保持原样", () => {
  const dns = { servers: [{ type: "https", tag: "cf", server: "1.1.1.1" }], final: "cf" };
  const snapshot = JSON.parse(JSON.stringify(dns));
  normalizeLegacyDns(dns);
  assert.deepEqual(dns, snapshot);
});

test("导入订阅解析即把旧 dns 迁移到新格式", () => {
  const raw = JSON.stringify({
    outbounds: [{ type: "socks", tag: "N1", server: "127.0.0.1", server_port: 1081 }],
    dns: { servers: [{ tag: "r", address: "tls://1.1.1.1", detour: "select" }], final: "r" },
  });
  const { config } = parseSingBoxProfile(raw);
  const dns = config.dns as { servers: unknown[] };
  assert.deepEqual(dns.servers, [{ type: "tls", tag: "r", server: "1.1.1.1", detour: "select" }]);
});

test("subscription size uses UTF-8 bytes rather than JavaScript character count", () => {
  const config = {outbounds:[{type:"socks",tag:"node",server:"127.0.0.1",server_port:1081}],description:"x".repeat(710000)};
  assert.equal(parseSingBoxProfile(JSON.stringify(config)).nodes.length,1);
  config.description = "节".repeat(710000);
  const raw = JSON.stringify(config);
  assert.ok(raw.length < 2 * 1024 * 1024);
  assert.throws(() => parseSingBoxProfile(raw),/error\.subscriptionTooLarge/);
});
