import assert from "node:assert/strict";
import test from "node:test";
import { parseDeepLink, redeemedSubscription } from "../src/lib/deep-link.ts";

const code = "a1b2c3d4e5f6g7h8";

test("navigation links open a screen and do not carry a payload", () => {
  assert.deepEqual(parseDeepLink("clashnova://open"), { kind: "open" });
  assert.deepEqual(parseDeepLink("clashnova://nodes"), { kind: "nodes" });
  assert.deepEqual(parseDeepLink("clashnova://settings"), { kind: "settings" });
  assert.deepEqual(parseDeepLink("clashnova://connect"), { kind: "connect" });
  assert.equal(parseDeepLink("clashnova://open/extra"), null);
  assert.equal(parseDeepLink("https://example.com/open"), null);
});

test("an import link accepts one short-lived code", () => {
  assert.deepEqual(parseDeepLink(`clashnova://subscription/import?code=${code}`), { kind: "import", code });
  assert.deepEqual(parseDeepLink(`clashnova://subscription/import?token=${code}`), { kind: "import", code });
});

test("a subscription address or long credential is refused", () => {
  assert.deepEqual(parseDeepLink("clashnova://subscription/import?token=https://example.com/s/secret"), { kind: "rejected" });
  assert.deepEqual(parseDeepLink(`clashnova://subscription/import?url=${code}`), { kind: "rejected" });
  assert.deepEqual(parseDeepLink(`clashnova://subscription/import?code=${code}&token=${code}`), { kind: "rejected" });
  assert.deepEqual(parseDeepLink("clashnova://subscription/import?token=" + "a".repeat(65)), { kind: "rejected" });
  assert.deepEqual(parseDeepLink("clashnova://subscription/import?token=short"), { kind: "rejected" });
  assert.deepEqual(parseDeepLink("clashnova://subscription/import"), { kind: "rejected" });
});

test("a redeemed subscription must be an address, not the one-time code", () => {
  assert.deepEqual(redeemedSubscription({ name: "Everyday", url: "https://example.com/sub" }), { name: "Everyday", url: "https://example.com/sub" });
  assert.equal(redeemedSubscription({ url: `https://example.com/${code}`, name: "  " }).name, "");
  assert.equal(redeemedSubscription({ url: code }), null);
  assert.equal(redeemedSubscription({ url: "http://example.com/sub" }), null);
  assert.deepEqual(redeemedSubscription({ url: "http://127.0.0.1:8080/sub" })?.url, "http://127.0.0.1:8080/sub");
  assert.equal(redeemedSubscription({ url: "https://user:secret@example.com/sub" }), null);
});

// Clash clients import subscriptions with clash://install-config?url=...&name=...,
// where the address is in the link itself — always confirmed, and https-only.
const clash = (sub: string, name?: string) =>
  `clash://install-config?url=${encodeURIComponent(sub)}${name ? `&name=${encodeURIComponent(name)}` : ""}`;

test("clash install-config links carry the subscription address straight through", () => {
  assert.deepEqual(parseDeepLink(clash("https://sub.example.com/x?token=abc", "My Airport")),
    { kind: "importUrl", url: "https://sub.example.com/x?token=abc", name: "My Airport" });
  // Trailing slash after the action, and clashmeta://, are the same request.
  assert.deepEqual(parseDeepLink("clash://install-config/?url=" + encodeURIComponent("https://s/x")),
    { kind: "importUrl", url: "https://s/x", name: "" });
  assert.deepEqual(parseDeepLink(clash("https://s/y").replace("clash://", "clashmeta://")),
    { kind: "importUrl", url: "https://s/y", name: "" });
  // http is allowed only on localhost, consistent with the rest of the app.
  assert.deepEqual(parseDeepLink(clash("http://127.0.0.1:9/x")),
    { kind: "importUrl", url: "http://127.0.0.1:9/x", name: "" });
});

test("a clash link is refused unless it carries a safe https address", () => {
  assert.deepEqual(parseDeepLink(clash("http://evil.example.com/x")), { kind: "rejected" });
  assert.deepEqual(parseDeepLink(clash("https://user:pass@h/x")), { kind: "rejected" });
  assert.deepEqual(parseDeepLink("clash://install-config"), { kind: "rejected" });
  assert.deepEqual(parseDeepLink("clash://other?url=" + encodeURIComponent("https://s/x")), { kind: "rejected" });
});
