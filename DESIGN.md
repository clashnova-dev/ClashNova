---
name: ClashNova
description: Compact, theme-aware desktop proxy controls.
colors:
  accent: "#2f6fdd"
  accent-text: "#2459b8"
  bg: "#fff"
  surface: "#f4f6fa"
  text: "#1c2533"
  strong: "#344760"
  muted: "#5b677a"
  line: "#e6eaf0"
  border: "#d9e0ea"
  hover: "#f2f6fc"
  selected: "#f1f6ff"
  nav-on: "#e6eefb"
  selected-border: "#a9c4ef"
  focus: "#8eb5ee"
  danger: "#b3403a"
  accent-dark: "#457bd0"
  accent-text-dark: "#94bbf8"
  bg-dark: "#171d27"
  surface-dark: "#121822"
  text-dark: "#e6edf6"
  strong-dark: "#d5e1f1"
  muted-dark: "#a1b0c5"
  line-dark: "#2d3746"
  border-dark: "#3a4658"
  hover-dark: "#222d3d"
  selected-dark: "#22344f"
  nav-on-dark: "#22344f"
  selected-border-dark: "#5076a9"
  focus-dark: "#84b3fa"
  danger-dark: "#ffaaa1"
typography:
  title:
    fontFamily: "-apple-system,BlinkMacSystemFont,\"Segoe UI\",\"PingFang SC\",\"Microsoft YaHei\",sans-serif"
    fontSize: "19px"
    fontWeight: 650
    lineHeight: 1.5
  body:
    fontFamily: "-apple-system,BlinkMacSystemFont,\"Segoe UI\",\"PingFang SC\",\"Microsoft YaHei\",sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.5
  table:
    fontFamily: "-apple-system,BlinkMacSystemFont,\"Segoe UI\",\"PingFang SC\",\"Microsoft YaHei\",sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "-apple-system,BlinkMacSystemFont,\"Segoe UI\",\"PingFang SC\",\"Microsoft YaHei\",sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: 1.5
rounded:
  field: "8px"
  button: "9px"
  select: "10px"
  list: "12px"
  data-panel: "14px"
  panel: "16px"
spacing:
  compact: "8px"
  gutter: "10px"
  control: "12px"
  cell: "14px"
  inset: "24px"
components:
  button:
    backgroundColor: "{colors.bg}"
    textColor: "{colors.strong}"
    rounded: "{rounded.button}"
    padding: "8px 14px"
  button-primary:
    backgroundColor: "{colors.accent}"
    rounded: "{rounded.button}"
    padding: "8px 14px"
  search-field:
    textColor: "{colors.muted}"
    rounded: "{rounded.field}"
    height: "32px"
  nav-active:
    backgroundColor: "{colors.nav-on}"
    textColor: "{colors.accent-text}"
  panel:
    backgroundColor: "{colors.bg}"
    rounded: "{rounded.panel}"
    padding: "20px 22px"
---

# Design System: ClashNova

## Overview

ClashNova uses a restrained desktop workspace: persistent navigation, compact controls and readable operational data. The existing application icon and interface remain authoritative under PRODUCT.md. This record captures the built system; src/styles/app.css owns theme values and shared styles, while existing UI components own their interaction details.

**Key Characteristics:**
- System typography and compact hierarchy.
- Blue selection on neutral light and dark surfaces.
- Bounded data regions with visible controls.

## Colors

Blue identifies selection, active navigation and primary actions. Neutral background, surface, text, muted text and divider roles form the workspace. The frontmatter records the paired light and dark values; its keys map directly to the identically named CSS variables, with the `-dark` suffix describing the dark override. Danger is reserved for errors and destructive feedback.

**The Theme Pair Rule.** Use the matching light or dark semantic token set together; selected backgrounds and accent text have separate roles.

## Typography

Use the system stack for app text. Page titles are compact and semibold; body, data and labels step down without a separate display face. Dense tables use the table role with smaller, medium-weight headers. Preserve localized fallback fonts. Numeric pagination and data totals use tabular numerals where implemented.

## Layout

The desktop shell fills the viewport, with a fixed sidebar (182px) and flexible workspace. The header stays above the main region; child data regions use flex layout with zero minimum height so their contents scroll inside the available space. At the existing narrow-shell breakpoint (839px), the sidebar contracts (162px). These are desktop window adaptations.

**The Control Alignment Rule.** Dense data toolbars align search and select triggers at the same compact height.

## Elevation & Depth

Fine borders and neutral surface changes organize data. Panels have a subtle card shadow; menus use a stronger floating shadow. Focus is visible through the shared button outline or the field/select border treatment. Keep menu elevation distinct from data containers.

## Shapes

Controls have softly rounded corners; larger containers use a broader radius. The field, button, select, list and panel roles in the frontmatter describe the recurring hierarchy. Borders are generally thin (1px), with clipped table containers and restrained hover fills.

## Components

- **Buttons:** neutral bordered actions, blue primary actions, subdued disabled states. Shared hover and keyboard focus styles come from app.css; compact data controls override size locally.
- **SearchField:** the Connections search pattern extracted into a reusable component. It combines a search icon, text input, clear action and case/whole-word/regex toggles. Active toggles use selected fill and accent text; invalid regex uses danger border and accessible error text.
- **SelectMenu:** shared theme-aware combobox with truncated selected label, chevron and floating list. Hover tracks the active option; the chosen option has accent text and a check. Arrow/Home/End navigation, Enter/Space selection, Escape and focus restoration are implemented.
- **Navigation:** persistent icon-and-label sidebar actions with muted default text and a blue-tinted active row. Rules uses the existing Route icon and navigation pattern.
- **Containers and data tables:** subtle border, neutral background, small muted sticky headers, hover fill and clipped cells with full-text titles. Rules-specific sizing, pagination and states live in its surface brief.

## Do's and Don'ts

- Do reuse the existing theme variables and UI components.
- Do keep keyboard focus visible and localize labels and states.
- Do truncate dense table values with access to their full text.
- Don't hard-code light colors into dark surfaces.
- Don't replace the established icon or desktop shell for a page addition.
