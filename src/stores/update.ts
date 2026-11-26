import { defineStore } from "pinia";
import { ref } from "vue";
import { msg, type Text } from "../i18n";
import { isNative } from "../lib/platform";
import { confirmation } from "../lib/confirmation";
import { useCoreStore } from "./core";

// update: app update state. Methods like check/download/install depend on the Tauri updater and will be moved into this store's actions later.
export const useUpdateStore = defineStore("update", () => {
  const installedVersion = ref("0.1.0");
  const checkingUpdates = ref(false);
  const installingUpdate = ref(false);
  const updateOffer = ref<{ version: string; notes: string } | null>(null);
  const updateStatus = ref<Text>("");
  let pendingUpdate: { downloadAndInstall: () => Promise<void> } | null = null;
  const { showError } = useCoreStore();

  async function checkUpdates() {
    if (checkingUpdates.value || installingUpdate.value) return;
    checkingUpdates.value = true;
    updateStatus.value = "";
    updateOffer.value = null;
    pendingUpdate = null;
    try {
      if (!isNative) {
        updateStatus.value = "settings.updateDesktopOnly";
        return;
      }
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      if (!update) {
        updateStatus.value = "settings.updateUpToDate";
        return;
      }
      pendingUpdate = update;
      updateOffer.value = { version: update.version, notes: update.body ?? "" };
    } catch {
      showError("error.updateCheckFailed");
    } finally {
      checkingUpdates.value = false;
    }
  }

  async function installUpdate() {
    if (!pendingUpdate || installingUpdate.value) return;
    installingUpdate.value = true;
    if (
      !(await confirmation.request({
        title: "confirmation.updateTitle",
        body: msg("confirmation.updateBody", {
          version: updateOffer.value?.version || installedVersion.value,
        }),
        confirmLabel: "settings.installUpdate",
      }))
    ) {
      installingUpdate.value = false;
      return;
    }
    updateStatus.value = "settings.installingUpdate";
    try {
      await pendingUpdate.downloadAndInstall();
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch {
      updateStatus.value = "";
      showError("error.updateInstallFailed");
    } finally {
      installingUpdate.value = false;
    }
  }

  return {
    installedVersion,
    checkingUpdates,
    installingUpdate,
    updateOffer,
    updateStatus,
    checkUpdates,
    installUpdate,
  };
});
