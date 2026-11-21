import assert from "node:assert/strict";
import test from "node:test";
import { prepareConnection } from "../src/lib/connection-startup.ts";

test("notification consent cannot block or fail a ready connection", async () => {
  let requested = 0;
  await prepareConnection({ saveProfile: async () => {}, ensureTunnel: async () => {}, prepareNotifications: () => { requested++; return new Promise(() => {}); } });
  assert.equal(requested, 1);
  await prepareConnection({ saveProfile: async () => {}, ensureTunnel: async () => {}, prepareNotifications: async () => { throw new Error("permission denied"); } });
});
test("profile and tunnel readiness run concurrently and both gate connecting", async () => {
  let saveReady!: () => void, tunnelReady!: () => void;
  const events: string[] = [];
  const ready = prepareConnection({
    saveProfile: () => { events.push("save"); return new Promise<void>(r => { saveReady = r; }); },
    ensureTunnel: () => { events.push("tunnel"); return new Promise<void>(r => { tunnelReady = r; }); },
    prepareNotifications: async () => { events.push("notifications"); },
  }).then(() => events.push("connect"));
  assert.deepEqual(events, ["save", "tunnel"]);
  saveReady(); await Promise.resolve(); assert.deepEqual(events, ["save", "tunnel"]);
  tunnelReady(); await ready;
  assert.deepEqual(events, ["save", "tunnel", "notifications", "connect"]);
});
test("a profile/service preparation failure prevents connection and notification requests", async () => {
  for (const failure of ["profile", "service"]) {
    let requested = false;
    await assert.rejects(prepareConnection({
      saveProfile: async () => { if (failure === "profile") throw new Error("invalid profile"); },
      ensureTunnel: async () => { if (failure === "service") throw new Error("service unavailable"); },
      prepareNotifications: async () => { requested = true; },
    }));
    assert.equal(requested, false);
  }
});
