<script setup lang="ts">
  // "Add device" dialog: manually add a peer by address. The address is two-way bound to the
  // parent and the actual add runs there; this is the view only.
  import { ref } from "vue";
  import { Plus, Link2 } from "@lucide/vue";
  import { useApp } from "../../composables/useApp";
  import BaseButton from "../ui/BaseButton.vue";
  import LanDialog from "./LanDialog.vue";

  defineProps<{ adding: boolean }>();
  const emit = defineEmits<{ (e: "submit"): void }>();
  const addr = defineModel<string>("addr", { default: "" });

  const { t } = useApp();
  const dialog = ref<InstanceType<typeof LanDialog>>();

  defineExpose({ open: () => dialog.value?.open(), close: () => dialog.value?.close() });
</script>

<template>
  <LanDialog
    ref="dialog"
    labelledby="lan-add-title"
    :title="t('lan.addDevice')"
    variant="add-device-dialog"
    form
    @submit="emit('submit')"
    @close="addr = ''"
  >
    <template #icon><Plus :size="22" /></template>
    <label class="field-label" for="lan-add-input">{{ t("lan.manualAdd") }}</label>
    <div class="dialog-field">
      <Link2 class="field-icon" :size="15" /><input
        id="lan-add-input"
        v-model="addr"
        type="text"
        inputmode="url"
        :placeholder="t('lan.manualPlaceholder')"
        :disabled="adding"
      />
    </div>
    <template #actions>
      <BaseButton :disabled="adding" @click="dialog?.close()"> {{ t("common.cancel") }}</BaseButton><BaseButton
        type="submit"
        class="primary"
        :disabled="adding || !addr.trim()"
      >
        <Plus :size="15" />{{ t("lan.addDevice") }}
      </BaseButton>
    </template>
  </LanDialog>
</template>

<style scoped lang="scss">
  :deep(.incoming-heading) {
    align-items: center;
  }
  :deep(.incoming-actions) {
    margin-top: 20px;
  }
  .field-label {
    display: block;
    margin: 16px 0 7px;
    color: var(--muted);
    font-size: 12px;
  }
  .dialog-field {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--bg);
  }
  .dialog-field:focus-within {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
    border-color: var(--selected-border);
  }
  .field-icon {
    flex: none;
    color: var(--muted);
  }
  .dialog-field input {
    flex: 1;
    width: 0;
    min-width: 0;
    min-height: 40px;
    padding: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 13px;
  }
</style>
