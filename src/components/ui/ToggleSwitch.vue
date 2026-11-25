<script setup lang="ts">
// 统一的开关组件。根元素保留全局 `.toggle-switch` 类，既复用 app.css 的样式，
// 也让父组件既有的 `:deep(.toggle-switch)` / `.xxx .toggle-switch` 覆写继续生效。
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
