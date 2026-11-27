<script setup lang="ts">
  // Shared shell for the LAN page dialogs (the "incoming-dialog" family: an icon badge + title,
  // an optional subtitle, a body, and a right-aligned actions footer). It owns the shared
  // incoming-* styling so each extracted dialog only supplies its own body and actions.
  //
  // The inner wrapper is a <form> when `form` is set (so the add/settings dialogs submit on Enter
  // and honour the submit button's type) and a transparent display:contents <div> otherwise. The
  // transparent wrapper keeps the heading/body/actions as direct layout children of the <dialog>,
  // which the paired dialog relies on for its flex column.
  import { ref } from "vue";

  const props = withDefaults(
    defineProps<{
      // The id placed on the title <h2> and referenced by the dialog's aria-labelledby.
      labelledby: string;
      title: string;
      // Extra class(es) added to the <dialog> for per-dialog overrides (e.g. "qr-dialog").
      variant?: string;
      form?: boolean;
      // When set, Escape is intercepted (default close prevented) and only emits "cancel" so the
      // parent can respond (pair/incoming dialogs reject instead of silently closing).
      preventCancel?: boolean;
    }>(),
    { form: false, preventCancel: false }
  );
  const emit = defineEmits<{ (e: "close"): void; (e: "cancel"): void; (e: "submit"): void }>();

  const dialog = ref<HTMLDialogElement>();
  function open() {
    dialog.value?.showModal();
  }
  function close() {
    dialog.value?.close();
  }
  function isOpen() {
    return !!dialog.value?.open;
  }
  function onCancel(event: Event) {
    if (props.preventCancel) event.preventDefault();
    emit("cancel");
  }

  defineExpose({ open, close, isOpen });
</script>

<template>
  <dialog
    ref="dialog"
    class="incoming-dialog"
    :class="variant"
    :aria-labelledby="labelledby"
    @close="emit('close')"
    @cancel="onCancel"
  >
    <component :is="form ? 'form' : 'div'" class="lan-dialog-wrap" @submit.prevent="form && emit('submit')">
      <header class="incoming-heading">
        <span class="incoming-icon"><slot name="icon" /></span>
        <div>
          <h2 :id="labelledby">{{ title }}</h2>
          <p v-if="$slots.subtitle"><slot name="subtitle" /></p>
        </div>
      </header>
      <slot />
      <div v-if="$slots.actions" class="incoming-actions"><slot name="actions" /></div>
    </component>
  </dialog>
</template>

<style scoped lang="scss">
  .incoming-dialog {
    width: 400px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 32px);
    overflow: auto;
    padding: 22px;
    border: 0;
    border-radius: 14px;
    background: var(--bg);
    color: var(--text);
    box-shadow: var(--shadow-hover);
  }
  .incoming-dialog::backdrop {
    background: #18293e66;
  }
  /* Transparent wrapper: keeps heading/body/actions as direct layout children of the dialog. */
  .lan-dialog-wrap {
    display: contents;
  }
  .incoming-heading {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }
  .incoming-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--selected);
    color: var(--accent-text);
  }
  .incoming-heading h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .incoming-heading p {
    margin: 5px 0 0;
    color: var(--muted);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  /* The subtitle's emphasised text is passed through the slot, so reach it with :slotted. */
  .incoming-heading :slotted(b) {
    color: var(--text);
    font-weight: 500;
  }
  .incoming-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  /* Footer buttons are slotted in (BaseButton renders a <button> root), so style via :slotted. */
  .incoming-actions :slotted(button) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 34px;
    padding: 6px 13px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--bg);
    color: var(--text);
    font-size: 13px;
  }
  .incoming-actions :slotted(button.primary) {
    border-color: transparent;
    background: var(--accent);
    color: #fff;
  }
  .incoming-actions :slotted(button:disabled) {
    opacity: 0.5;
  }
</style>
