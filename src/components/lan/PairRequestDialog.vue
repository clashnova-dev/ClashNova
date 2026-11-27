<script setup lang="ts">
  // Incoming pair-request dialog: another device asks to pair. Accept/reject (and Escape) are
  // emitted to the parent, which owns the request queue and drives open/close.
  import { ref } from "vue";
  import { Link2, X, Check } from "@lucide/vue";
  import { useApp } from "../../composables/useApp";
  import BaseButton from "../ui/BaseButton.vue";
  import LanDialog from "./LanDialog.vue";
  import type { LanPairRequest } from "../../lib/lan-transfer";

  defineProps<{ request: LanPairRequest | null; responding: boolean }>();
  const emit = defineEmits<{ (e: "respond", accept: boolean): void }>();

  const { t } = useApp();
  const dialog = ref<InstanceType<typeof LanDialog>>();

  defineExpose({
    open: () => dialog.value?.open(),
    close: () => dialog.value?.close(),
    isOpen: () => dialog.value?.isOpen() ?? false,
  });
</script>

<template>
  <LanDialog
    ref="dialog"
    labelledby="lan-pair-title"
    :title="t('lan.pairRequestTitle')"
    prevent-cancel
    @cancel="emit('respond', false)"
  >
    <template #icon><Link2 :size="22" /></template>
    <template v-if="request" #subtitle
      >{{ t("lan.from") }} <b>{{ request.peerAlias }}</b></template
    >
    <template v-if="request">
      <p class="pair-desc">{{ t("lan.pairDesc") }}</p>
    </template>
    <template v-if="request" #actions>
      <BaseButton :disabled="responding" @click="emit('respond', false)">
        <X :size="15" />{{ t("lan.reject") }}</BaseButton
      ><BaseButton class="primary" :disabled="responding" @click="emit('respond', true)">
        <Check :size="15" />{{ t("lan.accept") }}
      </BaseButton>
    </template>
  </LanDialog>
</template>

<style scoped lang="scss">
  .pair-desc {
    margin: 16px 0 20px;
    color: var(--muted);
    font-size: 13px;
    line-height: 1.6;
  }
</style>
