import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { encode, messageKeys, msg, translator } from "../src/i18n/index.ts";
import zhCN from "../src/i18n/zh-CN.ts";
import en from "../src/i18n/en.ts";
import { parsePreferences } from "../src/lib/preferences.ts";

const src = fileURLToPath(new URL("../src/", import.meta.url));
const walk = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name).replaceAll("\\", "/")]);
const sources = walk(src).filter(file => /\.(vue|ts)$/.test(file));
// Not app copy: the dictionaries, the long-form help articles, unit specs, and the patterns that recognise region names inside node names.
const appCode = sources.filter(file => !file.includes("/i18n/") && !file.endsWith("content/help.ts") && !file.endsWith("pages/help/help-content.ts") && !file.endsWith(".spec.ts") && !file.endsWith("lib/regions.ts"));
// The native side sends keys too: the app and the TUN service helper.
const root = fileURLToPath(new URL("../", import.meta.url));
const rustCode = ["src-tauri/src", "tun-service/src"].flatMap(dir => walk(join(root, dir))).filter(file => file.endsWith(".rs"));
// Template expressions and file names that happen to look like a key.
const notKeys = new Set(["nodes.length", "shortcuts.json", "subscriptions.json", "check.json"]);
const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map(match => match[1]).sort().join(",");

test("empty text renders as nothing and a key renders in both languages", () => {
  for (const locale of ["zh-CN", "en"] as const) {
    const { t, tx } = translator(() => locale);
    const errors: Record<string, string | undefined> = {};
    assert.equal(tx(errors["not-tested"]), "");
    assert.equal(tx(null), "");
    assert.equal(tx(""), "");
    assert.equal(tx(errors["not-tested"]) || t("nodes.notTested"), locale === "en" ? "Not tested" : "未检测");
  }
});

test("both dictionaries define the same keys, with the same placeholders and no blanks", () => {
  assert.deepEqual(Object.keys(en).sort(), Object.keys(zhCN).sort());
  for (const key of messageKeys) {
    assert.ok(zhCN[key].trim() && en[key].trim(), `blank copy: ${key}`);
    assert.equal(placeholders(en[key]), placeholders(zhCN[key]), `placeholders differ: ${key}`);
    assert.match(key, /^[a-z][A-Za-z]*\.[a-z][A-Za-z0-9]*$/, `key is not namespace.name: ${key}`);
  }
});

test("values, prepared messages, native keys and verbatim text all render", () => {
  const english = translator(() => "en"), chinese = translator(() => "zh-CN");
  assert.equal(english.t("nodes.testProgress", { done: 2, total: 56, count: 3 }), "Tested 2 / 56 · 3 available");
  assert.equal(chinese.t("nodes.testProgress", { done: 2, total: 56, count: 3 }), "已检测 2 / 56 · 3 个可用");
  // A stored message follows the language in use when it is shown.
  const stored = msg("subscriptions.nowUsing", { name: "My VPN" });
  assert.equal(english.tx(stored), "Using My VPN");
  assert.equal(chinese.tx(stored), "已启用 My VPN");
  // A value may itself be a message.
  assert.equal(english.tx(msg("log.connectionState", { state: msg("status.connected") })), "Connection state: Connected");
  // A key reported by the native side renders like any other; anything else is shown as is.
  assert.equal(english.tx("native.probeCoreStartTimeout"), en["native.probeCoreStartTimeout"]);
  assert.equal(chinese.tx("native.probeCoreStartTimeout"), zhCN["native.probeCoreStartTimeout"]);
  assert.equal(english.tx("PRO 日本01"), "PRO 日本01");
  assert.equal(english.tx("constructor"), "constructor");
});

test("messages survive the trip as plain strings: native errors, status text and log lines", () => {
  const english = translator(() => "en"), chinese = translator(() => "zh-CN");
  // A bare key and an encoded message both render; so does one nested as a value of another.
  assert.equal(encode("status.connected"), "status.connected");
  const wire = encode(msg("subscriptions.nowUsing", { name: "My VPN" }));
  assert.equal(wire, '{"key":"subscriptions.nowUsing","values":{"name":"My VPN"}}');
  assert.equal(english.tx(wire), "Using My VPN");
  assert.equal(chinese.tx(wire), "已启用 My VPN");
  assert.equal(english.tx(msg("nodes.testFailed", { error: "error.network" })), "Test failed: " + english.t("error.network"));
  assert.equal(english.tx(msg("nodes.testFailed", { error: wire })), "Test failed: Using My VPN");
  // What the native side builds (see `msg` in tun-service/src/message.rs): number values, and an inner error as a value.
  assert.equal(english.tx('{"key":"native.proxyStarted","values":{"address":"127.0.0.1","port":2080}}'), "Proxy started on mixed port 127.0.0.1:2080");
  const inner = '{"key":"native.probeHttpStatus","values":{"status":"502 Bad Gateway"}}';
  assert.equal(chinese.tx(JSON.stringify({ key: "native.egressCheckFailed", values: { error: inner } })),
    zhCN["native.egressCheckFailed"].replace("{error}", zhCN["native.probeHttpStatus"].replace("{status}", "502 Bad Gateway")));
  // Text that only looks like a message is left alone.
  assert.equal(english.tx('{"key":"no.such.key"}'), '{"key":"no.such.key"}');
  assert.equal(english.tx('{"key":'), '{"key":');
});

test("every key used in the source exists, and every key in the dictionary is used", () => {
  const namespaces = [...new Set(messageKeys.map(key => key.split(".")[0]))].join("|");
  const literal = new RegExp(`["'\`]((?:${namespaces})\\.[A-Za-z0-9]+)["'\`]`, "g");
  const used = new Set<string>();
  for (const file of [...appCode, ...rustCode]) for (const match of readFileSync(file, "utf8").matchAll(literal)) if (!notKeys.has(match[1])) used.add(match[1]);
  const known = new Set<string>(messageKeys);
  assert.deepEqual([...used].filter(key => !known.has(key)), [], "keys used but not defined");
  assert.deepEqual(messageKeys.filter(key => !used.has(key)), [], "keys defined but never used");
});

test("app code holds no Chinese copy outside the dictionaries", () => {
  const offenders: string[] = [];
  for (const file of appCode) {
    readFileSync(file, "utf8").split("\n").forEach((line, index) => {
      for (const match of line.matchAll(/(["'`])((?:(?!\1).)*[一-龥](?:(?!\1).)*)\1/g)) offenders.push(`${file.slice(src.length)}:${index + 1} ${match[2]}`);
    });
  }
  assert.deepEqual(offenders, []);
});

test("the native side holds no Chinese copy: it sends keys", () => {
  const offenders: string[] = [];
  for (const file of rustCode) {
    readFileSync(file, "utf8").split("\n").forEach((line, index) => {
      for (const match of line.matchAll(/"(?:[^"\\]|\\.)*"/g)) if (/[一-龥]/.test(match[0])) offenders.push(`${file.slice(root.length)}:${index + 1} ${match[0]}`);
    });
  }
  assert.deepEqual(offenders, []);
});

test("language and theme are restored; old caches and invalid values fall back to defaults", () => {
  const saved = parsePreferences('{"version":1,"locale":"en","theme":"dark"}');
  assert.equal(saved?.locale, "en"); assert.equal(saved?.theme, "dark");
  const legacy = parsePreferences('{"version":1,"locale":"invalid","theme":"invalid"}');
  assert.equal(legacy?.locale, "en"); assert.equal(legacy?.theme, "system");
});
