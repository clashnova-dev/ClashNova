<script setup lang="ts">
// 带行号与 JSON 语法高亮的代码编辑器。做法:透明 textarea 叠在只读高亮层之上,
// 二者字体/行高/内边距完全一致,滚动同步,从而既可编辑又有高亮(无第三方依赖)。
import { computed, nextTick, ref } from "vue";
import { highlightJson } from "../lib/json-highlight";

const props = defineProps<{ modelValue: string }>();
const emit = defineEmits<{ (e: "update:modelValue", value: string): void }>();

const input = ref<HTMLTextAreaElement | null>(null);
const layer = ref<HTMLPreElement | null>(null);
const gutter = ref<HTMLDivElement | null>(null);

const highlighted = computed(() => highlightJson(props.modelValue));
const lineCount = computed(() => props.modelValue.split("\n").length || 1);

function onInput(event: Event) {
  emit("update:modelValue", (event.target as HTMLTextAreaElement).value);
}

// 高亮层与行号栏自身不滚动,跟随 textarea 的滚动位移,保证字符逐行对齐。
function syncScroll() {
  const field = input.value;
  if (!field) return;
  if (layer.value) {
    layer.value.scrollTop = field.scrollTop;
    layer.value.scrollLeft = field.scrollLeft;
  }
  if (gutter.value) gutter.value.scrollTop = field.scrollTop;
}

// Tab 缩进两空格,而不是把焦点移出编辑器。
function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Tab") return;
  event.preventDefault();
  const field = event.target as HTMLTextAreaElement;
  const { selectionStart, selectionEnd, value } = field;
  emit("update:modelValue", value.slice(0, selectionStart) + "  " + value.slice(selectionEnd));
  nextTick(() => {
    field.selectionStart = field.selectionEnd = selectionStart + 2;
  });
}

defineExpose({ focus: () => input.value?.focus() });
</script>

<template>
  <div class="code-editor">
    <div ref="gutter" class="code-gutter" aria-hidden="true"><div v-for="n in lineCount" :key="n">{{ n }}</div></div>
    <div class="code-area">
      <pre ref="layer" class="code-layer" aria-hidden="true"><code v-html="highlighted"></code><br></pre>
      <textarea
        ref="input" class="code-input" :value="modelValue" spellcheck="false"
        autocapitalize="off" autocomplete="off" autocorrect="off" wrap="off"
        @input="onInput" @scroll="syncScroll" @keydown="onKeydown"
      ></textarea>
    </div>
  </div>
</template>

<style scoped>
.code-editor {
  display: flex; flex: 1; min-height: 0; min-width: 0;
  border: 1px solid var(--border); border-radius: 10px; overflow: hidden; background: var(--bg);
}
.code-gutter {
  flex: none; overflow: hidden; padding: 12px 0; text-align: right;
  color: var(--muted); background: var(--surface); border-right: 1px solid var(--line); user-select: none;
}
.code-gutter > div {
  padding: 0 10px 0 14px; min-width: 20px;
  font: 13px/20px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-variant-numeric: tabular-nums;
}
.code-area { position: relative; flex: 1; min-width: 0; }
/* 高亮层与输入层必须完全同参数,字符才会逐一对齐。 */
.code-layer, .code-input {
  margin: 0; padding: 12px 14px; border: 0; box-sizing: border-box;
  font: 13px/20px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  tab-size: 2; white-space: pre; overflow-wrap: normal; word-break: normal; letter-spacing: 0;
}
.code-layer { position: absolute; inset: 0; overflow: hidden; pointer-events: none; color: var(--text); }
.code-layer code { font: inherit; }
.code-input {
  position: absolute; inset: 0; width: 100%; height: 100%; overflow: auto; resize: none;
  background: transparent; color: transparent; caret-color: var(--text);
  scrollbar-width: thin; scrollbar-color: var(--scrollbar) transparent;
}
.code-input:focus { outline: none; }
.code-input::selection { background: color-mix(in srgb, var(--accent) 28%, transparent); }
/* token 配色复用主题变量,明暗模式自动适配。 */
.code-layer :deep(.tok-key) { color: var(--accent); }
.code-layer :deep(.tok-str) { color: var(--success); }
.code-layer :deep(.tok-num) { color: var(--warn); }
.code-layer :deep(.tok-bool) { color: var(--danger); }
.code-layer :deep(.tok-null) { color: var(--muted); }
</style>
