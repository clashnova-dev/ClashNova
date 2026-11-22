import assert from "node:assert/strict";
import test from "node:test";
import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { parseSingBoxProfile } from "../src/lib/profiles.ts";

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

test("subscription size uses UTF-8 bytes rather than JavaScript character count", () => {
  const config = {outbounds:[{type:"socks",tag:"node",server:"127.0.0.1",server_port:1081}],description:"x".repeat(710000)};
  assert.equal(parseSingBoxProfile(JSON.stringify(config)).nodes.length,1);
  config.description = "节".repeat(710000);
  const raw = JSON.stringify(config);
  assert.ok(raw.length < 2 * 1024 * 1024);
  assert.throws(() => parseSingBoxProfile(raw),/error\.subscriptionTooLarge/);
});
