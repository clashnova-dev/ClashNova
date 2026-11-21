<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { ShieldAlert, X } from "@lucide/vue";
import { confirmation } from "../../lib/confirmation";
import { translator, type Locale } from "../../i18n";
const props = defineProps<{ locale: Locale }>();
const { t, tx } = translator(() => props.locale);
const current = confirmation.current;
const dialog = ref<HTMLDialogElement | null>(null);
function settle(approved: boolean) {
  if (current.value) confirmation.settle(current.value.id, approved);
}
function trapFocus(event: KeyboardEvent) {
  if (event.key !== "Tab") return;
  const buttons = dialog.value?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)");
  if (!buttons?.length) { event.preventDefault(); return; }
  const first = buttons[0], last = buttons[buttons.length - 1];
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
}
function sync() {
  if (current.value) {
    if (dialog.value && !dialog.value.open) dialog.value.showModal();
  } else if (dialog.value?.open) dialog.value.close();
}
watch(current, sync, { flush: "post" });
onMounted(sync);
onUnmounted(() => confirmation.cancel());
</script>

<template>
  <dialog ref="dialog" class="confirm-dialog" role="alertdialog" aria-labelledby="confirmation-title" aria-describedby="confirmation-body" @keydown="trapFocus" @cancel.prevent="settle(false)" @close="!dialog?.open && settle(false)" @click="$event.target === dialog && settle(false)">
    <template v-if="current">
      <div class="confirm-heading">
        <span class="confirm-icon" :class="{ destructive: current.danger }"><ShieldAlert :size="22" aria-hidden="true" /></span>
        <h2 id="confirmation-title">{{ tx(current.title) }}</h2>
        <button type="button" class="icon-button confirm-close" :aria-label="t('common.close')" @click="settle(false)"><X :size="18" /></button>
      </div>
      <p id="confirmation-body">{{ tx(current.body) }}</p>
      <ul v-if="current.details?.length" class="confirm-details"><li v-for="(detail, index) in current.details" :key="index">{{ tx(detail) }}</li></ul>
      <div class="confirm-actions">
        <button type="button" autofocus @click="settle(false)">{{ t("common.cancel") }}</button>
        <button type="button" :class="current.danger ? 'confirm-danger' : 'primary'" @click="settle(true)">{{ t(current.confirmLabel || "confirmation.continue") }}</button>
      </div>
    </template>
  </dialog>
</template>

<style scoped>
.confirm-dialog{width:min(420px,calc(100vw - 40px));max-height:calc(100dvh - 48px);overflow:auto;padding:24px;border:1px solid var(--border);border-radius:16px;background:var(--bg);color:var(--text);box-shadow:0 22px 70px #14213d33}
.confirm-dialog::backdrop{background:#101a2d66}
.confirm-heading{display:flex;align-items:center;gap:12px}
.confirm-icon{display:grid;place-items:center;flex:none;width:40px;height:40px;border-radius:12px;color:var(--accent-text);background:var(--nav-on)}
.confirm-icon.destructive{color:var(--danger);background:var(--danger-bg)}
.confirm-heading h2{flex:1;min-width:0;margin:0;font-size:18px;font-weight:650;line-height:1.4;overflow-wrap:anywhere}
.confirm-close{min-height:30px;width:30px;border:0;background:transparent;color:var(--muted)}
.confirm-dialog p{margin:18px 0 0;color:var(--muted);font-size:14px;line-height:1.7;overflow-wrap:anywhere}
.confirm-details{margin:16px 0 0;padding:12px 14px 12px 30px;border:1px solid var(--line);border-radius:10px;background:var(--surface);font-size:13px;line-height:1.7;overflow-wrap:anywhere}
.confirm-details li+li{margin-top:6px}
.confirm-actions{display:flex;justify-content:flex-end;gap:10px;margin-top:24px}
.confirm-actions button{min-width:84px}
.confirm-danger{background:var(--danger);border-color:var(--danger);color:var(--bg)}
.confirm-danger:hover:not(:disabled){background:var(--danger);border-color:var(--danger);color:var(--bg);filter:brightness(.94)}
@media(max-width:480px){.confirm-dialog{padding:20px}.confirm-actions button{flex:1;min-width:0}}
</style>
