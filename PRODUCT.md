# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

The existing Vue interface runs inside a Tauri desktop host on Windows, Linux and macOS. This record covers the desktop client repository.

## Users

Desktop users import a proxy subscription, select a node and choose how the client handles their traffic. They inspect connections, routing rules, logs and network diagnostics when troubleshooting.

## Product Purpose

ClashNova is a desktop subscription proxy client using the bundled sing-box core. The primary workflow is importing a subscription, selecting a node, choosing system/manual proxy or TUN handling, connecting, and inspecting the resulting client state.

## Capabilities and Constraints

- The standalone build does not require account login; backend login is an existing optional build configuration.
- The sidebar exposes overview, nodes, subscriptions, connections, rules, logs, network and settings.
- Rules shows routing generated from the selected subscription and application policies, preserving evaluation order and compound conditions. It supports source filtering, case/word/regex search and bounded pagination.
- The rule viewer describes configuration, not measured rule hits. Native TUN DNS/UDP capture rules are appended separately by the core and are identified as outside this preview.
- System/manual proxy and TUN have different routing coverage. Diagnostics must distinguish observed evidence, configuration state and unconfirmed results.
- Desktop configuration and preferences use private native files. Legacy webview storage is migration input only, with cleanup after verified persistence; file failures must not silently become browser persistence.
- Existing subscriptions and settings must be preserved during updates.
- Network-affecting and sensitive changes require explicit confirmation. TUN service installation/removal follows operating-system authorization.
- Releases and signed desktop updates use the official GitHub repository.

## Brand Commitments

- Product name: ClashNova.
- Public developer identity: clashnova-dev.
- Official desktop repository: https://github.com/clashnova-dev/ClashNova.
- Preserve the existing application icon and the established desktop interface. Clash Verge Rev is a user-supplied reference for desktop proxy-client behavior.
- Do not add Telegram group links or personal developer details.

## Evidence on Hand

- Existing source, app assets, styles, configuration contracts and test suites in this repository.
- README.md documents local file locations, routing coverage and release behavior.
- No product-specific demographic, positioning claim or marketing performance claim has been established; future work should not invent one.

## Product Principles

- Show actual client configuration and evidence without claiming that a setting alone proves privacy.
- Preserve local user data and make persistence failures visible.
- Keep large rule and log datasets usable without rendering every entry at once.
- Keep network configuration and routing execution in the native/core boundaries.
