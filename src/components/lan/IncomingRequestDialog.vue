<script setup lang="ts">
  // Incoming transfer confirmation (unpaired / third-party LocalSend fallback): confirm one text
  // or file batch at a time. Accept/reject (and Escape) are emitted to the parent, which owns the
  // request queue and drives open/close.
  import { computed, ref } from "vue";
  import { Radio, X, Check } from "@lucide/vue";
  import { useApp } from "../../composables/useApp";
  import BaseButton from "../ui/BaseButton.vue";
  import FileTypeIcon from "../FileTypeIcon.vue";
  import LanDialog from "./LanDialog.vue";
  import type { LanIncomingRequest } from "../../lib/lan-transfer";

  const props = defineProps<{ incoming: LanIncomingRequest | null; responding: boolean }>();
  const emit = defineEmits<{ (e: "respond", accept: boolean): void }>();

  const { t } = useApp();
  const dialog = ref<InstanceType<typeof LanDialog>>();

  const title = computed(() =>
    props.incoming ? t(props.incoming.text != null ? "lan.incomingTextTitle" : "lan.incomingTitle") : ""
  );

  function fmtSize(bytes: number): string {
    const units = ["B", "KB", "MB", "GB", "TB"];
    let value = Math.max(0, bytes),
      i = 0;
    while (value >= 1024 && i < units.length - 1) {
      value /= 1024;
      i++;
    }
    return `${i && value < 10 ? value.toFixed(1) : value.toFixed(0)} ${units[i]}`;
  }

  defineExpose({
    open: () => dialog.value?.open(),
    close: () => dialog.value?.close(),
    isOpen: () => dialog.value?.isOpen() ?? false,
  });
</script>

<template>
  <LanDialog
    ref="dialog"
    labelledby="lan-incoming-title"
    :title="title"
    prevent-cancel
    @cancel="emit('respond', false)"
  >
    <template #icon><Radio :size="22" /></template>
    <template v-if="incoming" #subtitle
      >{{ t("lan.from") }} <b>{{ incoming.peerAlias }}</b> · {{ fmtSize(incoming.totalSize) }}</template
    >
    <template v-if="incoming">
      <pre v-if="incoming.text != null" class="message-text">{{ incoming.text }}</pre>
      <ul v-else class="incoming-files">
        <li v-for="(f, i) in incoming.files" :key="i">
          <FileTypeIcon :name="f.name" :size="18" /><span :title="f.name">{{ f.name }}</span
          ><small>{{ fmtSize(f.size) }}</small>
        </li>
      </ul>
    </template>
    <template v-if="incoming" #actions>
      <BaseButton :disabled="responding" @click="emit('respond', false)">
        <X :size="15" />{{ t("lan.reject") }}</BaseButton
      ><BaseButton class="primary" :disabled="responding" @click="emit('respond', true)">
        <Check :size="15" />{{ t("lan.accept") }}
      </BaseButton>
    </template>
  </LanDialog>
</template>

<style scoped lang="scss">
  .message-text {
    margin: 16px 0;
    padding: 12px;
    max-height: 240px;
    overflow: auto;
    border-radius: 8px;
    background: var(--surface);
    font-family: inherit;
    font-size: 13px;
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }
  .incoming-files {
    padding: 4px 12px;
    margin: 16px 0;
    list-style: none;
    max-height: 200px;
    overflow: auto;
    border-radius: 8px;
    background: var(--surface);
  }
  .incoming-files li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 0;
    font-size: 12px;
  }
  .incoming-files svg {
    flex: none;
    color: var(--muted);
  }
  .incoming-files span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .incoming-files small {
    flex: none;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>
