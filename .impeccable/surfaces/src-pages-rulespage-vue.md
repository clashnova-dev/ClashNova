---
version: 1
slug: "src-pages-rulespage-vue"
primary_target: "src/pages/RulesPage.vue"
related_targets: ["src/components/ui/SearchField.vue", "src/lib/rule-viewer.ts"]
---

# Rules

Mode: Operate. Existing-world, code-led extension of the desktop interface; no new concept or visual world is established.

Purpose: inspect generated routing from the selected subscription and app policies. The sidebar entry uses the established navigation. Keep PRODUCT.md authoritative for product boundaries and DESIGN.md for shared styling.

The page shows routing mode, group/entry totals and the preview limitation above a remaining-height data panel. SearchField and the existing SelectMenu align at 32px; the toolbar wraps in narrower windows. The table keeps a sticky header and an internal scroll region, with a fixed footer for result count and first/previous/next/last pagination. Its minimum width is 570px; narrow windows scroll inside the table region. Cells truncate with full-text titles; type titles retain group order. Empty states distinguish no subscription, invalid configuration and no matches; no-subscription state links to Subscriptions. Regex errors remain visible and accessible.

Source filters distinguish application, built-in, subscription and fallback rules. Case, whole-word and regex search share the Connections pattern. Search applies after a short delay (120ms); filters and changed data reset the page. Render at most 100 entries per page to support large subscriptions without mounting every rule. Preserve evaluation order and compound conditions; show the final fallback last. These are generated configuration entries, not observed hits. Native core-added TUN DNS/UDP rules are explicitly outside this preview.

Source evidence: src/pages/RulesPage.vue, src/components/ui/SearchField.vue, src/components/ui/SelectMenu.vue, src/lib/rule-viewer.ts, src/pages/ConnectionsPage.vue and src/styles/app.css. The scoped finish review accepted populated light, dark and wide-window captures without a material visual finding; this record adds no new UI requirement.
