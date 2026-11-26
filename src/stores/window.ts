import { defineStore } from "pinia";
import { watch } from "vue";
import { storeToRefs } from "pinia";
import { msg } from "../i18n";
import { isNative } from "../lib/platform";
import { enableLogin } from "../constants/flags";
import { MAIN_WINDOW, acceptMainWindow } from "../lib/window-size";
import { readClientSetting, writeClientSetting } from "../lib/settings-store";
import { MAIN_WINDOW_KEY } from "../constants/storage";
import { useCoreStore } from "./core";
import { useAccountStore } from "./account";

// window: lifecycle of native window geometry and visibility (login/main window switching, remembered size, right-click blocking).
// Desktop only; all methods safely no-op in a non-native environment.
export const useWindowStore = defineStore("window", () => {
  const { message } = storeToRefs(useCoreStore());
  const { access, localMode } = storeToRefs(useAccountStore());

  // The login window is card-sized; the main interface is larger and keeps the size the user set.
  const LOGIN_WINDOW = { width: 360, height: 500 };
  let windowMode: "login" | "main" | "" = "";
  let windowBooted = false;
  // Stay hidden until subscriptions and the session are restored, to avoid drawing the login window at startup and then growing it into the main window.
  let windowReady = false;
  let windowQueue: Promise<void> = Promise.resolve();
  let stopWindowResize: (() => void) | undefined;
  let windowSaveTimer: ReturnType<typeof setTimeout> | undefined;
  let rememberedMainWindow = {
    width: MAIN_WINDOW.width,
    height: MAIN_WINDOW.height,
    defaultWidth: MAIN_WINDOW.width,
  };
  function savedMainWindow() {
    return rememberedMainWindow;
  }
  // Queue the work: if a login-state change coincides with a previous resize still in progress, they won't interleave.
  function syncWindowMode() {
    if (!isNative) return windowQueue;
    windowQueue = windowQueue
      .then(async () => {
        if (!windowReady) return;
        const mode = enableLogin && !access.value && !localMode.value ? "login" : "main";
        const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
        const win = getCurrentWindow();
        if (mode !== windowMode) {
          windowMode = mode;
          if (await win.isMaximized()) await win.unmaximize();
          if (mode === "login") {
            await win.setMinSize(new LogicalSize(LOGIN_WINDOW.width, LOGIN_WINDOW.height));
            await win.setSize(new LogicalSize(LOGIN_WINDOW.width, LOGIN_WINDOW.height));
            await win.setResizable(false);
          } else {
            const size = savedMainWindow();
            await win.setResizable(true);
            await win.setSize(new LogicalSize(size.width, size.height));
            await win.setMinSize(new LogicalSize(MAIN_WINDOW.minWidth, MAIN_WINDOW.minHeight));
          }
          // The window is still hidden during startup; login/logout re-centers it.
          if (windowBooted) await win.center();
        }
        // Only show once the size matches the current screen, to avoid flashing the other window type at startup.
        await win.show();
      })
      .catch(() => {
        windowMode = ""; /* Retry on the next change; the window is still usable at its current size. */
      });
    return windowQueue;
  }
  async function watchWindowSize() {
    if (!isNative) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    stopWindowResize = await win.onResized(() => {
      clearTimeout(windowSaveTimer);
      windowSaveTimer = setTimeout(async () => {
        try {
          if (windowMode !== "main" || (await win.isMaximized()) || (await win.isFullscreen())) return;
          const size = (await win.innerSize()).toLogical(await win.scaleFactor());
          const next = {
            width: Math.round(size.width),
            height: Math.round(size.height),
            defaultWidth: MAIN_WINDOW.width,
          };
          await writeClientSetting("mainWindow", next, MAIN_WINDOW_KEY);
          rememberedMainWindow = next;
        } catch {
          /* Remembering the size is best-effort. */
        }
      }, 400);
    });
  }
  async function revealMain() {
    if (!isNative) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.show();
    await win.unminimize();
    await win.setFocus();
  }
  async function windowAction(action: "minimize" | "toggleMaximize" | "close") {
    if (!("__TAURI_INTERNALS__" in window)) {
      message.value = "error.windowControlsDesktopOnly";
      return;
    }
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow()[action]();
    } catch (error) {
      message.value = msg("error.windowActionFailed", { error: String(error) });
    }
  }
  function preventContextMenu(event: MouseEvent) {
    event.preventDefault();
  }

  // Load the remembered main-window size during bootstrap.
  async function loadRemembered() {
    rememberedMainWindow = await readClientSetting("mainWindow", rememberedMainWindow, acceptMainWindow, {
      key: MAIN_WINDOW_KEY,
      parse: raw => {
        try {
          return acceptMainWindow(JSON.parse(raw));
        } catch {
          return null;
        }
      },
    });
    await writeClientSetting("mainWindow", rememberedMainWindow, MAIN_WINDOW_KEY);
  }
  // End of bootstrap: once subscriptions/session are ready, release and show the window.
  async function revealAfterBoot() {
    windowReady = true;
    await syncWindowMode();
    windowBooted = true;
  }
  function startContextGuard() {
    document.addEventListener("contextmenu", preventContextMenu);
  }
  function teardown() {
    stopWindowResize?.();
    clearTimeout(windowSaveTimer);
    document.removeEventListener("contextmenu", preventContextMenu);
  }

  watch([access, localMode], () => {
    void syncWindowMode();
  });

  return {
    savedMainWindow,
    syncWindowMode,
    watchWindowSize,
    revealMain,
    windowAction,
    preventContextMenu,
    loadRemembered,
    revealAfterBoot,
    startContextGuard,
    teardown,
  };
});
