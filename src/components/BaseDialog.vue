<script setup lang="ts">
// 统一弹窗外壳:封装 <dialog> 的 showModal/close、点背景关闭、标题栏(标题 + head 插槽 + 关闭按钮)、
// 以及两个关键约定——① display 只在 [open] 下设,避免覆盖 dialog:not([open]){display:none} 导致关不掉;
// ② 统一柔和聚焦样式,去掉浏览器默认的蓝色方框。各弹窗只提供 body 与 footer 内容。
import { ref } from "vue";
import { X } from "@lucide/vue";
import { useApp } from "../composables/useApp";

withDefaults(defineProps<{ title?: string; size?: "sm" | "md" | "lg"; expanded?: boolean }>(), { size: "md", expanded: false });
const emit = defineEmits<{ (e: "close"): void }>();
const { t } = useApp();
const dialog = ref<HTMLDialogElement | null>(null);

function open() { dialog.value?.showModal(); }
function close() { dialog.value?.close(); }

defineExpose({ open, close });
</script>

<template>
  <dialog
    ref="dialog" class="subscription-dialog base-dialog" :class="['bd-' + size, { 'bd-expanded': expanded }]"
    @close="emit('close')" @click="$event.target === dialog && close()"
  >
    <div class="dialog-heading">
      <h2>{{ title }}</h2>
      <div class="bd-head-extra"><slot name="head" /><button class="icon-button" type="button" :aria-label="t('common.close')" @click="close"><X :size="20" /></button></div>
    </div>
    <div class="bd-body"><slot /></div>
    <div v-if="$slots.footer" class="bd-footer"><slot name="footer" /></div>
  </dialog>
</template>

<style scoped>
.base-dialog { box-sizing: border-box; max-height: calc(100vh - 48px); overflow: hidden; }
/* display 仅在 [open] 下设,否则覆盖 dialog:not([open]){display:none},弹窗关闭后仍可见、「关不掉」。 */
.base-dialog[open] { display: flex; flex-direction: column; }
.bd-sm { width: min(460px, calc(100vw - 40px)); }
.bd-md { width: min(680px, calc(100vw - 40px)); height: min(560px, calc(100vh - 64px)); }
.bd-lg { width: min(880px, calc(100vw - 40px)); height: min(620px, calc(100vh - 64px)); }
.base-dialog.bd-expanded { width: calc(100vw - 32px); height: calc(100vh - 32px); max-height: calc(100vh - 32px); }
.bd-head-extra { display: flex; align-items: center; gap: 10px; }
.bd-body { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: auto; margin-top: 16px; }
.bd-footer { flex: none; display: flex; align-items: center; gap: 10px; justify-content: flex-end; margin-top: 16px; }
/* 统一柔和聚焦:去掉默认蓝色方框,改用 accent 柔光(含「高级」等链接按钮)。 */
.base-dialog :focus-visible { outline: none; border-radius: 8px; box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
</style>
