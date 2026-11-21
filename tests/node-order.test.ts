import assert from "node:assert/strict";
import test from "node:test";
import { sortByLatency } from "../src/lib/node-order.ts";
test("按延迟排序，零毫秒有效，失败与待测排后且不修改订阅顺序", () => {
  const nodes = ["failed", "slow", "zero", "pending", "fast"].map(id => ({ id }));
  assert.deepEqual(sortByLatency(nodes, { slow: 500, zero: 0, fast: 50 }).map(n => n.id), ["zero", "fast", "slow", "failed", "pending"]);
  assert.equal(nodes[0].id, "failed");
});
