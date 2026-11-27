<script setup lang="ts">
  // Paired-device management dialog: lists trusted devices and lets the user unpair each one.
  import { ref } from "vue";
  import { ShieldCheck, Unlink } from "@lucide/vue";
  import { useApp } from "../../composables/useApp";
  import BaseButton from "../ui/BaseButton.vue";
  import LanDialog from "./LanDialog.vue";
  import type { LanTrustedDevice } from "../../lib/lan-transfer";

  defineProps<{ trusted: LanTrustedDevice[] }>();
  const emit = defineEmits<{ (e: "unpair", fingerprint: string): void }>();

  const { t } = useApp();
  const dialog = ref<InstanceType<typeof LanDialog>>();

  defineExpose({ open: () => dialog.value?.open(), close: () => dialog.value?.close() });
</script>

<template>
  <LanDialog
    ref="dialog"
    labelledby="lan-paired-title"
    :title="`${t('lan.pairedSection')} · ${trusted.length}`"
    variant="paired-dialog"
  >
    <template #icon><ShieldCheck :size="22" /></template>
    <div class="paired-manage">
      <div v-for="d in trusted" :key="d.fingerprint" class="paired-item">
        <span class="paired-ico"><ShieldCheck :size="16" /></span>
        <span class="paired-meta"
          ><strong :title="d.alias">{{ d.alias }}</strong
          ><small :title="d.fingerprint">{{ t("lan.trusted") }} · {{ d.fingerprint.slice(0, 12) }}…</small></span
        >
        <BaseButton
          class="paired-unpair"
          :title="t('lan.unpair')"
          :aria-label="`${t('lan.unpair')} · ${d.alias}`"
          @click="emit('unpair', d.fingerprint)"
        >
          <Unlink :size="14" />
        </BaseButton>
      </div>
      <p v-if="!trusted.length" class="paired-empty">{{ t("lan.noPaired") }}</p>
    </div>
    <template #actions>
      <BaseButton @click="dialog?.close()">{{ t("common.close") }}</BaseButton>
    </template>
  </LanDialog>
</template>

<style scoped lang="scss">
  .incoming-dialog.paired-dialog[open] {
    display: flex;
    flex-direction: column;
    height: min(420px, calc(100dvh - 64px));
    overflow: hidden;
  }
  :deep(.incoming-heading) {
    flex: none;
  }
  :deep(.incoming-actions) {
    flex: none;
    margin-top: 16px;
  }
  .paired-manage {
    margin-top: 16px;
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 9px;
  }
  .paired-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
  }
  .paired-item:last-child {
    border-bottom: 0;
  }
  .paired-ico {
    display: grid;
    place-items: center;
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: var(--surface);
    color: var(--success);
  }
  .paired-meta {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }
  .paired-meta strong {
    font-size: 13px;
    font-weight: 500;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .paired-meta small {
    color: var(--muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .paired-unpair {
    display: grid;
    place-items: center;
    flex: none;
    width: 30px;
    height: 30px;
    min-height: 30px;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
  }
  .paired-unpair:hover {
    background: var(--danger-bg);
    color: var(--danger);
  }
  .paired-empty {
    margin: 0;
    padding: 16px;
    text-align: center;
    color: var(--muted);
    font-size: 12px;
  }
</style>
