import assert from "node:assert/strict";
import test from "node:test";
import { createConfirmation } from "../src/lib/confirmation.ts";
const prompt = { title: "confirmation.removeTitle", body: "confirmation.removeBody" };
test("sensitive action waits for an explicit approval", async () => {
  const gate = createConfirmation(); let changes = 0;
  const action = (async () => { if (await gate.request(prompt)) changes++; })();
  await Promise.resolve(); assert.equal(changes, 0);
  gate.settle(gate.current.value!.id, true); await action;
  assert.equal(changes, 1); assert.equal(gate.current.value, null);
});
test("cancel, Escape, backdrop, navigation and host disposal deny the action", async () => {
  const gate = createConfirmation();
  const result = gate.request(prompt); gate.cancel();
  assert.equal(await result, false); assert.equal(gate.current.value, null);
  gate.cancel(); // Idempotent after the modal closes.
});
test("a double click cannot queue another approval or execute twice", async () => {
  const gate = createConfirmation();
  const first = gate.request(prompt), second = gate.request(prompt);
  assert.equal(await second, false);
  const id = gate.current.value!.id;
  gate.settle(id, true); gate.settle(id, true);
  assert.equal(await first, true); assert.equal(gate.current.value, null);
});
test("a stale close event cannot approve or dismiss a newer action", async () => {
  const gate = createConfirmation();
  const first = gate.request(prompt), old = gate.current.value!.id;
  gate.settle(old, false); assert.equal(await first, false);
  const second = gate.request(prompt), latest = gate.current.value!.id;
  gate.settle(old, true); gate.settle(old, false);
  assert.equal(gate.current.value?.id, latest);
  gate.settle(latest, false); assert.equal(await second, false);
});
