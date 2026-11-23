<script setup lang="ts">
import { nextTick, ref, useId } from "vue";
import { CircleHelp } from "@lucide/vue";

// A "?" after a label that shows its explanation on hover or keyboard focus.
defineProps<{ text: string; inline?: boolean }>();
const id = useId();
const open = ref(false);
const anchor = ref<HTMLElement | null>(null);
const bubble = ref<HTMLElement | null>(null);
const position = ref<{ left: string; top: string }>({ left: "0px", top: "0px" });

// The bubble is fixed to the viewport (and teleported) so a scrolling page cannot clip it;
// it opens below the icon and flips above when there is no room.
async function show() {
  open.value = true;
  await nextTick();
  const icon = anchor.value?.getBoundingClientRect(), box = bubble.value?.getBoundingClientRect();
  if (!icon || !box) return;
  const margin = 8;
  const left = Math.min(Math.max(margin, icon.left + icon.width / 2 - box.width / 2), window.innerWidth - box.width - margin);
  const fitsBelow = icon.bottom + 6 + box.height <= window.innerHeight - margin;
  position.value = { left: `${Math.round(left)}px`, top: `${Math.round(fitsBelow ? icon.bottom + 6 : icon.top - 6 - box.height)}px` };
}
</script>
<template>
  <span class="help-tip">
    <button ref="anchor" type="button" class="help-tip-button" :aria-label="text" :aria-describedby="open ? id : undefined" @mouseenter="show" @mouseleave="open = false" @focus="show" @blur="open = false" @keydown.esc.stop.prevent="open = false"><CircleHelp :size="15" /></button>
    <Teleport to="body" :disabled="inline"><span v-if="open" :id="id" ref="bubble" class="help-tip-bubble" role="tooltip" :style="position">{{ text }}</span></Teleport>
  </span>
</template>
<style scoped>
.help-tip { display:inline-flex; margin-left:6px; vertical-align:middle; }
.help-tip-button { width:20px; height:20px; min-height:0; padding:0; border:0; border-radius:50%; background:transparent; color:var(--muted); cursor:help; }
.help-tip-button:hover:not(:disabled),.help-tip-button:focus-visible { background:transparent; color:var(--accent-text); }
.help-tip-bubble { position:fixed; z-index:100; max-width:280px; padding:8px 11px; border-radius:10px; background:var(--text); color:var(--bg); font-size:13px; font-weight:400; line-height:1.55; box-shadow:0 8px 22px #14243b33; pointer-events:none; }
</style>
