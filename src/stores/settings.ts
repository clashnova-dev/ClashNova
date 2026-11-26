import { defineStore, storeToRefs } from "pinia";
import { ref, computed } from "vue";
import {
  DEFAULT_PROXY_PORT,
  DEFAULT_TUN_MTU,
  defaultTunSettings,
  parseRouteExclude,
  validateProxyPort,
  validateTunMtu,
  proxyAddress,
  type NetworkSettings,
  type TunSettings,
} from "../lib/network-settings";
import { defaultNetworkSecurity, type NetworkSecurity } from "../lib/network-security";
import { type AppPolicy } from "../lib/app-policies";
import { networkSettingsStoreKey, networkSecurityStoreKey } from "../lib/settings-store";
import { confirmation } from "../lib/confirmation";
import { tunServiceBusy } from "../lib/tun-service";
import { type Text } from "../i18n";
import { useCoreStore } from "./core";
import { useI18nStore } from "./i18n";
import { useSubscriptionsStore } from "./subscriptions";
import { useLogsStore } from "./logs";
import { useConnectionStore } from "./connection";

// settings: network settings / app policies / autostart / diagnostics / tray. Saving, applying, and diagnostics depend on the backend bridge and persistent storage.
export const useSettingsStore = defineStore(
  "settings",
  () => {
    const networkSettings = ref<NetworkSettings>({
      proxyPort: DEFAULT_PROXY_PORT,
      allowLan: false,
      tun: defaultTunSettings(),
    });
    const networkSecurity = ref<NetworkSecurity>(defaultNetworkSecurity());
    const appPolicies = ref<Record<string, AppPolicy>>({});
    const draftProxyPort = ref(String(DEFAULT_PROXY_PORT));
    const draftAllowLan = ref(false);
    const draftTun = ref<TunSettings>(defaultTunSettings());
    const draftTunMtu = ref(String(DEFAULT_TUN_MTU));
    const draftTunExclude = ref("");
    const networkLoaded = ref(false);
    const networkError = ref<Text>("");
    const showTraySpeed = ref(false);
    const autostartEnabled = ref(false);
    const autostartReady = ref(false);
    const autostartBusy = ref(false);
    const autostartError = ref<Text>("");
    const proxyDiagnostic = ref<Text>("settings.checkingSystemProxy");
    const diagnosticBusy = ref(false);

    function storedNetwork(value: unknown): NetworkSettings | null {
      if (!value || typeof value !== "object") return null;
      const item = value as NetworkSettings;
      if (typeof item.proxyPort !== "number" || typeof item.allowLan !== "boolean") return null;
      return {
        proxyPort: item.proxyPort,
        allowLan: item.allowLan,
        tun: { ...defaultTunSettings(), ...(item.tun ?? {}) },
      };
    }
    function sameNetwork(left: NetworkSettings, right: NetworkSettings) {
      const a = storedNetwork(left),
        b = storedNetwork(right);
      return (
        !!a &&
        !!b &&
        a.proxyPort === b.proxyPort &&
        a.allowLan === b.allowLan &&
        JSON.stringify(a.tun) === JSON.stringify(b.tun)
      );
    }
    function applyNetworkSettings(settings: NetworkSettings) {
      // An older native build does not send the device options.
      const tun = { ...defaultTunSettings(), ...(settings.tun ?? {}) };
      networkSettings.value = { ...settings, tun };
      draftProxyPort.value = String(settings.proxyPort);
      draftAllowLan.value = settings.allowLan;
      draftTun.value = { ...tun, routeExclude: [...tun.routeExclude] };
      draftTunMtu.value = String(tun.mtu);
      draftTunExclude.value = tun.routeExclude.join("\n");
    }
    const networkDirty = computed(() => {
      const saved = networkSettings.value,
        tun = saved.tun,
        draft = draftTun.value;
      return (
        draftProxyPort.value !== String(saved.proxyPort) ||
        draftAllowLan.value !== saved.allowLan ||
        draft.stack !== tun.stack ||
        draft.strictRoute !== tun.strictRoute ||
        draft.ipv6 !== tun.ipv6 ||
        draft.autoRedirect !== tun.autoRedirect ||
        draft.killSwitch !== tun.killSwitch ||
        draft.dnsMode !== tun.dnsMode ||
        draft.dnsRemote !== tun.dnsRemote ||
        draft.dnsDomestic !== tun.dnsDomestic ||
        draftTunMtu.value.trim() !== String(tun.mtu) ||
        draftTunExclude.value.trim() !== tun.routeExclude.join("\n")
      );
    });
    const localProxyAddress = computed(() => proxyAddress(networkSettings.value.proxyPort));

    async function refreshProxyDiagnostics() {
      const { bridge } = useConnectionStore();
      if (diagnosticBusy.value) return;
      diagnosticBusy.value = true;
      try {
        proxyDiagnostic.value = await (await bridge()).proxyDiagnostics();
      } catch (error) {
        proxyDiagnostic.value = String(error);
      } finally {
        diagnosticBusy.value = false;
      }
    }

    async function saveNetworkSettings() {
      const connection = useConnectionStore();
      const { bridge, persistSetting } = connection;
      const core = useCoreStore();
      const { busy, connected, message } = storeToRefs(core);
      const { t } = useI18nStore();
      if (busy.value || connected.value || !networkLoaded.value) return;
      networkError.value = "";
      busy.value = true;
      try {
        const settings: NetworkSettings = {
          proxyPort: validateProxyPort(draftProxyPort.value),
          allowLan: draftAllowLan.value,
          tun: {
            ...draftTun.value,
            mtu: validateTunMtu(draftTunMtu.value),
            routeExclude: parseRouteExclude(draftTunExclude.value),
          },
        };
        if (
          !(await confirmation.request({
            title: "confirmation.networkTitle",
            body: "confirmation.networkBody",
            confirmLabel: "settings.saveNetworkSettings",
            details: [
              `${t("settings.proxyPort")}: ${settings.proxyPort}`,
              ...(settings.allowLan && !networkSettings.value.allowLan ? ["confirmation.lanWarning"] : []),
              ...(!settings.tun.killSwitch && networkSettings.value.tun.killSwitch
                ? ["confirmation.guardWarning"]
                : []),
              ...(!settings.tun.strictRoute ||
              settings.tun.routeExclude.some(prefix => !networkSettings.value.tun.routeExclude.includes(prefix))
                ? ["confirmation.routeWarning"]
                : []),
            ],
          }))
        )
          return;
        applyNetworkSettings(await (await bridge()).saveNetworkSettings(settings));
        await persistSetting(networkSettingsStoreKey, networkSettings.value);
        message.value = "settings.networkSaved";
      } catch (error) {
        networkError.value = error instanceof Error ? error.message : String(error);
      } finally {
        busy.value = false;
      }
    }

    async function setNetworkSecurity(patch: Partial<NetworkSecurity>) {
      const { persistSetting } = useConnectionStore();
      const next = { ...networkSecurity.value, ...patch };
      await persistSetting(networkSecurityStoreKey, next);
      networkSecurity.value = next;
    }

    // Process-level network management: records an app's routing policy. inherit (follow the rules) is the default and is not persisted.
    async function setAppPolicy(app: string, policy: AppPolicy) {
      const connection = useConnectionStore();
      const { bridge, reconcileConnectionError } = connection;
      const { busy } = storeToRefs(useCoreStore());
      if (!app || busy.value || tunServiceBusy.value) throw new Error("native.connectionBusy");
      busy.value = true;
      try {
        const result = await (await bridge()).setApplicationPolicy(app, policy);
        appPolicies.value = result.policies;
        return result;
      } catch (error) {
        await reconcileConnectionError(error);
        throw error;
      } finally {
        busy.value = false;
      }
    }

    // Kill switch and IPv6 actually take effect through the existing TUN settings; not changeable while connected (consistent with the settings page).
    async function setTunProtection(patch: Partial<Pick<TunSettings, "killSwitch" | "ipv6">>) {
      const connection = useConnectionStore();
      const { bridge, persistSetting } = connection;
      const { busy, connected, connectionMode } = storeToRefs(useCoreStore());
      if (busy.value || connected.value || !networkLoaded.value || connectionMode.value !== "tun") return;
      busy.value = true;
      networkError.value = "";
      try {
        if (
          !(await confirmation.request({
            title: "confirmation.networkTitle",
            body: "confirmation.networkBody",
            details: patch.killSwitch === false ? ["confirmation.guardWarning"] : [],
          }))
        )
          return;
        const settings: NetworkSettings = {
          ...networkSettings.value,
          tun: { ...networkSettings.value.tun, ...patch },
        };
        applyNetworkSettings(await (await bridge()).saveNetworkSettings(settings));
        await persistSetting(networkSettingsStoreKey, networkSettings.value);
      } catch (error) {
        networkError.value = error instanceof Error ? error.message : String(error);
      } finally {
        busy.value = false;
      }
    }

    /** Explicit resync, unlike selecting the already-selected mode (which is a no-op). */
    async function repairNetwork() {
      const connection = useConnectionStore();
      const { bridge, applySnapshot, buildProfileConfig, reconcileConnectionError } = connection;
      const { busy, connected, connectionMode } = storeToRefs(useCoreStore());
      const { rawProfile } = storeToRefs(useSubscriptionsStore());
      const { logs } = storeToRefs(useLogsStore());
      if (busy.value || tunServiceBusy.value) return false;
      busy.value = true;
      try {
        if (
          !(await confirmation.request({
            title: "secnet.repairTitle",
            body: "check.reconnectConfirm",
          }))
        )
          return false;
        const client = await bridge();
        const reconnect = connected.value;
        applySnapshot(await client.disconnect());
        await client.setConnectionMode(connectionMode.value);
        if (rawProfile.value) await client.saveProfile(buildProfileConfig());
        if (reconnect) applySnapshot(await client.connect(connectionMode.value));
        logs.value = await client.logs();
        return true;
      } catch (error) {
        await reconcileConnectionError(error);
        return false;
      } finally {
        busy.value = false;
      }
    }

    async function toggleAutostart() {
      if (!autostartReady.value || autostartBusy.value) return;
      autostartBusy.value = true;
      autostartError.value = "";
      try {
        if (
          !(await confirmation.request({
            title: "confirmation.startupTitle",
            body: autostartEnabled.value ? "confirmation.startupDisable" : "confirmation.startupEnable",
          }))
        )
          return;
        const plugin = await import("@tauri-apps/plugin-autostart");
        if (autostartEnabled.value) await plugin.disable();
        else await plugin.enable();
        autostartEnabled.value = await plugin.isEnabled();
      } catch (error) {
        autostartError.value = String(error);
      } finally {
        autostartBusy.value = false;
      }
    }

    return {
      networkSettings,
      networkSecurity,
      appPolicies,
      draftProxyPort,
      draftAllowLan,
      draftTun,
      draftTunMtu,
      draftTunExclude,
      networkLoaded,
      networkError,
      showTraySpeed,
      autostartEnabled,
      autostartReady,
      autostartBusy,
      autostartError,
      proxyDiagnostic,
      diagnosticBusy,
      storedNetwork,
      sameNetwork,
      applyNetworkSettings,
      networkDirty,
      localProxyAddress,
      refreshProxyDiagnostics,
      saveNetworkSettings,
      setNetworkSecurity,
      setAppPolicy,
      setTunProtection,
      repairNetwork,
      toggleAutostart,
    };
  },
  { persist: { key: "airport.settings", pick: ["showTraySpeed"] } }
);
