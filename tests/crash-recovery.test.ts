import assert from "node:assert/strict";
import test from "node:test";
import { restoreCrashConnection } from "../src/lib/crash-recovery.ts";

function scenario({requested = true, profile = true, tun = false, ready = true} = {}) {
  const events: string[] = [];
  return { events, deps: {
    requested: async () => requested, canConnect: () => profile, tun,
    tunReady: async () => { events.push("check-service"); return ready; },
    connect: async () => { events.push("connect"); }, report: (key: string) => { events.push(key); },
  }};
}
test("normal launch never connects or requests service access", async () => {
  const { events, deps } = scenario({requested: false, tun: true});
  await restoreCrashConnection(deps); assert.deepEqual(events, []);
});
test("crash restores a previously connected profile", async () => {
  const { events, deps } = scenario();
  await restoreCrashConnection(deps); assert.deepEqual(events, ["recovery.restoring", "connect"]);
});
test("a lost profile never starts a connection", async () => {
  const { events, deps } = scenario({profile: false});
  await restoreCrashConnection(deps); assert.deepEqual(events, ["recovery.profileUnavailable"]);
});
test("TUN recovery checks the installed service and never installs it", async () => {
  const unavailable = scenario({tun: true, ready: false});
  await restoreCrashConnection(unavailable.deps);
  assert.deepEqual(unavailable.events, ["check-service", "recovery.tunUnavailable"]);
  const available = scenario({tun: true}); await restoreCrashConnection(available.deps);
  assert.deepEqual(available.events, ["check-service", "recovery.restoring", "connect"]);
});
test("failed recovery propagates without retrying indefinitely", async () => {
  const { events, deps } = scenario();
  deps.connect = async () => { events.push("failed"); throw new Error("offline"); };
  await assert.rejects(restoreCrashConnection(deps), /offline/);
  assert.deepEqual(events, ["recovery.restoring", "failed"]);
});
