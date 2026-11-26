import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { Phase } from "../lib/connection";
import type { Text, MessageKey } from "../i18n";
import { useI18nStore } from "./i18n";
import { notifications } from "../lib/notifications";

// core: connection-core state container. Connection phase / message / busy / guard / mode are shared across multiple features.
// Connection orchestration methods (connect/disconnect/bootstrap, etc.) stay in useApp for now and will be migrated in gradually.
export const useCoreStore = defineStore(
  "core",
  () => {
    const phase = ref<Phase>("IDLE");
    const message = ref<Text>("notice.signInFirst");
    const busy = ref(false);
    const connectionAction = ref<"connect" | "disconnect" | null>(null);
    const connectionProgress = ref("");
    // Kill switch: currently blocking the network (waiting for the tunnel to recover or to be released).
    const guardBlocking = ref(false);
    // Whether the kill switch is actually armed (backend arm succeeded).
    const guardArmed = ref(false);
    const connectionMode = ref<"system" | "tun" | "manual">("system");
    const connected = computed(() => phase.value === "CONNECTED" || phase.value === "RECONNECTING");
    const i18n = useI18nStore();
    // Failure reason: prefer Error.message / string, otherwise fall back to the copy key.
    const failure = (error: unknown, fallback: MessageKey): Text =>
      error instanceof Error && error.message ? error.message : typeof error === "string" && error ? error : fallback;
    // Unified error feedback: write to message and show a notification.
    function showError(detail: Text) {
      message.value = detail;
      void notifications.show({
        body: i18n.tx(detail),
        kind: "error",
        native: false,
        key: `error:${typeof detail === "string" ? detail : "detail"}`,
      });
    }
    return {
      phase,
      message,
      busy,
      connectionAction,
      connectionProgress,
      guardBlocking,
      guardArmed,
      connectionMode,
      connected,
      failure,
      showError,
    };
  },
  { persist: { key: "airport.core", pick: ["connectionMode"] } }
);
