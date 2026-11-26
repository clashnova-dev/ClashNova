import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { notifications, prepareNotifications } from "./notifications";
import { translator, type Locale } from "../i18n";
export type TunStatus = {
  ready: boolean;
  running: boolean;
  outdated: boolean;
  message: string;
};
export const tunStatus = ref<TunStatus>({
  ready: false,
  running: false,
  outdated: false,
  message: "tun.tunServiceHasNotBeen",
});
export const tunServiceBusy = ref(false);
const native = () => "__TAURI_INTERNALS__" in window;
export async function refreshTunService() {
  if (!native()) {
    tunStatus.value = {
      ready: false,
      running: false,
      outdated: false,
      message: "tun.tunServiceIsAvailableIn",
    };
    return tunStatus.value;
  }
  tunStatus.value = await invoke<TunStatus>("tun_service_status");
  return tunStatus.value;
}
export async function installTunService(locale: Locale) {
  if (tunServiceBusy.value) throw new Error("tun.busy");
  if (!native()) throw new Error("tun.tunServiceIsAvailableIn");
  tunServiceBusy.value = true;
  const { t, tx } = translator(() => locale);
  try {
    await prepareNotifications();
    tunStatus.value = await invoke<TunStatus>("install_tun_service");
    await notifications.show({
      body: t("tun.tunServiceInstalled"),
      kind: "success",
      native: true,
    });
    return tunStatus.value;
  } catch (error) {
    await refreshTunService().catch(() => {});
    await notifications.show({
      body: tx(String(error)),
      nativeBody: t("tun.tunServiceInstallationDidNot"),
      kind: "error",
      native: true,
    });
    throw error;
  } finally {
    tunServiceBusy.value = false;
  }
}
// A service that is merely from an earlier app version still works, so connecting goes ahead;
// the user is told once per run that a repair would update the privileged copies.
let outdatedNoticeShown = false;
export async function ensureTunService(locale: Locale) {
  const status = await refreshTunService();
  if (!status.ready) {
    await installTunService(locale);
    return;
  }
  if (status.outdated && !outdatedNoticeShown) {
    outdatedNoticeShown = true;
    const { t } = translator(() => locale);
    await notifications.show({ body: t("tun.serviceOutdatedNotice"), kind: "info" }).catch(() => {});
  }
}
export async function uninstallTunService(locale: Locale) {
  if (tunServiceBusy.value) return;
  tunServiceBusy.value = true;
  const { t, tx } = translator(() => locale);
  try {
    tunStatus.value = await invoke<TunStatus>("uninstall_tun_service");
    await notifications.show({
      body: t("tun.tunServiceUninstalled"),
      kind: "success",
      native: true,
    });
  } catch (error) {
    await notifications.show({
      body: tx(String(error)),
      nativeBody: t("tun.tunServiceRemovalDidNot"),
      kind: "error",
      native: true,
    });
  } finally {
    tunServiceBusy.value = false;
  }
}
