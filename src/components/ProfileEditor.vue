<script setup lang="ts">
  // "Edit file" dialog: edits the raw content of a local subscription (sing-box JSON). It's prettified
  // on open for readability and normalized/validated via saveExternalEdit on save; if the subscription is
  // currently in use, saving refreshes its nodes and re-applies the config.
  import { nextTick, ref } from "vue";
  import { Braces, ClipboardPaste, Maximize2, Minimize2 } from "@lucide/vue";
  import { useApp } from "../composables/useApp";
  import BaseDialog from "./BaseDialog.vue";
  import BaseButton from "./ui/BaseButton.vue";
  import CodeEditor from "./CodeEditor.vue";
  import { prettyJson } from "../lib/json-highlight";

  const { t, tx, saveExternalEdit } = useApp();
  const base = ref<InstanceType<typeof BaseDialog> | null>(null);
  const editor = ref<InstanceType<typeof CodeEditor> | null>(null);
  const id = ref("");
  const name = ref("");
  const remote = ref(false);
  const draft = ref("");
  const error = ref("");
  const busy = ref(false);
  const expanded = ref(false);

  function open(profile: { id: string; name: string; url: string; raw: string }) {
    id.value = profile.id;
    name.value = profile.name;
    remote.value = profile.url.trim().length > 0;
    draft.value = prettyJson(profile.raw);
    error.value = "";
    expanded.value = false;
    base.value?.open();
    nextTick(() => editor.value?.focus());
  }
  function close() {
    base.value?.close();
  }

  async function paste() {
    try {
      const text = await navigator.clipboard.readText();
      if (text) draft.value = text;
    } catch {
      // Silently ignore when the clipboard is unavailable.
    }
  }
  function format() {
    draft.value = prettyJson(draft.value);
  }

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
  <BaseDialog
    ref="base"
    size="md"
    :expanded="expanded"
    :title="t('subscriptions.editNamed', { name })"
    @close="error = ''"
  >
    <p class="editor-hint">
      {{ remote ? t("subscriptions.editRemoteHint", { name }) : t("subscriptions.editHint", { name }) }}
    </p>
    <CodeEditor ref="editor" v-model="draft" class="editor-body" />
    <p v-if="error" class="form-error editor-error" role="alert">{{ tx(error) }}</p>
    <template #footer>
      <div class="editor-tools">
        <BaseButton
          class="icon-button"
          :disabled="busy"
          :aria-label="t('subscriptions.editPaste')"
          :title="t('subscriptions.editPaste')"
          @click="paste"
        >
          <ClipboardPaste :size="18" />
        </BaseButton>
        <BaseButton
          class="icon-button"
          :disabled="busy"
          :aria-label="t('subscriptions.editFormat')"
          :title="t('subscriptions.editFormat')"
          @click="format"
        >
          <Braces :size="18" />
        </BaseButton>
        <BaseButton
          class="icon-button"
          :aria-label="t(expanded ? 'subscriptions.editCollapse' : 'subscriptions.editExpand')"
          :title="t(expanded ? 'subscriptions.editCollapse' : 'subscriptions.editExpand')"
          @click="expanded = !expanded"
        >
          <component :is="expanded ? Minimize2 : Maximize2" :size="18" />
        </BaseButton>
      </div>
      <BaseButton @click="close">{{ t("common.cancel") }}</BaseButton>
      <BaseButton variant="primary" :disabled="busy || !draft.trim()" @click="save">
        {{ busy ? t("connections.generating") : t("lan.save") }}
      </BaseButton>
    </template>
  </BaseDialog>
</template>

<style lang="scss" scoped>
  .editor-hint {
    flex: none;
    margin: 0 0 12px;
    color: var(--muted);
    font-size: 13px;
    line-height: 1.5;
  }
  .editor-body {
    flex: 1;
    min-height: 0;
  }
  .editor-error {
    flex: none;
    margin: 12px 0 0;
  }
  .editor-tools {
    display: flex;
    gap: 6px;
    margin-right: auto;
  }
  .editor-tools .icon-button {
    color: var(--muted);
  }
</style>
