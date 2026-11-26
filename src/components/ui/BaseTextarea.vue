<script setup lang="ts">
  // Unified multiline text field. Matches BaseInput's look. The specialized JSON editor
  // (CodeEditor.vue) stays separate; this is for plain multiline input.
  import { useId } from "vue";

  defineOptions({ inheritAttrs: false });

  const props = withDefaults(
    defineProps<{
      modelValue?: string;
      label?: string;
      hint?: string;
      error?: string;
      modelModifiers?: { trim?: boolean };
    }>(),
    { modelModifiers: () => ({}) }
  );
  const emit = defineEmits<{ "update:modelValue": [value: string] }>();
  const errorId = useId();

  function onInput(e: Event) {
    emit("update:modelValue", (e.target as HTMLTextAreaElement).value);
  }
  function onChange(e: Event) {
    if (props.modelModifiers.trim) emit("update:modelValue", (e.target as HTMLTextAreaElement).value.trim());
  }
</script>

<template>
  <label class="base-field">
    <span v-if="label" class="base-field-label">{{ label }}</span>
    <textarea
      class="base-textarea"
      :value="modelValue"
      :aria-invalid="error ? true : undefined"
      :aria-describedby="error ? errorId : undefined"
      v-bind="$attrs"
      @input="onInput"
      @change="onChange"
    />
    <span v-if="error" :id="errorId" class="base-field-error" role="alert">{{ error }}</span>
    <span v-else-if="hint" class="base-field-hint">{{ hint }}</span>
  </label>
</template>

<style scoped>
  .base-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .base-field-label {
    font-size: 14px;
    color: var(--muted);
  }
  .base-textarea {
    width: 100%;
    min-height: 88px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: inherit;
    font-weight: 400;
    resize: vertical;
  }
  .base-textarea:focus,
  .base-textarea:focus-visible {
    outline: none;
    border-color: var(--selected-border);
  }
  .base-textarea:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .base-field-hint {
    font-size: 13px;
    color: var(--muted);
  }
  .base-field-error {
    font-size: 13px;
    color: var(--danger);
  }
</style>
