<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { createAppIconCache } from "../lib/app-icon-cache";

  // Module scope survives component unmounts and is shared by the list and popover.
  const cachedIcon = createAppIconCache(async path => {
    const bytes = await invoke<number[] | null>("application_icon", { app: path });
    return bytes?.length ? new Blob([new Uint8Array(bytes)], { type: "image/png" }) : null;
  });
</script>
<script setup lang="ts">
  import { ref, watch, onUnmounted } from "vue";
  import { AppWindow } from "@lucide/vue";
  const props = defineProps<{ path?: string }>();
  const src = ref("");
  let generation = 0;
  let alive = true;
  function clear() {
    if (src.value) URL.revokeObjectURL(src.value);
    src.value = "";
  }
  watch(
    () => props.path,
    async path => {
      const run = ++generation;
      clear();
      if (!path || !("__TAURI_INTERNALS__" in window)) return;
      try {
        const icon = await cachedIcon(path);
        if (alive && run === generation && icon) src.value = URL.createObjectURL(icon);
      } catch {
        /* Unavailable icons keep the generic application symbol. */
      }
    },
    { immediate: true }
  );
  onUnmounted(() => {
    alive = false;
    generation++;
    clear();
  });
</script>
<template>
  <span class="app-icon"
    ><img v-if="src" :src="src" alt="" @error="clear" /><AppWindow v-else :size="17" aria-hidden="true"
  /></span>
</template>
<style lang="scss" scoped>
  .app-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    color: var(--muted);
  }
  .app-icon img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
</style>
