<script setup lang="ts">
  // Code editor with line numbers and JSON syntax highlighting. Approach: a transparent textarea
  // sits over a read-only highlight layer; the two share identical font/line-height/padding and scroll
  // in sync, giving editable text with highlighting (no third-party dependencies).
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

  // The highlight layer and line-number gutter don't scroll on their own; they follow the textarea's scroll offset so characters stay aligned line by line.
  function syncScroll() {
    const field = input.value;
    if (!field) return;
    if (layer.value) {
      layer.value.scrollTop = field.scrollTop;
      layer.value.scrollLeft = field.scrollLeft;
    }
    if (gutter.value) gutter.value.scrollTop = field.scrollTop;
  }

  // Tab indents by two spaces instead of moving focus out of the editor.
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
    <div ref="gutter" class="code-gutter" aria-hidden="true">
      <div v-for="n in lineCount" :key="n">{{ n }}</div>
    </div>
    <div class="code-area">
      <pre ref="layer" class="code-layer" aria-hidden="true"><code v-html="highlighted"></code><br></pre>
      <textarea
        ref="input"
        class="code-input"
        :value="modelValue"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        autocorrect="off"
        wrap="off"
        @input="onInput"
        @scroll="syncScroll"
        @keydown="onKeydown"
      ></textarea>
    </div>
  </div>
</template>

<style lang="scss" scoped>
  .code-editor {
    display: flex;
    flex: 1;
    min-height: 0;
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    background: var(--bg);
  }
  .code-gutter {
    flex: none;
    overflow: hidden;
    padding: 12px 0;
    text-align: right;
    color: var(--muted);
    background: var(--surface);
    border-right: 1px solid var(--line);
    user-select: none;
  }
  .code-gutter > div {
    padding: 0 10px 0 14px;
    min-width: 20px;
    font:
      13px/20px ui-monospace,
      SFMono-Regular,
      Menlo,
      Consolas,
      monospace;
    font-variant-numeric: tabular-nums;
  }
  .code-area {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  /* The highlight layer and input layer must share identical parameters so characters line up exactly. */
  .code-layer,
  .code-input {
    margin: 0;
    padding: 12px 14px;
    border: 0;
    box-sizing: border-box;
    font:
      13px/20px ui-monospace,
      SFMono-Regular,
      Menlo,
      Consolas,
      monospace;
    tab-size: 2;
    white-space: pre;
    overflow-wrap: normal;
    word-break: normal;
    letter-spacing: 0;
  }
  .code-layer {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
    color: var(--text);
  }
  .code-layer code {
    font: inherit;
  }
  .code-input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: auto;
    resize: none;
    background: transparent;
    color: transparent;
    caret-color: var(--text);
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar) transparent;
  }
  .code-input:focus {
    outline: none;
  }
  .code-input::selection {
    background: color-mix(in srgb, var(--accent) 28%, transparent);
  }
  /* Token colors reuse theme variables, adapting to light/dark mode automatically. */
  .code-layer :deep(.tok-key) {
    color: var(--accent);
  }
  .code-layer :deep(.tok-str) {
    color: var(--success);
  }
  .code-layer :deep(.tok-num) {
    color: var(--warn);
  }
  .code-layer :deep(.tok-bool) {
    color: var(--danger);
  }
  .code-layer :deep(.tok-null) {
    color: var(--muted);
  }
</style>
