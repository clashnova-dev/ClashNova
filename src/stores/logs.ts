import { defineStore, storeToRefs } from "pinia";
import { ref, computed, nextTick } from "vue";
import { encode, type Text } from "../i18n";
import { isNative } from "../lib/platform";
import { useI18nStore } from "./i18n";
import { useConnectionStore } from "./connection";
import { useUiStore } from "./ui";

// logs: log data and display (core/activity logs, rendering, scrolling, appending).
// Pulling from the backend (pollLogs/refreshLogs) depends on the connection bridge; orchestration stays in useApp for now and fills in the logs here.
export const useLogsStore = defineStore("logs", () => {
  const i18n = useI18nStore();
  const logs = ref<string[]>([]);
  const activityLogs = ref<string[]>([]);
  const logViewport = ref<HTMLElement | null>(null);
  const followLogs = ref(true);
  const logsLoading = ref(false);
  const logsError = ref("");
  const allLogs = computed(() => [...activityLogs.value, ...logs.value].sort());
  // Log lines are stored language-neutral (after the time + level comes a key or encoded message) and rendered here in the current language.
  const logLinePattern = /^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\s+(?:\[[A-Z]+\]\s+)?)([\s\S]*)$/;
  const renderLogLine = (line: string) => {
    const parts = logLinePattern.exec(line);
    return parts ? parts[1] + i18n.tx(parts[2]) : i18n.tx(line);
  };
  const logsText = computed(() =>
    allLogs.value.length ? allLogs.value.map(renderLogLine).join("\n") : i18n.t("logs.empty")
  );
  function logLine(value: string) {
    return `${new Date().toLocaleString("sv-SE", { hour12: false })}  [INFO] ${value}`;
  }
  function addActivity(text: Text) {
    const value = encode(text);
    const line = value.length > 4096 ? "native.logEntryTooLong" : value;
    activityLogs.value = [...activityLogs.value.slice(-99), logLine(line)];
    if (isNative && line === value) void import("@tauri-apps/plugin-log").then(({ info }) => info(line));
  }
  function onLogScroll() {
    const el = logViewport.value;
    if (el) followLogs.value = el.scrollHeight - el.clientHeight - el.scrollTop < 32;
  }
  async function scrollLogs() {
    await nextTick();
    if (followLogs.value && logViewport.value) logViewport.value.scrollTop = logViewport.value.scrollHeight;
  }

  let logsTimer: ReturnType<typeof setTimeout> | undefined;
  let logsGeneration = 0;
  async function refreshLogs() {
    const { bridge } = useConnectionStore();
    if (logsLoading.value) return;
    logsLoading.value = true;
    try {
      logs.value = await (await bridge()).logs();
      logsError.value = "";
    } catch (error) {
      logsError.value = error instanceof Error ? error.message : String(error);
      throw error;
    } finally {
      logsLoading.value = false;
    }
  }
  async function pollLogs(generation: number) {
    const { tab } = storeToRefs(useUiStore());
    try {
      await refreshLogs();
    } catch {
      /* Keep the previous logs on transient IPC failure. */
    }
    if (generation === logsGeneration && tab.value === "logs")
      logsTimer = setTimeout(() => void pollLogs(generation), 1000);
  }
  // Poll while the logs page is active, and invalidate the current round on leaving; driven by the ui tab watcher.
  async function setLogsActive(active: boolean) {
    clearTimeout(logsTimer);
    const generation = ++logsGeneration;
    if (active) {
      followLogs.value = true;
      void pollLogs(generation);
      await scrollLogs();
    }
  }
  function teardown() {
    logsGeneration++;
    clearTimeout(logsTimer);
  }

  return {
    logs,
    activityLogs,
    logViewport,
    followLogs,
    logsLoading,
    logsError,
    allLogs,
    logsText,
    renderLogLine,
    logLine,
    addActivity,
    onLogScroll,
    scrollLogs,
    refreshLogs,
    pollLogs,
    setLogsActive,
    teardown,
  };
});
