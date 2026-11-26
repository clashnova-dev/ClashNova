<script setup lang="ts">
  // Unified text input (text / url / number / password / email …). Owns the canonical field
  // styling so inputs look the same everywhere, and supports optional label / hint / error.
  // Native file inputs and checkboxes are intentionally out of scope — they stay raw.
  import { ref, useId } from "vue";

  defineOptions({ inheritAttrs: false });

  const props = withDefaults(
    defineProps<{
      modelValue?: string | number;
      label?: string;
      hint?: string;
      error?: string;
      // v-model modifiers (v-model.trim / v-model.number) are delivered here by Vue.
      modelModifiers?: { trim?: boolean; number?: boolean };
    }>(),
    { modelModifiers: () => ({}) }
  );
  const emit = defineEmits<{ "update:modelValue": [value: string | number] }>();
  const errorId = useId();
  const el = ref<HTMLInputElement | null>(null);
  // Expose focus() so callers that used a template ref on the old <input> keep working.
  defineExpose({ focus: () => el.value?.focus() });

  function cast(value: string): string | number {
    return props.modelModifiers.number && value !== "" ? Number(value) : value;
  }
  function onInput(e: Event) {
    // Emit untrimmed on input so spaces can be typed mid-value; .trim is applied on change.
    emit("update:modelValue", cast((e.target as HTMLInputElement).value));
  }
  function onChange(e: Event) {
    if (!props.modelModifiers.trim) return;
    const trimmed = (e.target as HTMLInputElement).value.trim();
    emit("update:modelValue", cast(trimmed));
  }
</script>

<template>
  <label class="base-field">
    <span v-if="label" class="base-field-label">{{ label }}</span>
    <input
      ref="el"
      class="base-input"
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
    gap: 8px;
    min-width: 0;
  }
  .base-field-label {
    font-size: 14px;
    font-weight: 550;
    color: var(--text);
  }
  .base-input {
    width: 100%;
    min-height: 44px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: inherit;
    font-weight: 400;
  }
  /* Scoped under .base-field for enough specificity to beat the shared dialog soft-focus
     (.base-dialog :focus-visible), which would otherwise add an outer glow halo. Keep just a
     clean focused border. */
  .base-field .base-input:focus,
  .base-field .base-input:focus-visible {
    outline: none;
    box-shadow: none;
    border-color: var(--selected-border);
  }
  .base-input:disabled {
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
