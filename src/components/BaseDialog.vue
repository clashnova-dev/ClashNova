<script setup lang="ts">
  // Shared dialog shell: wraps a <dialog>'s showModal/close, backdrop-click close, and a title bar
  // (title + head slot + close button). Two key conventions: (1) display is set only under [open], so it
  // doesn't override dialog:not([open]){display:none} and leave the dialog impossible to close;
  // (2) a unified soft focus style replaces the browser's default blue box. Each dialog only supplies body and footer content.
  import { ref } from "vue";
  import { X } from "@lucide/vue";
  import { useApp } from "../composables/useApp";
  import BaseButton from "./ui/BaseButton.vue";

  withDefaults(defineProps<{ title?: string; size?: "sm" | "md" | "lg"; expanded?: boolean }>(), {
    size: "md",
    expanded: false,
  });
  const emit = defineEmits<{ (e: "close"): void }>();
  const { t } = useApp();
  const dialog = ref<HTMLDialogElement | null>(null);

  function open() {
    dialog.value?.showModal();
    // showModal auto-focuses the first control (the close button), which shows a focus ring.
    // Move focus to the dialog itself (tabindex=-1, no visible ring); dialogs that want an input
    // focused do so after open(). A later keyboard Tab still shows focus rings normally.
    dialog.value?.focus();
  }
  function close() {
    dialog.value?.close();
  }

  defineExpose({ open, close });
</script>

<template>
  <dialog
    ref="dialog"
    tabindex="-1"
    class="subscription-dialog base-dialog"
    :class="['bd-' + size, { 'bd-expanded': expanded }]"
    @close="emit('close')"
    @click="$event.target === dialog && close()"
  >
    <div class="dialog-heading">
      <h2>{{ title }}</h2>
      <div class="bd-head-extra">
        <slot name="head" /><BaseButton variant="icon" :aria-label="t('common.close')" @click="close">
          <X :size="20" />
        </BaseButton>
      </div>
    </div>
    <div class="bd-body"><slot /></div>
    <div v-if="$slots.footer" class="bd-footer"><slot name="footer" /></div>
  </dialog>
</template>

<style lang="scss" scoped>
  .base-dialog {
    box-sizing: border-box;
    max-height: calc(100vh - 48px);
    overflow: hidden;
  }
  /* Set display only under [open]; otherwise it overrides dialog:not([open]){display:none}, leaving the dialog visible and "unclosable" after close. */
  .base-dialog[open] {
    display: flex;
    flex-direction: column;
  }
  .bd-sm {
    width: min(460px, calc(100vw - 40px));
  }
  .bd-md {
    width: min(680px, calc(100vw - 40px));
    height: min(560px, calc(100vh - 64px));
  }
  .bd-lg {
    width: min(880px, calc(100vw - 40px));
    height: min(620px, calc(100vh - 64px));
  }
  .base-dialog.bd-expanded {
    width: calc(100vw - 32px);
    height: calc(100vh - 32px);
    max-height: calc(100vh - 32px);
  }
  .bd-head-extra {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .bd-body {
    /* flex-basis auto so the body contributes its content height. With flex-basis 0 it
       collapses to zero inside an auto-height dialog (the sm size has no fixed height),
       which left sm dialogs showing only the title bar. md/lg have a fixed height, so the
       body still grows to fill it and scrolls when content overflows. */
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
    margin-top: 16px;
  }
  .bd-footer {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    justify-content: flex-end;
    margin-top: 16px;
  }
  /* Unified soft focus: drop the default blue box in favor of an accent glow (including link buttons like "Advanced"). */
  .base-dialog :focus-visible {
    outline: none;
    border-radius: 8px;
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent);
  }
</style>
