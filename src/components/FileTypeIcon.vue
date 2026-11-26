<script setup lang="ts">
  import { computed } from "vue";
  import { fileType } from "../lib/file-types";
  const props = withDefaults(defineProps<{ name: string; size?: number }>(), { size: 28 });
  const type = computed(() => fileType(props.name));
</script>
<template>
  <svg
    class="file-type-icon"
    :class="type.kind"
    :width="size"
    :height="size * 1.25"
    viewBox="0 0 32 40"
    fill="none"
    aria-hidden="true"
    focusable="false"
  >
    <path
      d="M6 1.5h13l7.5 7.5v27A2.5 2.5 0 0 1 24 38.5H6A2.5 2.5 0 0 1 3.5 36V4A2.5 2.5 0 0 1 6 1.5Z"
      fill="currentColor"
      fill-opacity=".07"
      stroke="currentColor"
      stroke-width="1.4"
      stroke-linejoin="round"
    />
    <path
      d="M19 1.5V9h7.5M9 15h12M9 18h9"
      stroke="currentColor"
      stroke-width="1.3"
      stroke-linecap="round"
      stroke-linejoin="round"
      opacity=".55"
    />
    <text
      x="15"
      y="29"
      text-anchor="middle"
      fill="currentColor"
      font-size="7"
      font-weight="650"
      font-family="-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif"
    >
      {{ type.label }}
    </text>
  </svg>
</template>
<style lang="scss" scoped>
  .file-type-icon {
    display: block;
    flex: none;
    color: var(--muted);
  }
  .pdf {
    color: var(--danger);
  }
  .document,
  .code {
    color: var(--accent-text);
  }
  .sheet,
  .audio {
    color: var(--success);
  }
  .slides,
  .archive {
    color: var(--warn);
  }
  .image,
  .video {
    color: var(--strong);
  }
</style>
