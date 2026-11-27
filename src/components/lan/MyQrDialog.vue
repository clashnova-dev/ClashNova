<script setup lang="ts">
  // "My QR code" dialog: shows this device's QR so a phone can scan and connect directly.
  import { computed, ref } from "vue";
  import { QrCode } from "@lucide/vue";
  import { renderSVG } from "uqr";
  import { useApp } from "../../composables/useApp";
  import BaseButton from "../ui/BaseButton.vue";
  import LanDialog from "./LanDialog.vue";
  import type { LanLocalDevice } from "../../lib/lan-transfer";

  const props = defineProps<{ device: LanLocalDevice | null }>();
  const { t } = useApp();
  const dialog = ref<InstanceType<typeof LanDialog>>();

  // QR direct connect: this device's QR payload (clashnova://lan?…). Scanning it on mobile allows a unicast direct connection,
  // with no multicast required (works even when iOS hasn't granted multicast permission). alias uses encodeURIComponent so the
  // mobile side's Uri.queryParameters decodes it correctly.
  const qrPayload = computed(() => {
    const d = props.device;
    if (!d || !d.ip || !d.fingerprint) return "";
    return `clashnova://lan?v=1&ip=${d.ip}&port=${d.port}&fp=${d.fingerprint}&alias=${encodeURIComponent(d.alias)}`;
  });
  const qrSvg = computed(() => (qrPayload.value ? renderSVG(qrPayload.value, { border: 2 }) : ""));

  defineExpose({ open: () => dialog.value?.open(), close: () => dialog.value?.close() });
</script>

<template>
  <LanDialog ref="dialog" labelledby="lan-qr-title" :title="t('lan.myQr')" variant="qr-dialog">
    <template #icon><QrCode :size="22" /></template>
    <div v-if="qrPayload" class="qr-box">
      <!-- eslint-disable-next-line vue/no-v-html -->
      <div class="qr-svg" v-html="qrSvg" />
      <div class="qr-meta">
        <strong>{{ device?.alias }}</strong><span>{{ device?.ip }}:{{ device?.port }}</span>
      </div>
      <p class="qr-hint">{{ t("lan.myQrHint") }}</p>
    </div>
    <p v-else class="qr-hint qr-noip">{{ t("lan.qrNoIp") }}</p>
    <template #actions>
      <BaseButton class="primary" @click="dialog?.close()">
        {{ t("common.done") }}
      </BaseButton>
    </template>
  </LanDialog>
</template>

<style scoped lang="scss">
  .incoming-dialog.qr-dialog {
    width: 320px;
  }
  .qr-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    margin: 10px 0 18px;
  }
  .qr-svg {
    width: 220px;
    height: 220px;
    padding: 10px;
    background: #fff;
    border-radius: 12px;
    box-sizing: content-box;
  }
  .qr-svg :deep(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }
  .qr-meta {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .qr-meta strong {
    font-size: 14px;
  }
  .qr-meta span {
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .qr-hint {
    color: var(--muted);
    font-size: 12px;
    text-align: center;
    margin: 0;
    line-height: 1.5;
  }
  .qr-noip {
    margin: 20px 0;
  }
</style>
