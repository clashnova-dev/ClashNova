import assert from "node:assert/strict";
import test from "node:test";
import { normalizeToolPath } from "../scripts/tool-env.mjs";

test("Windows tool setup preserves mixed-case Path before prepending tools", () => {
  const original = { Path: "C:\\Python312;C:\\Windows\\System32", OTHER: "kept" };
  const env = normalizeToolPath(original, "win32");
  env.PATH = "C:\\cargo\\bin;" + env.PATH;
  assert.deepEqual(Object.keys(env).filter(key => key.toLowerCase() === "path"), ["PATH"]);
  assert.equal(env.PATH, "C:\\cargo\\bin;C:\\Python312;C:\\Windows\\System32");
  assert.equal(env.OTHER, "kept");
  assert.equal(original.Path, "C:\\Python312;C:\\Windows\\System32");
});

test("Windows path normalization resolves duplicates and accepts unusual casing", () => {
  assert.deepEqual(normalizeToolPath({ PATH: "preferred", Path: "duplicate" }, "win32"), { PATH: "preferred" });
  assert.deepEqual(normalizeToolPath({ pAtH: "tools" }, "win32"), { PATH: "tools" });
  assert.deepEqual(normalizeToolPath({}, "win32"), { PATH: "" });
});

test("Unix tool setup preserves case-sensitive environment keys", () => {
  const env = { PATH: "/usr/bin", Path: "unrelated" };
  assert.deepEqual(normalizeToolPath(env, "linux"), env);
});
