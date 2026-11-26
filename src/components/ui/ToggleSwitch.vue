<script setup lang="ts">
  // Shared toggle-switch component. The root element keeps the global `.toggle-switch` class, so it both
  // reuses app.css styles and lets parents' existing `:deep(.toggle-switch)` / `.xxx .toggle-switch` overrides keep working.
  defineProps<{ modelValue: boolean; label: string; disabled?: boolean }>();
  const emit = defineEmits<{ "update:modelValue": [value: boolean] }>();
</script>

<template>
  <button
    type="button"
    class="toggle-switch"
    role="switch"
    :class="{ enabled: modelValue }"
    :aria-checked="modelValue"
    :aria-label="label"
    :disabled="disabled"
    @click="emit('update:modelValue', !modelValue)"
  >
    <span />
  </button>
</template>
