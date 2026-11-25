<div align="center">

<img src="app-icon.png" width="116" alt="ClashNova" />

<h1>ClashNova Desktop</h1>

<p>
  <b>A local-first proxy client built on <a href="https://sing-box.sagernet.org/">sing-box</a>.</b><br/>
  Import a subscription and you're connected — parsing, routing and the outbound connection all happen on your machine.
</p>

<p>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/License-GPL--3.0-2f6fed?style=flat-square"></a>
  <img alt="Platforms" src="https://img.shields.io/badge/macOS%20·%20Windows%20·%20Linux-5b6472?style=flat-square">
  <img alt="Vue 3" src="https://img.shields.io/badge/Vue_3-42b883?style=flat-square&logo=vuedotjs&logoColor=white">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri_2-24c8db?style=flat-square&logo=tauri&logoColor=white">
  <img alt="sing-box core" src="https://img.shields.io/badge/core-sing--box-7d56f4?style=flat-square">
  <a href="https://x.com/ClashNova2048"><img alt="Follow on X" src="https://img.shields.io/badge/Follow-%40ClashNova2048-000000?style=flat-square&logo=x&logoColor=white"></a>
</p>

<p>
  <b>English</b> ·
  <a href="docs/i18n/README.zh-CN.md">简体中文</a> ·
  <a href="docs/i18n/README.de.md">Deutsch</a> ·
  <a href="docs/i18n/README.fr.md">Français</a> ·
  <a href="docs/i18n/README.ja.md">日本語</a> ·
  <a href="docs/i18n/README.ko.md">한국어</a> ·
  <a href="docs/i18n/README.ru.md">Русский</a>
</p>

<img src="docs/screenshots/overview-light.jpg" width="840" alt="ClashNova Desktop — Overview" />

</div>

## ✨ Features

- **Overview** — connect or disconnect in one click, with live connection state, the current node, real-time up/down speed, and session usage. Subscription usage and the egress IP with its geo lookup are shown inline (hideable, refreshed manually or on a timer), alongside quick switching between your frequent nodes.
- **Nodes** — search, sort by latency, or run *Test all* to measure every node asynchronously; results stream in and can be re-sorted by lowest latency. Pick a node by hand, or leave it on automatic selection (the core's `urltest` group keeps choosing the fastest).
- **Subscriptions** — import Clash, sing-box, or universal (share-link) subscriptions from a URL or a local file; the conversion runs locally rather than through an online service. Refresh, copy, or remove each one, and see its usage and expiry when the provider reports them.
- **Connection & routing** — choose **system proxy**, **manual proxy**, or **TUN** (a virtual network interface, which requires system authorization). Switch between **rule** and **global** routing, and configure the local port, LAN access, and more. An active-connections list, the core's live log, and traffic statistics are all available.
- **Network protection** — protection-mode presets, DNS-leak protection, an IPv6 policy, a kill switch and malicious-domain blocking, plus on-demand health checks, DNS / IPv6 / WebRTC leak tests, and streaming / AI unlock detection.
- **System integration** — system tray, launch at login, and light / dark / follow-system themes. The interface is available in Simplified Chinese, English, 日本語, 한국어, Deutsch, Français, and Русский.

## 🌟 Highlights

A few things that go beyond a plain subscription client.

### Proxy chain

Relay traffic through several nodes in order — an entry, any number of relays, and an exit. Build a chain by hand, measure its end-to-end latency, and save the ones you use as named presets. Latency adds up across the hops and the whole chain is only as good as its weakest link, so the editor keeps the full path in view.

<p align="center"><img src="docs/screenshots/special-chain.jpg" width="760" alt="Proxy chain editor"></p>

### Network protection & checks

The **Network** page gathers everything about how traffic leaves the machine. Choose how proxying is applied — system proxy, TUN, or manual — with a plain-language note on what each mode does and doesn't cover. The **kill switch** (TUN mode) blocks all traffic the moment the tunnel drops, so nothing falls back to your real connection. On demand, run a health check (local network, proxy reachability, DNS, exit IP), DNS / IPv6 / WebRTC leak tests, and streaming / AI-service unlock detection.

<p align="center"><img src="docs/screenshots/special-network.jpg" width="760" alt="Network protection and checks"></p>

### LAN transfer

Send text and files to your own devices on the same network, with no server or cloud in between. It works like a personal file-helper chat: pair a device once, then exchange messages and files in a thread. Pairing establishes trust, so a paired device can receive automatically.

<p align="center"><img src="docs/screenshots/special-lan.jpg" width="760" alt="LAN transfer"></p>

### Routing rules

Inspect the routing table that is actually in effect — each rule's type, what it matches, where it sends traffic (direct / proxy / block), and whether it came from your subscription or the built-in defaults. Search by content, type, or target to see why a given domain goes where it goes.

<p align="center"><img src="docs/screenshots/special-rules.jpg" width="760" alt="Routing rules"></p>

## 📸 Screenshots

| Overview | Nodes | Subscriptions |
| :-: | :-: | :-: |
| <img src="docs/screenshots/overview-dark.jpg" width="240"> | <img src="docs/screenshots/nodes.jpg" width="240"> | <img src="docs/screenshots/subscriptions.jpg" width="240"> |

| Connections | Logs | Settings |
| :-: | :-: | :-: |
| <img src="docs/screenshots/connections.jpg" width="240"> | <img src="docs/screenshots/logs.jpg" width="240"> | <img src="docs/screenshots/settings.jpg" width="240"> |

## 🛠️ Building

You'll need Node.js with pnpm, Rust, Go, Python 3, and the Tauri system dependencies for your platform.

```sh
pnpm install --frozen-lockfile
python3 scripts/prepare-sidecars.py   # compile the sing-box sidecar from source
pnpm tauri dev                         # development
pnpm tauri build                       # package a release build
```

`prepare-sidecars` builds the core and the subscription converter from source instead of downloading prebuilt binaries. Tauri's dev and build hooks run it automatically, so you don't need to invoke it by hand after changing the core sources.

To preview the UI without installing Rust or Go, run the frontend on its own with `pnpm dev` and open it in a browser. `browserBridge()` stubs out the native side, so anything that depends on the core — the connection, egress IP, live speed — still has to be verified in the real desktop build.

## 🍎 Running on macOS

Release builds are not code-signed or notarized (there is no Apple Developer certificate), so macOS may refuse to open a downloaded build ("ClashNova is damaged…" or an unidentified-developer warning). Clear the quarantine flag, then open the app normally — adjust the path if you installed it elsewhere:

```sh
sudo xattr -rd com.apple.quarantine "/Applications/ClashNova.app"
```

## ⚙️ Configuration

Build-time options are read by Vite from a `.env` file (ignored by `.gitignore`):

```
VITE_DEFAULT_SERVER=     # optional backend URL; leave empty to run as a standalone subscription client
VITE_ENABLE_LOGIN=false  # when false, the account login screen is hidden
```

Left at their defaults, the app is a self-contained subscription client that needs no server. The two flags are the extension point for wiring it to your own backend.

## 🗂️ Project layout

```
src/
  App.vue                 shell: side navigation plus the individual pages
  composables/useApp.ts   state hub (session / subscriptions / nodes / connection / settings)
  pages/                  Overview / Nodes / Subscriptions / Connections / Logs / Network / Settings / Help
  i18n/                   message catalogs
  lib/                    connection state machine, subscription parsing, preferences, TUN service
  styles/app.css          light and dark design tokens
src-tauri/                Tauri (Rust) host: windows, tray, and the lifecycle of the core and TUN helper
```

The actual outbound traffic is forwarded by the bundled sing-box core.

## 📄 License

GPL-3.0 — see [LICENSE](LICENSE). The bundled sing-box core is compiled from source (GPL-3.0), so the project as a whole is released under GPL-3.0; distributions must follow the same license and make the corresponding source available.
