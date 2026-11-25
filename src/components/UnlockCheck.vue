<script setup lang="ts">
import { computed, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "@lucide/vue";
import HelpTip from "./ui/HelpTip.vue";
import { useApp } from "../composables/useApp";
import type { MessageKey, Text } from "../i18n";

const { t, tx, isNative, connected, selectedNode, selectedNodeName } = useApp();

type Status = "available" | "partial" | "region_blocked" | "unreachable" | "unknown";
type Item = { serviceId: string; status: Status; region: string };
type Group = "all" | "ai" | "streaming" | "other";

const icons = import.meta.glob("../assets/unlock/*.png", { eager: true, import: "default" }) as Record<string, string>;

const services = [
  { id: "chatgpt", name: "unlock.chatgpt", group: "ai" },
  { id: "claude", name: "unlock.claude", group: "ai" },
  { id: "gemini", name: "unlock.gemini", group: "ai" },
  { id: "copilot", name: "unlock.copilot", group: "ai" },
  { id: "perplexity", name: "unlock.perplexity", group: "ai" },
  { id: "metaai", name: "unlock.metaai", group: "ai" },
  { id: "netflix", name: "unlock.netflix", group: "streaming" },
  { id: "disney", name: "unlock.disney", group: "streaming" },
  { id: "youtube", name: "unlock.youtube", group: "streaming" },
  { id: "prime", name: "unlock.prime", group: "streaming" },
  { id: "max", name: "unlock.max", group: "streaming" },
  { id: "paramount", name: "unlock.paramount", group: "streaming" },
  { id: "peacock", name: "unlock.peacock", group: "streaming" },
  { id: "hulu", name: "unlock.hulu", group: "streaming" },
  { id: "dazn", name: "unlock.dazn", group: "streaming" },
  { id: "abema", name: "unlock.abema", group: "streaming" },
  { id: "bahamut", name: "unlock.bahamut", group: "streaming" },
  { id: "bilibili", name: "unlock.bilibili", group: "streaming" },
  { id: "bbc", name: "unlock.bbc", group: "streaming" },
  { id: "iqiyi", name: "unlock.iqiyi", group: "streaming" },
  { id: "kktv", name: "unlock.kktv", group: "streaming" },
  { id: "viu", name: "unlock.viu", group: "streaming" },
  { id: "mytvsuper", name: "unlock.mytvsuper", group: "streaming" },
  { id: "spotify", name: "unlock.spotify", group: "streaming" },
  { id: "tiktok", name: "unlock.tiktok", group: "streaming" },
  { id: "apple", name: "unlock.apple", group: "other" },
  { id: "wikipedia", name: "unlock.wikipedia", group: "other" },
  { id: "reddit", name: "unlock.reddit", group: "other" },
] as const;

function logo(id: string) {
  return icons[`../assets/unlock/${id}.png`] ?? "";
}

const statusText: Record<Status, MessageKey> = {
  available: "unlock.available",
  partial: "unlock.partial",
  region_blocked: "unlock.regionBlocked",
  unreachable: "unlock.unreachable",
  unknown: "unlock.unknown",
};

const filters: { id: Group; label: MessageKey }[] = [
  { id: "all", label: "unlock.all" },
  { id: "ai", label: "unlock.ai" },
  { id: "streaming", label: "unlock.streaming" },
  { id: "other", label: "unlock.other" },
];

const filter = ref<Group>("all");
const results = ref<Record<string, Item>>({});
const running = ref(false);
const ran = ref(false);
const error = ref<Text>("");
const busy = ref<string[]>([]);
let ticket = 0;

const visible = computed(() => filter.value === "all" ? services : services.filter((service) => service.group === filter.value));
const nodeLabel = computed(() => t("unlock.currentNode", { name: tx(selectedNodeName.value) || t("nodes.notSelected") }));

function statusKey(id: string): MessageKey {
  const status = results.value[id]?.status;
  return status ? statusText[status] : "unlock.pending";
}

async function run(ids: string[], refresh: boolean) {
  const current = ++ticket;
  running.value = true;
  busy.value = ids;
  error.value = "";
  if (!isNative) {
    error.value = "unlock.desktopOnly";
  } else if (!connected.value) {
    error.value = "unlock.needConnection";
  } else {
    try {
      const report = await invoke<{ items: Item[] }>("unlock_check", { nodeId: selectedNode.value, services: ids, refresh });
      if (current !== ticket) return;
      for (const item of report.items) results.value[item.serviceId] = item;
    } catch (reason) {
      if (current !== ticket) return;
      error.value = reason instanceof Error ? reason.message : String(reason);
    }
  }
  if (current === ticket) {
    running.value = false;
    ran.value = true;
    busy.value = [];
  }
}

onUnmounted(() => {
  ticket += 1;
  if (isNative) void invoke("unlock_cancel").catch(() => undefined);
});
</script>

<template>
  <section class="card unlock" :aria-label="t('unlock.title')" :aria-busy="running">
    <header class="unlock-head">
      <h2>{{ t("unlock.title") }}<HelpTip :text="t('unlock.subtitle')" /></h2>
      <button class="primary" :disabled="running" @click="run(visible.map((service) => service.id), ran)"><RefreshCw :size="16" :stroke-width="2" />{{ running ? t("unlock.running") : ran ? t("unlock.again") : t("unlock.start") }}</button>
    </header>
    <div class="unlock-tools">
      <div class="unlock-filters" role="group" :aria-label="t('unlock.title')">
        <button v-for="item in filters" :key="item.id" type="button" :aria-pressed="filter === item.id" :class="{ chosen: filter === item.id }" @click="filter = item.id">{{ t(item.label) }}</button>
      </div>
      <span class="unlock-node">{{ nodeLabel }}</span>
    </div>
    <div class="unlock-grid">
      <article v-for="service in visible" :key="service.id" class="unlock-card">
        <div class="unlock-card-top">
          <img class="unlock-logo" :src="logo(service.id)" alt="" width="28" height="28" />
          <button type="button" class="unlock-retry" :disabled="running" :aria-label="t('unlock.retry', { name: t(service.name) })" @click="run([service.id], true)"><RefreshCw :class="{ spin: busy.includes(service.id) }" :size="15" :stroke-width="1.8" /></button>
        </div>
        <strong>{{ t(service.name) }}</strong>
        <span class="unlock-status" :class="results[service.id]?.status || 'pending'">{{ t(statusKey(service.id)) }}<template v-if="results[service.id]?.region"> · {{ results[service.id].region }}</template></span>
      </article>
    </div>
    <p v-if="error" class="form-error" role="alert">{{ tx(error) }}</p>
  </section>
</template>

<style scoped>
.unlock h2 { display: inline-flex; align-items: center; }
.unlock-head, .unlock-tools, .unlock-card-top { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.unlock-head button { flex: none; min-height: 40px; padding: 0 16px; }
.unlock-tools { margin-top: 14px; min-width: 0; }
.unlock-filters { display: flex; flex-wrap: wrap; gap: 4px; }
.unlock-filters button { min-height: 30px; padding: 4px 12px; border: 0; border-radius: 8px; background: transparent; color: var(--muted); }
.unlock-filters button.chosen { background: var(--pill-bg); color: var(--text); font-weight: 600; }
.unlock-node { min-width: 0; color: var(--muted); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.unlock-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; margin-top: 14px; }
.unlock-card { min-width: 0; padding: 14px 14px 12px; border: 1px solid var(--line); border-radius: 8px; background: var(--bg); }
.unlock-card-top { color: var(--muted); }
.unlock-logo { width: 28px; height: 28px; object-fit: contain; border-radius: 6px; }
.unlock-retry { width: 28px; height: 28px; min-height: 0; padding: 0; border: 0; background: transparent; color: var(--muted); }
.unlock-card strong { display: block; margin-top: 14px; font-size: 15px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.unlock-status { display: inline-block; max-width: 100%; margin-top: 8px; padding: 2px 8px; border-radius: 8px; background: var(--pill-bg); color: var(--pill-text); font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.unlock-status.available { background: var(--success-bg); color: var(--success); }
.unlock-status.partial { background: var(--warn-bg); color: var(--warn); }
.unlock-status.region_blocked, .unlock-status.unreachable { background: var(--danger-bg); color: var(--danger); }
.unlock .form-error { margin-top: 12px; }
@media (max-width: 980px) {
  .unlock-tools { align-items: flex-start; flex-direction: column; }
  .unlock-node { white-space: normal; }
}
@media (max-width: 640px) {
  .unlock-grid { grid-template-columns: 1fr; }
  .unlock-head { align-items: flex-start; flex-wrap: wrap; }
}
</style>
