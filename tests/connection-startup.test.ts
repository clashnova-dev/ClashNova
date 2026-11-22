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

test('failed preparation waits for the other side effect before allowing another connection attempt', async () => {
  for (const failing of ['profile', 'service']) {
    let finish!: () => void, settled = false, notifications = 0;
    const waiting = new Promise<void>(resolve => { finish = resolve; });
    const operation = prepareConnection({
      saveProfile: () => failing === 'profile' ? Promise.reject(new Error('profile failed')) : waiting,
      ensureTunnel: () => failing === 'service' ? Promise.reject(new Error('service failed')) : waiting,
      prepareNotifications: async () => { notifications++; },
    });
    const checked = assert.rejects(operation, new RegExp(`${failing} failed`)).then(() => { settled = true; });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(settled, false);
    finish(); await checked;
    assert.equal(settled, true);
    assert.equal(notifications, 0);
  }
});
