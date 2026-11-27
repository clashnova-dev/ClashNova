<script setup lang="ts">
  // LAN settings dialog: device name + discoverable / accept-incoming / auto-accept toggles.
  // State is owned by the parent (two-way bound) and saving runs there; this is the view only.
  import { ref } from "vue";
  import { Settings, Check } from "@lucide/vue";
  import { useApp } from "../../composables/useApp";
  import BaseButton from "../ui/BaseButton.vue";
  import LanDialog from "./LanDialog.vue";

  defineProps<{ saving: boolean }>();
  const emit = defineEmits<{ (e: "submit"): void }>();
  const name = defineModel<string>("name", { default: "" });
  const discoverable = defineModel<boolean>("discoverable", { default: true });
  const accept = defineModel<boolean>("accept", { default: true });
  const autoAccept = defineModel<boolean>("autoAccept", { default: true });

  const { t } = useApp();
  const dialog = ref<InstanceType<typeof LanDialog>>();

  defineExpose({ open: () => dialog.value?.open(), close: () => dialog.value?.close() });
</script>

<template>
  <LanDialog ref="dialog" labelledby="lan-settings-title" :title="t('lan.settingsTitle')" form @submit="emit('submit')">
    <template #icon><Settings :size="22" /></template>
    <label class="field-label" for="lan-name-input">{{ t("lan.deviceName") }}</label>
    <div class="dialog-field">
      <input
        id="lan-name-input"
        v-model="name"
        type="text"
        maxlength="64"
        :placeholder="t('lan.deviceName')"
        :disabled="saving"
      />
    </div>
    <label class="toggle-row"
      ><span>{{ t("lan.visible") }}</span
      ><input v-model="discoverable" type="checkbox" role="switch" :disabled="saving" /><span
        class="switch"
        aria-hidden="true"
    /></label>
    <label class="toggle-row"
      ><span>{{ t("lan.acceptIncoming") }}</span
      ><input v-model="accept" type="checkbox" role="switch" :disabled="saving" /><span
        class="switch"
        aria-hidden="true"
    /></label>
    <label class="toggle-row"
      ><span>{{ t("lan.autoAccept") }}</span
      ><input v-model="autoAccept" type="checkbox" role="switch" :disabled="saving || !accept" /><span
        class="switch"
        aria-hidden="true"
    /></label>
    <template #actions>
      <BaseButton :disabled="saving" @click="dialog?.close()"> {{ t("common.cancel") }}</BaseButton><BaseButton
        type="submit"
        class="primary"
        :disabled="saving || !name.trim()"
      >
        <Check :size="15" />{{ t("lan.save") }}
      </BaseButton>
    </template>
  </LanDialog>
</template>

<style scoped lang="scss">
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
  .toggle-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 20px 0 6px;
    font-size: 13px;
    cursor: pointer;
  }
  .toggle-row > span:first-child {
    flex: 1;
  }
  .toggle-row input {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }
  .switch {
    position: relative;
    flex: none;
    width: 38px;
    height: 22px;
    border-radius: 11px;
    background: var(--line);
    transition: background 0.15s ease;
  }
  .switch::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 2px rgba(16, 24, 40, 0.25);
    transition: transform 0.15s ease;
  }
  .toggle-row input:checked ~ .switch {
    background: var(--accent);
  }
  .toggle-row input:checked ~ .switch::after {
    transform: translateX(16px);
  }
  .toggle-row input:focus-visible ~ .switch {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
</style>
