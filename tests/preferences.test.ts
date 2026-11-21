import assert from "node:assert/strict";
import test from "node:test";
import { parsePreferences } from "../src/lib/preferences.ts";

test("损坏或未知版本的缓存回退到首次启动", () => {
  for (const value of [null, "{", "null", '{"version":2}']) assert.equal(parsePreferences(value), null);
});

test("恢复业务偏好，丢弃凭据及不合法模式", () => {
  const prefs = parsePreferences(JSON.stringify({ version: 1, password: "secret", accessToken: "token",
    connectionMode: "invalid", routeMode: "global", activeProfileId: "profile-1",
    selectedNodes: { "profile-1": "Tokyo", bad: null }, email: "user@example.com" }));
  assert.equal(prefs?.connectionMode, "system");
  assert.equal(prefs?.routeMode, "global");
  assert.deepEqual(prefs?.selectedNodes, { "profile-1": "Tokyo" });
  assert.ok(!JSON.stringify(prefs).includes("secret"));
  assert.ok(!JSON.stringify(prefs).includes("token"));
});


test("托盘网速偏好兼容旧缓存并严格解析布尔值", () => {
  assert.equal(parsePreferences('{"version":1}')?.showTraySpeed, false);
  assert.equal(parsePreferences('{"version":1,"showTraySpeed":true}')?.showTraySpeed, true);
  assert.equal(parsePreferences('{"version":1,"showTraySpeed":"false"}')?.showTraySpeed, false);
});
