import { defineStore, storeToRefs } from "pinia";
import { ref, watch } from "vue";
import { prepareConnection } from "../lib/connection-startup";
import { resolveProxyPort } from "../lib/port-conflict";
import { confirmation } from "../lib/confirmation";
import { restoreCrashConnection } from "../lib/crash-recovery";
import { createBridge } from "../lib/bridge";
import { type Snapshot, type ExternalProfile, type ChainGroup } from "../types";
import { notifications, loadNotificationPreference, prepareNotifications, dismissNotice } from "../lib/notifications";
import { ensureTunService, refreshTunService, tunServiceBusy } from "../lib/tun-service";
import { AUTOMATIC_NODE_ID, activeChain, configureSelection, outboundTag } from "../lib/automatic-selection";
import { preferencesKey } from "../lib/preferences";
import { PREFS_MIGRATED_KEY } from "../constants/storage";
import { loadExternalSubscriptions } from "../lib/subscriptions-store";
import {
  appPoliciesStoreKey,
  loadStoredPreferences,
  networkSecurityStoreKey,
  networkSettingsStoreKey,
  openSettingsStore,
  rememberStored,
  writeStored,
  readLegacyClientValue,
  removeLegacyClientValue,
  type SettingStore,
  type StoreStatus,
} from "../lib/settings-store";
import { defaultNetworkSecurity, parseNetworkSecurity } from "../lib/network-security";
import { liveAppPolicyRules, parseAppPolicies, type AppPolicy } from "../lib/app-policies";
import { parseProfileConfig, parseSingBoxProfile } from "../lib/profiles";
import { msg } from "../i18n";
import { builtinRouteRules } from "../lib/routing-rules";
import { ruleSetConfig } from "../lib/rule-sets";
import { isNative, currentPlatform } from "../lib/platform";
import { useCoreStore } from "./core";
import { useI18nStore } from "./i18n";
import { useNodesStore } from "./nodes";
import { useLogsStore } from "./logs";
import { useSubscriptionsStore } from "./subscriptions";
import { useSettingsStore } from "./settings";
import { useAccountStore, defaultServer } from "./account";
import { useProbeStore } from "./probe";
import { useChainStore } from "./chain";
import { useUiStore } from "./ui";
import { useWindowStore } from "./window";
import { useUpdateStore } from "./update";

// connection: app runtime core — connection bridge, connection control, profile building, preference caching, tray, notifications, and startup/teardown orchestration.
// This is the convergence point that all feature stores depend on (the sink of the DAG), so the required state is bound once at the top of setup and the method bodies are left as-is.
export const useConnectionStore = defineStore(
  "connection",
  () => {
    const core = useCoreStore();
    const {
      phase,
      message,
      busy,
      connectionAction,
      connectionProgress,
      guardBlocking,
      guardArmed,
      connectionMode,
      connected,
    } = storeToRefs(core);
    const { showError } = core;
    const i18n = useI18nStore();
    const { locale } = storeToRefs(i18n);
    const { t, tx } = i18n;
    const nodesStore = useNodesStore();
    const { nodes, selectedNode, nodeSort } = storeToRefs(nodesStore);
    const logsStore = useLogsStore();
    const { logs } = storeToRefs(logsStore);
    const { addActivity, logLine } = logsStore;
    const subs = useSubscriptionsStore();
    const { rawProfile, profileKind, activeExternalId, externalProfiles, profileLabel } = storeToRefs(subs);
    const { activateExternal, acceptDeepLinks } = subs;
    const settings = useSettingsStore();
    const {
      networkSettings,
      appPolicies,
      networkSecurity,
      networkLoaded,
      networkError,
      showTraySpeed,
      autostartEnabled,
      autostartReady,
      autostartError,
    } = storeToRefs(settings);
    const { applyNetworkSettings, storedNetwork, sameNetwork, refreshProxyDiagnostics } = settings;
    const account = useAccountStore();
    const { baseUrl, email, access, localMode } = storeToRefs(account);
    const { restoreSession } = account;
    const probe = useProbeStore();
    const { probeStates, probeSummary, probeConcurrency } = storeToRefs(probe);
    const chainStore = useChainStore();
    const { chain, chainGroups, chainActive, chainSummary } = storeToRefs(chainStore);
    const ui = useUiStore();
    const { tab, theme, selectedNodeName, label } = storeToRefs(ui);
    const windowStore = useWindowStore();
    const { watchWindowSize, loadRemembered, revealAfterBoot, startContextGuard, revealMain, syncWindowMode } =
      windowStore;
    const { installedVersion } = storeToRefs(useUpdateStore());

    const routeMode = ref<"rule" | "global">("rule");
    const chinaGeosite = ref(true);
    const adBlock = ref(false);

    let settingsStore: SettingStore | null = null;
    const preferenceStoreStatus: StoreStatus = { useStore: false };
    async function persistSetting(key: string, value: unknown) {
      await writeStored(settingsStore, key, value, preferenceStoreStatus);
    }

    // Core transport layer: desktop goes through Tauri IPC, preview uses an in-memory mock (with app-policy reads and log-line generation injected).
    const { bridge, browserBridge } = createBridge({
      appPolicies: () => appPolicies.value,
      logLine,
    });

    function applySnapshot(snapshot: Snapshot) {
      phase.value = snapshot.phase;
      guardBlocking.value = snapshot.blocked === true;
      guardArmed.value = snapshot.guarded === true;
      if (["CONNECTED", "IDLE", "FAILED"].includes(snapshot.phase)) connectionProgress.value = "";
      message.value = snapshot.phase === "CONNECTED" ? "status.ready" : snapshot.message;
      if (snapshot.logs) logs.value = snapshot.logs;
    }

    let stopListen: (() => void) | undefined;
    let stopProgressListen: (() => void) | undefined;
    let stopTrayListen: (() => void) | undefined;
    let stopDeepLink: (() => void) | undefined;
    let cacheReady = false;

    watch(
      message,
      value => {
        if (value) addActivity(value);
        if (!cacheReady) return;
        if (value && !["PREPARING", "CONNECTING", "RECONNECTING"].includes(phase.value))
          void notifications.show({
            body: tx(value),
            kind: phase.value === "FAILED" ? "error" : "info",
          });
      },
      { flush: "sync" }
    );
    watch(phase, (current, previous) => {
      if (!cacheReady || current === previous) return;
      // Abnormal disconnects (drop and reconnect) also fire a system notification: the wording is clearer when the kill switch is blocking the network; a manual disconnect → IDLE.
      const body =
        current === "CONNECTED"
          ? "notifications.proxyConnected"
          : current === "FAILED"
            ? "notifications.connectionFailed"
            : current === "RECONNECTING"
              ? guardBlocking.value
                ? "notifications.guardBlocking"
                : "notifications.connectionDropped"
              : current === "IDLE" && previous !== "IDLE"
                ? "notifications.proxyDisconnected"
                : "";
      if (body)
        void notifications.show({
          body: t(body),
          native: true,
          toast: false,
          kind: current === "CONNECTED" ? "success" : current === "IDLE" ? "info" : "error",
          key: `connection:${current}`,
        });
    });

    // Per-profile node selection / proxy chain / chain-group cache. Changed to reactive refs, automatically
    // persisted to localStorage by the persistedstate plugin (see persist.pick in defineStore at the end of the file).
    const selectedNodes = ref<Record<string, string>>({});
    const chains = ref<Record<string, string[]>>({});
    const chainGroupsByProfile = ref<Record<string, ChainGroup[]>>({});
    const profileCacheKey = () =>
      profileKind.value === "external" ? activeExternalId.value : `airport:${baseUrl.value}:${email.value}`;
    function cachedNode(candidates: typeof nodes.value) {
      const cached = selectedNodes.value[profileCacheKey()];
      if (cached === AUTOMATIC_NODE_ID && candidates.some(node => node.agentOnline)) return AUTOMATIC_NODE_ID;
      return (
        candidates.find(node => node.id === cached && node.agentOnline)?.id ||
        candidates.find(node => node.agentOnline)?.id ||
        ""
      );
    }
    // Restore the saved chain for this profile, dropping any hop that is no longer present.
    function cachedChain(candidates: typeof nodes.value) {
      const ids = new Set(candidates.map(node => node.id));
      return (chains.value[profileCacheKey()] || []).filter(id => ids.has(id));
    }
    function cachedChainGroups() {
      return (chainGroupsByProfile.value[profileCacheKey()] || []).map(group => ({
        ...group,
        nodes: [...group.nodes],
      }));
    }
    function dropSelectedNode(id: string) {
      delete selectedNodes.value[id];
    }
    // Write the current profile's selection / chain / chain groups into the per-profile cache maps;
    // the other scalar preferences (theme/locale/routeMode…) are cached directly by each store's persist, so there's no need to aggregate them here.
    // The actual persistence is done automatically by the persistedstate plugin deep-watching these refs.
    function persistPreferences() {
      if (!cacheReady) return;
      const key = profileCacheKey();
      if (selectedNode.value) selectedNodes.value[key] = selectedNode.value;
      chains.value[key] = chain.value;
      chainGroupsByProfile.value[key] = chainGroups.value;
    }
    watch([selectedNode, chain, chainGroups], persistPreferences, { deep: true });

    // Native menu items cannot look copy up themselves, so their text is sent from the dictionary.
    const trayLabels = {
      open: "tray.open",
      system: "tray.system",
      tun: "tray.tun",
      "route-title": "tray.routeTitle",
      global: "tray.global",
      rule: "tray.rule",
      profile: "tray.profile",
      "node-list": "tray.nodeList",
      connect: "tray.connect",
      disconnect: "tray.disconnect",
      more: "tray.more",
      sub: "tray.subscriptions",
      settings: "tray.settings",
      help: "tray.help",
      quit: "tray.quit",
      connected: "tray.connected",
      failed: "tray.failed",
      idle: "tray.idle",
    } as const;
    async function syncTray() {
      if (!isNative) return;
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("sync_tray", {
          snapshot: {
            showTraySpeed: showTraySpeed.value,
            labels: Object.fromEntries(Object.entries(trayLabels).map(([id, key]) => [id, t(key)])),
            mode: connectionMode.value,
            route: routeMode.value,
            node: tx(selectedNodeName.value),
            selected: selectedNode.value,
            profile: tx(profileLabel.value),
            nodes: [
              ...(nodes.value.length
                ? [
                    {
                      id: AUTOMATIC_NODE_ID,
                      name: t("nodes.automatic"),
                      enabled: nodes.value.some(node => node.agentOnline),
                    },
                  ]
                : []),
              ...nodes.value.map(node => ({
                id: node.id,
                name: node.name,
                enabled: node.agentOnline,
              })),
            ],
            ready: !!rawProfile.value && !!selectedNode.value,
            busy: busy.value,
          },
        });
      } catch (error) {
        addActivity(msg("log.trayUpdateFailed", { error: String(error) }));
      }
    }
    watch(
      [
        phase,
        busy,
        connectionMode,
        routeMode,
        selectedNodeName,
        rawProfile,
        locale,
        profileLabel,
        nodes,
        showTraySpeed,
      ],
      syncTray
    );
    async function trayAction(action: string) {
      if (busy.value) return;
      if (action === "toggle-connection" && rawProfile.value && selectedNode.value) await toggle();
      else if (action === "toggle-route") await setRouteMode(routeMode.value === "rule" ? "global" : "rule");
      else if (action === "connect" && !connected.value && rawProfile.value) await toggle();
      else if (action === "disconnect" && connected.value) await toggle();
      else if (["system", "manual", "tun"].includes(action))
        await setConnectionMode(action === connectionMode.value ? "manual" : (action as "system" | "manual" | "tun"));
      else if (action.startsWith("select-node:")) await selectNode(action.slice("select-node:".length));
      else if (action === "rule" || action === "global") await setRouteMode(action);
      else if (action === "nodes" || action === "settings" || action === "sub" || action === "help") tab.value = action;
      await syncTray();
    }

    // Wire the active chain into [cfg] so each hop dials through the previous one via sing-box detour.
    function applyChain(cfg: any, external: boolean): string | null {
      if (chain.value.length < 2) return null;
      const tags = chain.value.map(id => outboundTag(id, external));
      const byTag = new Map<string, any>((cfg.outbounds ?? []).map((o: any) => [o.tag, o]));
      if (!tags.every(tag => byTag.has(tag))) return null;
      for (let i = 1; i < tags.length; i++) byTag.get(tags[i]).detour = tags[i - 1];
      return chain.value[chain.value.length - 1];
    }

    function buildProfileConfig() {
      const cfg: any = parseProfileConfig(rawProfile.value, profileKind.value === "external");
      // External profiles may expose a local Clash API or other experimental listener.
      delete cfg.experimental;
      delete cfg.services;
      const external = profileKind.value === "external";
      if (chain.value.length && activeChain(chain.value, selectedNode.value).length === 0) chain.value = [];
      const chainExit = applyChain(cfg, external);
      configureSelection(cfg, chainExit ?? selectedNode.value, nodes.value, external);
      cfg.route ??= {};
      cfg.route.final = "select";
      const ruleToggles = {
        chinaGeosite: chinaGeosite.value,
        adBlock: adBlock.value,
      };
      cfg.route.rules =
        routeMode.value === "rule"
          ? [
              ...builtinRouteRules(true, ruleToggles),
              ...(profileKind.value === "external" && Array.isArray(cfg.route.rules) ? cfg.route.rules : []),
            ]
          : [];
      // Process-level policy: inject per-app process rules at the very front, taking priority over the default and China-direct rules.
      cfg.route.rules = [...liveAppPolicyRules(appPolicies.value), ...cfg.route.rules];
      const ruleDefs = routeMode.value === "rule" ? ruleSetConfig(ruleToggles).defs : [];
      if (ruleDefs.length) cfg.route.rule_set = ruleDefs;
      else delete cfg.route.rule_set;
      cfg.inbounds = [
        {
          type: "mixed",
          tag: "mixed-in",
          listen: networkSettings.value.allowLan ? "0.0.0.0" : "127.0.0.1",
          listen_port: networkSettings.value.proxyPort,
        },
      ];
      return JSON.stringify(cfg);
    }

    async function reconcileConnectionError(error: unknown) {
      try {
        applySnapshot(await (await bridge()).state());
      } catch (stateError) {
        addActivity(msg("log.stateReadFailed", { error: String(stateError) }));
      }
      showError(error instanceof Error ? error.message : String(error));
    }

    // Rebuild the profile from the current selection/chain and push it to the core.
    async function reapplyProfile() {
      const next = buildProfileConfig();
      const client = await bridge();
      const wasConnected = connected.value;
      if (wasConnected) applySnapshot(await client.disconnect());
      await client.saveProfile(next);
      if (wasConnected) applySnapshot(await client.connect(connectionMode.value));
      logs.value = await client.logs();
    }

    // Flip the exit node on the already-running core via the Clash API selector, keeping the
    // connection alive (existing flows continue, new ones use the new node). Eligible only when
    // connected to a concrete node with no active multi-hop chain — switching to the auto URLTest
    // group or changing a chain alters outbound topology, which the selector default cannot express.
    // Returns true on success; callers fall back to a full re-apply (restart) otherwise.
    async function tryHotSwitchNode(): Promise<boolean> {
      if (!connected.value || selectedNode.value === AUTOMATIC_NODE_ID || chainActive.value) return false;
      const client = await bridge();
      const external = profileKind.value === "external";
      await client.selectNode(outboundTag(selectedNode.value, external));
      // Mirror the choice to the on-disk config so a core restart (crash recovery) keeps this node.
      // Fire-and-forget: the live switch already took effect, so this must not block the UI.
      void client.saveProfile(buildProfileConfig()).catch(() => {});
      return true;
    }

    async function applySelectedNode() {
      if (!rawProfile.value || !selectedNode.value) {
        message.value = "subscriptions.importFirst";
        return;
      }
      const node =
        selectedNode.value === AUTOMATIC_NODE_ID
          ? { name: t("nodes.automatic") }
          : nodes.value.find(n => n.id === selectedNode.value);
      if (!node) return;
      try {
        let hotSwitched = false;
        try {
          hotSwitched = await tryHotSwitchNode();
        } catch {
          // Hot-switch unavailable or rejected (core busy, unknown tag): fall back to a full restart.
          hotSwitched = false;
        }
        if (!hotSwitched) await reapplyProfile();
        message.value = msg("nodes.switchedTo", { name: node.name });
        addActivity(msg("log.nodeSwitched", { name: node.name }));
      } catch (error) {
        await reconcileConnectionError(error);
      }
    }

    // Replace the proxy chain (ordered entry → exit) and apply it live.
    async function commitChain(ids: string[]) {
      if (ids.length >= 2) selectedNode.value = ids[ids.length - 1];
      chain.value = [...ids];
      if (!rawProfile.value) return;
      if (!chainActive.value && !selectedNode.value) return;
      try {
        await reapplyProfile();
        message.value = chainActive.value ? "nodes.chainApplied" : "nodes.chainCleared";
        addActivity(chainActive.value ? msg("log.chainApplied", { chain: chainSummary.value }) : "log.chainCleared");
      } catch (error) {
        await reconcileConnectionError(error);
      }
    }

    async function updateMode() {
      const client = await bridge();
      if (!rawProfile.value) {
        await client.setConnectionMode(connectionMode.value);
        return;
      }
      const wasConnected = connected.value;
      if (wasConnected) applySnapshot(await client.disconnect());
      await client.setConnectionMode(connectionMode.value);
      await client.saveProfile(buildProfileConfig());
      if (wasConnected) applySnapshot(await client.connect(connectionMode.value));
      logs.value = await client.logs();
    }
    async function setConnectionMode(value: "system" | "tun" | "manual") {
      if (connectionMode.value === value || busy.value || tunServiceBusy.value) return;
      busy.value = true;
      const previous = connectionMode.value;
      const reconnect = connected.value;
      try {
        if (value === "tun") {
          if (connected.value) applySnapshot(await (await bridge()).disconnect());
          await ensureTunService(locale.value);
        }
        connectionMode.value = value;
        await updateMode();
        if (value === "tun" && reconnect) applySnapshot(await (await bridge()).connect(value));
        addActivity(
          msg("log.connectionMode", {
            mode: value === "tun" ? "TUN" : msg(value === "manual" ? "settings.manualProxy" : "common.systemProxy"),
          })
        );
      } catch (error) {
        connectionMode.value = previous;
        await reconcileConnectionError(error);
      } finally {
        busy.value = false;
      }
    }
    async function setRouteMode(value: "rule" | "global") {
      if (routeMode.value === value || busy.value) return;
      busy.value = true;
      routeMode.value = value;
      try {
        await updateMode();
        addActivity(
          msg("log.routeMode", {
            mode: msg(value === "rule" ? "common.rule" : "overview.global"),
          })
        );
      } catch (error) {
        await reconcileConnectionError(error);
      } finally {
        busy.value = false;
      }
    }
    // Toggle a rule-set switch: persistence is handled by the watch; if already connected, reapply the config so it takes effect immediately.
    async function setRuleToggle(key: "chinaGeosite" | "adBlock", value: boolean) {
      const flag = key === "chinaGeosite" ? chinaGeosite : adBlock;
      if (flag.value === value || busy.value) return;
      busy.value = true;
      flag.value = value;
      try {
        if (connected.value) await reapplyProfile();
      } catch (error) {
        await reconcileConnectionError(error);
      } finally {
        busy.value = false;
      }
    }

    async function selectNode(id: string) {
      if (busy.value) return;
      if (
        id === AUTOMATIC_NODE_ID
          ? !nodes.value.some(node => node.agentOnline)
          : !nodes.value.find(node => node.id === id)?.agentOnline
      ) {
        message.value = "nodes.notReady";
        return;
      }
      busy.value = true;
      try {
        selectedNode.value = id;
        if (chain.value.length) chain.value = [];
        await applySelectedNode();
      } finally {
        busy.value = false;
      }
    }

    // Lifting the kill switch is disconnecting: the user chooses to be without the tunnel.
    async function releaseGuard() {
      if (busy.value) return;
      busy.value = true;
      try {
        applySnapshot(await (await bridge()).disconnect());
        dismissNotice();
      } catch (error) {
        showError(error instanceof Error ? error.message : String(error));
      } finally {
        busy.value = false;
      }
    }

    async function toggle() {
      if (busy.value || tunServiceBusy.value) return;
      const disconnecting = connected.value;
      busy.value = true;
      connectionAction.value = disconnecting ? "disconnect" : "connect";
      connectionProgress.value = disconnecting ? "status.disconnecting" : "startup.preparing";
      dismissNotice();
      try {
        const apiBridge = await bridge();
        if (!disconnecting) {
          // Only profile/service readiness gates connecting. Notification consent must not.
          await prepareConnection({
            prepareNotifications,
            ensureTunnel: () => (connectionMode.value === "tun" ? ensureTunService(locale.value) : Promise.resolve()),
            saveProfile: () => apiBridge.saveProfile(buildProfileConfig()),
          });
          if (
            !(await resolveProxyPort({
              inspect: () => apiBridge.inspectProxyPort(),
              release: token => apiBridge.releaseProxyPort(token),
              confirm: async conflict => {
                await revealMain();
                return confirmation.request({
                  title: "confirmation.portTitle",
                  body: msg("confirmation.portBody", { port: conflict.port }),
                  confirmLabel: "confirmation.portRelease",
                  danger: true,
                  details: conflict.listeners.map(process => `${process.name} · PID ${process.pid}`),
                });
              },
            }))
          )
            return;
        }
        const snapshot = disconnecting ? await apiBridge.disconnect() : await apiBridge.connect(connectionMode.value);
        applySnapshot(snapshot);
        addActivity(msg("log.connectionState", { state: msg(label.value) }));
        logs.value = snapshot.logs || (await apiBridge.logs());
      } catch (error) {
        const detail = error instanceof Error ? error.message : String(error);
        try {
          applySnapshot(await (await bridge()).state());
        } catch {
          /* Keep the original connection error visible. */
        }
        showError(detail);
      } finally {
        connectionAction.value = null;
        connectionProgress.value = "";
        busy.value = false;
        void refreshProxyDiagnostics();
      }
    }

    // One-time migration: older versions stored preferences in the native store; on the first upgrade, seed them into each store
    // (persistedstate then writes them to localStorage), so existing users don't lose theme / language / selection / chain settings.
    // hadStoredPrefs marks whether this is a returning user: it decides whether the first launch auto-activates the first subscription (matching the old logic).
    let hadStoredPrefs = false;
    async function migrateLegacyPreferences() {
      let migrated = false;
      try {
        const marker = await settingsStore?.get(PREFS_MIGRATED_KEY);
        migrated = marker === true || marker === "1";
      } catch {
        /* Read failure: treat as not migrated; at worst we idempotently seed again. */
      }
      if (migrated) {
        hadStoredPrefs = true;
        return;
      }
      const savedPrefs = await loadStoredPreferences(
        settingsStore,
        () => readLegacyClientValue(preferencesKey),
        preferenceStoreStatus,
        () => removeLegacyClientValue(preferencesKey)
      );
      if (savedPrefs) {
        baseUrl.value = savedPrefs.baseUrl;
        email.value = savedPrefs.email;
        connectionMode.value = savedPrefs.connectionMode;
        routeMode.value = savedPrefs.routeMode;
        chinaGeosite.value = savedPrefs.chinaGeosite;
        adBlock.value = savedPrefs.adBlock;
        selectedNodes.value = savedPrefs.selectedNodes;
        chains.value = savedPrefs.chains;
        chainGroupsByProfile.value = savedPrefs.chainGroups;
        nodeSort.value = savedPrefs.nodeSort;
        probeConcurrency.value = savedPrefs.probeConcurrency;
        locale.value = savedPrefs.locale;
        theme.value = savedPrefs.theme;
        showTraySpeed.value = savedPrefs.showTraySpeed;
        activeExternalId.value = savedPrefs.activeProfileId;
      }
      hadStoredPrefs = !!savedPrefs;
      try {
        await settingsStore?.set(PREFS_MIGRATED_KEY, true);
        await settingsStore?.save();
      } catch {
        /* Failing to write the marker is not fatal; it will be retried next time (idempotent). */
      }
    }

    const bootstrap = async () => {
      startContextGuard();
      const apiBridge = await bridge();
      // Register the core's state/progress push channels first, each guarded, so a failure in any
      // later init step cannot leave the UI without live state updates — it would otherwise freeze
      // on a stale "connected"/intermediate phase and never reflect drops, reconnects or recovery.
      try {
        if (apiBridge.onState) stopListen = await apiBridge.onState(applySnapshot);
      } catch (error) {
        addActivity(msg("log.stateReadFailed", { error: String(error) }));
      }
      try {
        if (apiBridge.onProgress)
          stopProgressListen = await apiBridge.onProgress(stage => {
            connectionProgress.value = stage;
          });
      } catch {
        /* The progress bar is cosmetic; its listener failing must not abort bootstrap. */
      }
      void watchWindowSize().catch(() => {
        /* The window still works without size memory. */
      });
      settingsStore = await openSettingsStore();
      try {
        await migrateLegacyPreferences();
        await loadRemembered();
        await loadNotificationPreference();
      } catch (error) {
        message.value = core.failure(error, "error.preferencesNotSaved");
      }
      // A server address specified at build time always takes precedence over the cached value.
      if (defaultServer) baseUrl.value = defaultServer;
      // The tray starts with fallback text; give it the real labels as soon as the language is known.
      void syncTray();
      // These reads don't share the preference file, so they overlap the settings chain below.
      // Settings themselves stay in order: parallel saves of that file can drop a key.
      await Promise.all([
        (async () => {
          if (!isNative) return;
          try {
            installedVersion.value = await (await import("@tauri-apps/api/app")).getVersion();
          } catch {
            /* The app version remains visible from the bundle metadata fallback. */
          }
        })(),
        (async () => {
          if (isNative) {
            try {
              autostartEnabled.value = await (await import("@tauri-apps/plugin-autostart")).isEnabled();
              autostartReady.value = true;
            } catch (error) {
              autostartError.value = String(error);
            }
          } else {
            autostartError.value = "settings.autostartDesktopOnly";
          }
        })(),
        (async () => {
          try {
            applySnapshot(await apiBridge.state());
            logs.value = await apiBridge.logs();
          } catch (error) {
            addActivity(msg("log.stateReadFailed", { error: String(error) }));
          }
        })(),
        (async () => {
          try {
            const nativeNetwork = await apiBridge.loadNetworkSettings();
            const chosen = await rememberStored(
              settingsStore,
              networkSettingsStoreKey,
              nativeNetwork,
              storedNetwork,
              preferenceStoreStatus
            );
            applyNetworkSettings(
              sameNetwork(chosen, nativeNetwork) ? chosen : await apiBridge.saveNetworkSettings(chosen)
            );
            networkLoaded.value = true;
          } catch (error) {
            networkError.value = String(error);
          }
          try {
            networkSecurity.value = await rememberStored(
              settingsStore,
              networkSecurityStoreKey,
              defaultNetworkSecurity(),
              parseNetworkSecurity,
              preferenceStoreStatus
            );
            appPolicies.value = await rememberStored(
              settingsStore,
              appPoliciesStoreKey,
              {} as Record<string, AppPolicy>,
              parseAppPolicies,
              preferenceStoreStatus
            );
          } catch (error) {
            // Fall back to the in-memory defaults; a stored-preference read failure must not abort boot.
            message.value = core.failure(error, "error.preferencesNotSaved");
          }
          try {
            const nativeList = await loadExternalSubscriptions(apiBridge, settingsStore, preferenceStoreStatus);
            const profiles = nativeList.filter(
              (item: any): item is ExternalProfile =>
                typeof item?.id === "string" &&
                typeof item?.name === "string" &&
                typeof item?.url === "string" &&
                typeof item?.raw === "string" &&
                typeof item?.updatedAt === "string"
            );
            externalProfiles.value = profiles;
            if (JSON.stringify(profiles) !== JSON.stringify(nativeList))
              await apiBridge.saveSubscriptions(JSON.stringify(profiles));
          } catch (error) {
            message.value = msg("error.subscriptionListFailed", {
              error: String(error),
            });
          }
        })(),
      ]);
      try {
        // Migrate an existing installation with saved subscriptions once; explicit logout stays logged out.
        const profile = hadStoredPrefs
          ? externalProfiles.value.find(item => item.id === activeExternalId.value)
          : externalProfiles.value[0];
        if (profile) {
          await activateExternal(profile);
          message.value = "subscriptions.restored";
        }
        await apiBridge.setConnectionMode(connectionMode.value);
      } catch (error) {
        message.value = msg("error.restoreFailed", { error: String(error) });
      }
      try {
        await restoreSession();
      } catch (error) {
        addActivity(msg("log.stateReadFailed", { error: String(error) }));
      }
      try {
        await revealAfterBoot();
      } catch {
        /* Revealing the window on boot is best-effort. */
      }
      void refreshProxyDiagnostics();
      cacheReady = true;
      persistPreferences();
      addActivity("log.clientStarted");
      // onState / onProgress are registered at the top of bootstrap (above), so they survive any
      // failure in the init steps in between.
      if (isNative) {
        // Restoring the tunnel should not wait on tray labels or scheme registration.
        const recovery = (async () => {
          // Held across the lease wait, so a tray click cannot start a second tunnel beside recovery.
          busy.value = true;
          try {
            const { invoke } = await import("@tauri-apps/api/core");
            await restoreCrashConnection({
              requested: () => invoke<boolean>("take_crash_recovery"),
              canConnect: () => Boolean(rawProfile.value && selectedNode.value && !connected.value && !busy.value),
              tun: connectionMode.value === "tun",
              tunReady: async () => (await refreshTunService()).ready,
              connect: async () => {
                busy.value = true;
                try {
                  await apiBridge.saveProfile(buildProfileConfig());
                  applySnapshot(await apiBridge.connect(connectionMode.value));
                  addActivity(msg("log.connectionState", { state: msg(label.value) }));
                } finally {
                  busy.value = false;
                }
              },
              report: key => {
                message.value = key;
                addActivity(key);
              },
            });
          } catch (error) {
            await reconcileConnectionError(error);
          } finally {
            busy.value = false;
          }
        })();
        const shell = (async () => {
          const { listen } = await import("@tauri-apps/api/event");
          stopTrayListen = await listen<string>("tray://action", event => {
            void trayAction(event.payload);
          });
          const { getCurrent, onOpenUrl, register } = await import("@tauri-apps/plugin-deep-link");
          if (currentPlatform === "windows" || currentPlatform === "linux")
            for (const scheme of ["clashnova", "clash", "clashmeta"]) await register(scheme).catch(() => {});
          stopDeepLink = await onOpenUrl(urls => {
            void acceptDeepLinks(urls);
          });
          void acceptDeepLinks(await getCurrent());
          await syncTray();
        })();
        await Promise.all([recovery, shell]);
      }
    };

    const teardown = () => {
      windowStore.teardown();
      logsStore.teardown();
      dismissNotice();
      ui.teardown();
      probe.teardown();
      stopListen?.();
      stopProgressListen?.();
      stopTrayListen?.();
      stopDeepLink?.();
    };

    return {
      routeMode,
      chinaGeosite,
      adBlock,
      // Returned so persistedstate can persist them; the aggregate facade does not forward them (components don't use them directly).
      selectedNodes,
      chains,
      chainGroupsByProfile,
      bridge,
      applySnapshot,
      persistSetting,
      cachedNode,
      cachedChain,
      cachedChainGroups,
      dropSelectedNode,
      persistPreferences,
      applyChain,
      buildProfileConfig,
      reconcileConnectionError,
      commitChain,
      setConnectionMode,
      setRouteMode,
      setRuleToggle,
      selectNode,
      releaseGuard,
      toggle,
      bootstrap,
      teardown,
    };
  },
  {
    persist: {
      key: "airport.connection",
      pick: ["routeMode", "chinaGeosite", "adBlock", "selectedNodes", "chains", "chainGroupsByProfile"],
    },
  }
);
