<script setup lang="ts">
// 「编辑文件」弹窗:编辑某个本地订阅的原始内容(sing-box JSON)。打开时美化便于阅读,
// 保存时经 saveExternalEdit 规整校验;若是正在使用的订阅,保存后会刷新节点并重新下发。
import { nextTick, ref } from "vue";
import { Braces, ClipboardPaste, Maximize2, Minimize2 } from "@lucide/vue";
import { useApp } from "../composables/useApp";
import BaseDialog from "./BaseDialog.vue";
import CodeEditor from "./CodeEditor.vue";
import { prettyJson } from "../lib/json-highlight";

const { t, tx, saveExternalEdit } = useApp();
const base = ref<InstanceType<typeof BaseDialog> | null>(null);
const editor = ref<InstanceType<typeof CodeEditor> | null>(null);
const id = ref("");
const draft = ref("");
const error = ref("");
const busy = ref(false);
const expanded = ref(false);

function open(profile: { id: string; name: string; url: string; raw: string }) {
  id.value = profile.id;
  draft.value = prettyJson(profile.raw);
  error.value = "";
  expanded.value = false;
  base.value?.open();
  nextTick(() => editor.value?.focus());
}
function close() { base.value?.close(); }

async function paste() {
  try {
    const text = await navigator.clipboard.readText();
    if (text) draft.value = text;
  } catch {
    // 剪贴板不可用时静默忽略。
  }
}
function format() { draft.value = prettyJson(draft.value); }

async function save() {
  if (busy.value) return;
  error.value = "";
  busy.value = true;
  try {
    await saveExternalEdit(id.value, draft.value);
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
  <BaseDialog ref="base" size="md" :expanded="expanded" :title="t('subscriptions.editTitle')" @close="error = ''">
    <CodeEditor ref="editor" v-model="draft" class="editor-body" />
    <p v-if="error" class="form-error editor-error" role="alert">{{ tx(error) }}</p>
    <template #footer>
      <div class="editor-tools">
        <button type="button" class="icon-button" :disabled="busy" :aria-label="t('subscriptions.editPaste')" :title="t('subscriptions.editPaste')" @click="paste"><ClipboardPaste :size="18" /></button>
        <button type="button" class="icon-button" :disabled="busy" :aria-label="t('subscriptions.editFormat')" :title="t('subscriptions.editFormat')" @click="format"><Braces :size="18" /></button>
        <button type="button" class="icon-button" :aria-label="t(expanded ? 'subscriptions.editCollapse' : 'subscriptions.editExpand')" :title="t(expanded ? 'subscriptions.editCollapse' : 'subscriptions.editExpand')" @click="expanded = !expanded"><component :is="expanded ? Minimize2 : Maximize2" :size="18" /></button>
      </div>
      <button type="button" @click="close">{{ t('common.cancel') }}</button>
      <button type="button" class="primary" :disabled="busy || !draft.trim()" @click="save">{{ busy ? t('connections.generating') : t('lan.save') }}</button>
    </template>
  </BaseDialog>
</template>

<style scoped>
.editor-body { flex: 1; min-height: 0; }
.editor-error { flex: none; margin: 12px 0 0; }
.editor-tools { display: flex; gap: 6px; margin-right: auto; }
.editor-tools .icon-button { color: var(--muted); }
</style>
