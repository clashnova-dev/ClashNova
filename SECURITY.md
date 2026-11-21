# Security Policy

## Supported versions

Security fixes land on the latest release. Please reproduce issues against the current `master` or the newest tagged build before reporting.

## Reporting a vulnerability

Please do **not** open a public issue for a security vulnerability.

Email **clashnova.dev@gmail.com** with:

- a description of the issue and its impact;
- the steps, configuration, or proof-of-concept needed to reproduce it;
- the affected version or commit, and your platform (macOS / Windows / Linux).

You can expect an acknowledgement within a few days. We'll confirm the issue, work on a fix, and coordinate a disclosure timeline with you. Please give us a reasonable window to release a fix before any public disclosure.

## Scope

This client runs locally and forwards traffic through a bundled sing-box core. Useful reports include flaws in subscription parsing or conversion, the local proxy and TUN handling, privilege boundaries of the TUN helper, or the handling of configuration and credentials on disk. Vulnerabilities in sing-box itself should be reported upstream to the [sing-box project](https://github.com/SagerNet/sing-box).

## Local hardening review — 2026-11-21

This review covers the desktop webview/native boundary, subscription ingestion, ordinary and privileged core configuration, preferences, and the authenticated TUN IPC service. It is a source review plus local regression testing, not a security certification.

Changes implemented in this checkout:

- Replaced Windows `cmd /C start` URL opening with `ShellExecuteW`, with parsed URL, scheme, credential and control-character checks. This removes a shell-interpreter boundary; Windows behavior still needs target-machine QA.
- Shared local-file/extension restrictions between ordinary proxy startup, latency tests and TUN configuration. Native code owns listener addresses and drops extra imported listeners. HTTP transport paths, HTTPS DNS paths and process-path routing matches remain supported.
- Rejected HTTPS subscription redirects to local HTTP services and HTTP loopback redirects to different origins. Explicit same-origin loopback HTTP subscriptions remain supported. This is not a claim that every possible DNS-rebinding or private-network scenario has been eliminated.
- Bounded subscription conversion to 15 seconds and 4 MiB of output. A timeout, excessive output or failed conversion kills/reaps the child; raw converter diagnostics are not exposed to the webview.
- Removed unrestricted webview Store access. Only fixed preference keys in the existing `airport-settings.json` file are exposed through native IPC. Atomic writes use private Unix permissions; existing values are retained and corrupt files are not overwritten. Subscription data remains plaintext under OS account access control.
- Bounded the TUN service's authenticated request queue and per-tick accept/dispatch batches, so queued frames/sockets cannot grow without limit or indefinitely starve lease maintenance. Unauthenticated read deadlines, AEAD and replay checks remain in place.
- Hardened CSP directives and verified the production bundle rejects injected inline scripts, base URLs and remote frames while settings and subscription interactions still work.
- Raised the bundled core build to Go 1.27.4 and updated affected compression, crypto, HTTP/2, gRPC, DHCP and routing dependencies. Sidecar builds use a minimum Go toolchain for the converter too.

Dependency audit status (2026-11-21):

- `pnpm audit --prod --registry https://registry.npmjs.org`: no reports.
- `govulncheck` with the shipped build tags, on darwin/arm64, and a binary scan of the bundled subscription converter: no findings after updates. The tags include gVisor, QUIC, DHCP, WireGuard, uTLS and Clash API.
- `cargo audit`: no entries in the vulnerability list, **but one unsoundness warning and six unmaintained-package warnings remain**. In particular, [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) affects `glib 0.18.5` in the Linux GTK/WebKit/Tauri dependency chain. The patched branch is `glib >= 0.20`; a coordinated GUI dependency-chain upgrade and Linux QA remain outstanding. This warning was not suppressed.

Verification includes malformed/unsafe URL cases, local-file reference rejection in connections and probes, converter timeout/output limits and a real bundled-converter fixture, legacy preference retention and private permissions, corrupt/symlink preference rejection, queue overflow socket cleanup, helper replay/tamper tests, release compilation and production-CSP browser tests. Tests use temporary files, isolated loopback listeners or mocked native IPC. They do not authorize reinstalling a system service or changing the user's current proxy/TUN.

The locally rebuilt sidecars do not replace an already installed privileged TUN service. After installing the updated client, use **Repair service** in Settings and approve the OS administrator prompt to update its protected binaries. Windows/Linux full-machine validation and signing/notarization validation are outside this local review.

References: [Go releases](https://go.dev/dl/), [Tauri CSP](https://v2.tauri.app/security/csp/), [Go vulnerability database](https://pkg.go.dev/vuln/).
