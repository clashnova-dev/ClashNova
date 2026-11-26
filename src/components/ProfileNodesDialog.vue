<script setup lang="ts">
  // "Edit nodes" dialog: the left side batch-adds nodes by pasting URI/Base64, the right side lists existing nodes (filterable, deletable one by one).
  // "Advanced" jumps to the raw file editor. Save reuses saveExternalNodes: convert and append, then validate and apply, with the selector rebuilt automatically.
  import { computed, ref } from "vue";
  import { Trash2 } from "@lucide/vue";
  import { useApp } from "../composables/useApp";
  import BaseDialog from "./BaseDialog.vue";
  import BaseButton from "./ui/BaseButton.vue";

  type NodeRow = { tag: string; type: string };
  const { t, tx, saveExternalNodes } = useApp();
  const emit = defineEmits<{ (e: "advanced", id: string): void }>();
  const base = ref<InstanceType<typeof BaseDialog> | null>(null);
  const id = ref("");
  const rows = ref<NodeRow[]>([]);
  const add = ref("");
  const filter = ref("");
  const error = ref("");
  const busy = ref(false);

  function open(profile: { id: string; raw: string }) {
    id.value = profile.id;
    add.value = "";
    filter.value = "";
    error.value = "";
    try {
      const config = JSON.parse(profile.raw);
      const outbounds: any[] = Array.isArray(config.outbounds) ? config.outbounds : [];
      rows.value = outbounds
        .filter(o => o && typeof o.server === "string" && o.server && Number.isInteger(o.server_port))
        .map(o => ({ tag: String(o.tag ?? ""), type: String(o.type ?? "") }));
    } catch {
      rows.value = [];
      error.value = "error.subscriptionNotSingBox";
    }
    base.value?.open();
  }

  const shown = computed(() => {
    const q = filter.value.trim().toLowerCase();
    return q ? rows.value.filter(r => r.tag.toLowerCase().includes(q)) : rows.value;
  });

  function removeRow(tag: string) {
    rows.value = rows.value.filter(r => r.tag !== tag);
  }
  function close() {
    base.value?.close();
  }
  function advanced() {
    const current = id.value;
    base.value?.close();
    emit("advanced", current);
  }

  async function save() {
    if (busy.value) return;
    error.value = "";
    busy.value = true;
    try {
      await saveExternalNodes(
        id.value,
        rows.value.map(r => r.tag),
        add.value
      );
      base.value?.close();
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      busy.value = false;
    }
  }

  defineExpose({ open });
</script>

<template>
  <BaseDialog ref="base" size="lg" :title="t('subscriptions.editNodesTitle')" @close="error = ''">
    <template #head>
      <BaseButton variant="link" class="nodes-adv" @click="advanced">
        {{ t("subscriptions.editNodesAdvanced") }}
      </BaseButton>
    </template>
    <div class="nodes-body">
      <div class="nodes-add">
        <textarea
          v-model="add"
          class="nodes-paste"
          :placeholder="t('subscriptions.editNodesPaste')"
          spellcheck="false"
        />
        <p class="hint">{{ t("subscriptions.editNodesAddHint") }}</p>
      </div>
      <div class="nodes-list-wrap">
        <input v-model="filter" class="nodes-filter" type="text" :placeholder="t('subscriptions.editNodesFilter')" />
        <div class="nodes-list">
          <div v-for="row in shown" :key="row.tag" class="node-row">
            <div class="node-meta">
              <span class="node-name" :title="row.tag">{{ row.tag }}</span><span v-if="row.type" class="node-type">{{
                row.type
              }}</span>
            </div>
            <BaseButton
              variant="icon"
              class="node-del"
              :aria-label="t('subscriptions.deleteNode')"
              :title="t('subscriptions.deleteNode')"
              @click="removeRow(row.tag)"
            >
              <Trash2 :size="16" />
            </BaseButton>
          </div>
          <p v-if="!shown.length" class="nodes-empty">{{ t("subscriptions.editNodesEmpty") }}</p>
        </div>
      </div>
    </div>
    <p v-if="error" class="form-error nodes-error" role="alert">{{ tx(error) }}</p>
    <template #footer>
      <span class="nodes-count">{{ t("subscriptions.editNodesCount", { count: rows.length }) }}</span>
      <BaseButton :disabled="busy" @click="close">{{ t("common.cancel") }}</BaseButton>
      <BaseButton variant="primary" :disabled="busy" @click="save">
        {{ busy ? t("connections.generating") : t("lan.save") }}
      </BaseButton>
    </template>
  </BaseDialog>
</template>

<style lang="scss" scoped>
  .nodes-adv {
    font-weight: 500;
  }
  .nodes-body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
  }
  .nodes-add {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .nodes-paste {
    flex: 1;
    min-height: 160px;
    resize: none;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font:
      13px/1.6 ui-monospace,
      SFMono-Regular,
      Menlo,
      Consolas,
      monospace;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar) transparent;
  }
  .nodes-add .hint {
    margin: 10px 0 0;
    font-size: 12px;
  }
  .nodes-list-wrap {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .nodes-filter {
    flex: none;
    width: 100%;
    min-height: 38px;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font-size: 14px;
  }
  .nodes-list {
    flex: 1;
    min-height: 0;
    margin-top: 10px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar) transparent;
  }
  .node-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--surface);
  }
  .node-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .node-name {
    font-size: 14px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .node-type {
    flex: none;
    padding: 1px 7px;
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--muted);
    font-size: 11px;
  }
  .node-del {
    flex: none;
    width: 32px;
    min-height: 32px;
    padding: 7px;
    color: var(--muted);
  }
  .node-del:hover {
    color: var(--danger);
    background: var(--danger-bg);
  }
  .nodes-empty {
    margin: 24px 0;
    text-align: center;
    color: var(--muted);
    font-size: 13px;
  }
  .nodes-error {
    flex: none;
    margin: 12px 0 0;
  }
  .nodes-count {
    margin-right: auto;
    color: var(--muted);
    font-size: 13px;
  }
  @media (max-width: 680px) {
    .nodes-body {
      grid-template-columns: 1fr;
    }
    .nodes-paste {
      min-height: 120px;
    }
  }
</style>
