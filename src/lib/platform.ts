import { platform } from "@tauri-apps/plugin-os";

// Runtime environment constants (initialized once, immutable), shared across the whole app.
// Non-reactive state; following common practice, a plain shared module is used rather than a Pinia store.
export const isNative = "__TAURI_INTERNALS__" in window;
export const currentPlatform = isNative ? platform() : "web";
export const isMac = currentPlatform === "macos";
export const isLinux = currentPlatform === "linux";
