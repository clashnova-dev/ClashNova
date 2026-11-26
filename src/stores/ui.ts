import { defineStore, storeToRefs } from "pinia";
import { ref, computed, watch, nextTick, type Component } from "vue";
import { House, Layers, Rss, Network, ScrollText, Settings, Radar, Route, Share2 } from "@lucide/vue";
import { type MessageKey } from "../i18n";
import { regionCode } from "../lib/regions";
import { type SubscriptionUsage } from "../lib/subscription-info";
import { AUTOMATIC_NODE_ID, AUTOMATIC_OUTBOUND_TAG, outboundTag } from "../lib/automatic-selection";
import { confirmation } from "../lib/confirmation";
import { useI18nStore } from "./i18n";
import { useCoreStore } from "./core";
import { useNodesStore } from "./nodes";
import { useSubscriptionsStore } from "./subscriptions";
import { useProbeStore } from "./probe";
import { useLogsStore } from "./logs";
import { type Tone } from "../types";

// ui: interface view model — derived display state such as theme / current tab / navigation / title / status bar / usage / duration.
export const useUiStore = defineStore(
  "ui",
  () => {
    const i18n = useI18nStore();
    const { locale } = storeToRefs(i18n);
    const { t, tx } = i18n;
    const core = useCoreStore();
    const { phase, connectionAction, connected } = storeToRefs(core);
    const { nodes, selectedNode, latencies } = storeToRefs(useNodesStore());
    const subs = useSubscriptionsStore();
    const { activeSub, profileKind } = storeToRefs(subs);
    const probe = useProbeStore();
    const { automaticSelection } = storeToRefs(probe);
    const logsStore = useLogsStore();
    const { allLogs } = storeToRefs(logsStore);
    const { scrollLogs, setLogsActive } = logsStore;

    const theme = ref<"system" | "light" | "dark">("system");
    const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
    function applyTheme() {
      document.documentElement.dataset.theme =
        theme.value === "system" ? (colorScheme.matches ? "dark" : "light") : theme.value;
      document.documentElement.lang = locale.value;
    }
    watch([theme, locale], applyTheme, { immediate: true });
    colorScheme.addEventListener("change", applyTheme);

    const tab = ref<
      "home" | "nodes" | "sub" | "connections" | "rules" | "logs" | "network" | "lan" | "settings" | "help"
    >("home");
    const mainContent = ref<HTMLElement | null>(null);
    watch(tab, async () => {
      await setLogsActive(tab.value === "logs");
      await nextTick();
      mainContent.value?.scrollTo({ top: 0 });
    });
    watch(tab, () => confirmation.cancel(), { flush: "sync" });
    watch(allLogs, () => {
      if (tab.value === "logs") void scrollLogs();
    });

    const navigation = computed(() => {
      const items: {
        key: "home" | "nodes" | "sub" | "connections" | "rules" | "logs" | "network" | "lan" | "settings" | "help";
        label: MessageKey;
        icon: Component;
      }[] = [
        { key: "home", label: "nav.overview", icon: House },
        { key: "nodes", label: "nav.nodes", icon: Layers },
        { key: "sub", label: "nav.subscriptions", icon: Rss },
        { key: "connections", label: "nav.connections", icon: Network },
        { key: "rules", label: "nav.rules", icon: Route },
        { key: "logs", label: "nav.logs", icon: ScrollText },
        { key: "network", label: "nav.networkCheck", icon: Radar },
        { key: "lan", label: "nav.lanTransfer", icon: Share2 },
        { key: "settings", label: "nav.settings", icon: Settings },
      ];
      return items;
    });
    const title = computed<MessageKey>(
      () =>
        (
          ({
            home: "nav.overview",
            nodes: "nav.nodes",
            sub: "nav.subscriptions",
            connections: "nav.connections",
            rules: "nav.rules",
            logs: "nav.logs",
            network: "nav.networkCheck",
            lan: "nav.lanTransfer",
            settings: "nav.settings",
            help: "nav.help",
          }) as const
        )[tab.value]
    );
    const label = computed<MessageKey>(
      () =>
        (
          ({
            IDLE: "status.idle",
            PREPARING: "status.preparing",
            CONNECTING: "status.connecting",
            CONNECTED: "status.connected",
            RECONNECTING: "status.reconnecting",
            DISCONNECTING: "status.disconnecting",
            FAILED: "status.failed",
          }) as Record<string, MessageKey>
        )[phase.value] || "status.unknown"
    );

    function fmt(n: number) {
      if (n < 1024) return `${n} B`;
      if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KiB`;
      if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MiB`;
      return `${(n / 1024 ** 3).toFixed(2)} GiB`;
    }
    const trafficLabel = computed(() => {
      const s = activeSub.value;
      if (!s) return "—";
      return `${fmt(s.usedChargedBytes)} / ${fmt(s.limitBytes)}`;
    });
    const airportUsage = computed<SubscriptionUsage | undefined>(() =>
      activeSub.value
        ? {
            upload: 0,
            download: activeSub.value.usedChargedBytes,
            total: activeSub.value.limitBytes,
            expire: activeSub.value.expiresAt
              ? Math.floor(new Date(activeSub.value.expiresAt).getTime() / 1000)
              : undefined,
          }
        : undefined
    );
    const usagePercent = computed(() =>
      activeSub.value?.limitBytes
        ? Math.min(100, Math.round((activeSub.value.usedChargedBytes / activeSub.value.limitBytes) * 100))
        : 0
    );
    const remainingTraffic = computed(() =>
      activeSub.value ? fmt(Math.max(0, activeSub.value.limitBytes - activeSub.value.usedChargedBytes)) : "—"
    );

    const automaticNodeName = computed(
      () =>
        nodes.value.find(
          node => outboundTag(node.id, profileKind.value === "external") === automaticSelection.value?.tag
        )?.name || ""
    );
    const selectedNodeName = computed(() =>
      selectedNode.value === AUTOMATIC_NODE_ID
        ? `${t("nodes.automatic")}${automaticNodeName.value ? " · " + automaticNodeName.value : ""}`
        : nodes.value.find(node => node.id === selectedNode.value)?.name || "nodes.notSelected"
    );
    const selectedNodeInfo = computed(() => {
      const external = profileKind.value === "external";
      const id =
        selectedNode.value === AUTOMATIC_NODE_ID
          ? nodes.value.find(node => outboundTag(node.id, external) === automaticSelection.value?.tag)?.id
          : selectedNode.value;
      return nodes.value.find(node => node.id === id);
    });
    const selectedRegionLabel = computed(() => {
      const code = selectedNodeInfo.value ? regionCode(selectedNodeInfo.value.region, selectedNodeInfo.value.name) : "";
      return code || (selectedNode.value === AUTOMATIC_NODE_ID ? "AUTO" : "··");
    });
    const selectedLatencyText = computed(() => {
      if (!selectedNode.value) return "";
      if (selectedNode.value === AUTOMATIC_NODE_ID)
        return connected.value && automaticSelection.value?.delay ? `${automaticSelection.value.delay} ms` : "";
      const value = latencies.value[selectedNode.value];
      return value === undefined ? "" : `${value} ms`;
    });
    const phaseTone = computed<Tone>(() =>
      connectionAction.value
        ? "fair"
        : phase.value === "CONNECTED"
          ? "good"
          : phase.value === "FAILED"
            ? "bad"
            : phase.value === "IDLE"
              ? "idle"
              : "fair"
    );
    // The header chip replaces the old bottom status bar: state, node and latency.
    const statusChipText = computed(() =>
      connectionAction.value
        ? t(connectionAction.value === "connect" ? "status.connecting" : "status.disconnecting")
        : [
            t(label.value),
            connected.value && selectedNode.value ? tx(selectedNodeName.value) : "",
            connected.value ? selectedLatencyText.value : "",
          ]
            .filter(Boolean)
            .join(" · ")
    );
    const connectionNodeNames = computed(() =>
      Object.fromEntries([
        ["select", t("connections.selector")],
        ["direct", t("connections.direct")],
        [AUTOMATIC_OUTBOUND_TAG, t("nodes.automatic")],
        ...nodes.value.map(node => [
          profileKind.value === "external" ? node.id : `node-${node.id.replaceAll("-", "").slice(0, 8)}`,
          node.name,
        ]),
      ])
    );

    // Time since this window first saw the connection up; resets on disconnect.
    const connectedSince = ref<number | null>(null);
    const clock = ref(Date.now());
    let clockTimer: ReturnType<typeof setInterval> | undefined;
    watch(
      connected,
      value => {
        clearInterval(clockTimer);
        if (value) {
          connectedSince.value ??= Date.now();
          clock.value = Date.now();
          clockTimer = setInterval(() => {
            clock.value = Date.now();
          }, 1000);
        } else connectedSince.value = null;
      },
      { immediate: true }
    );
    const connectionDuration = computed(() => {
      if (connectedSince.value === null) return "";
      const total = Math.max(0, Math.floor((clock.value - connectedSince.value) / 1000));
      const pad = (n: number) => String(n).padStart(2, "0");
      return `${pad(Math.floor(total / 3600))}:${pad(Math.floor(total / 60) % 60)}:${pad(total % 60)}`;
    });

    async function openExternal(url: string) {
      const { showError, failure } = useCoreStore();
      if (!url) return;
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("open_external", { url });
      } catch (error) {
        showError(failure(error, "native.openFailed"));
      }
    }

    function teardown() {
      clearInterval(clockTimer);
      colorScheme.removeEventListener("change", applyTheme);
    }

    return {
      theme,
      colorScheme,
      applyTheme,
      tab,
      mainContent,
      navigation,
      title,
      label,
      fmt,
      trafficLabel,
      airportUsage,
      usagePercent,
      remainingTraffic,
      automaticNodeName,
      selectedNodeName,
      selectedNodeInfo,
      selectedRegionLabel,
      selectedLatencyText,
      phaseTone,
      statusChipText,
      connectionNodeNames,
      connectionDuration,
      openExternal,
      teardown,
    };
  },
  { persist: { key: "airport.ui", pick: ["theme"] } }
);
