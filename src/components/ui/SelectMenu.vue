<script setup lang="ts" generic="T extends string">
  import { computed, onBeforeUnmount, ref, useId, watch, type Component } from "vue";
  import { Check, ChevronDown } from "@lucide/vue";

  // A select drawn by the app instead of the system, so it matches the rest of the UI in both themes.
  const props = defineProps<{
    modelValue: T;
    options: { value: T; label: string; meta?: string }[];
    label: string;
    icon?: Component;
    disabled?: boolean;
  }>();
  const emit = defineEmits<{ "update:modelValue": [value: T] }>();
  const id = useId();
  const open = ref(false);
  const active = ref(0);
  const root = ref<HTMLElement | null>(null);
  const trigger = ref<HTMLButtonElement | null>(null);
  const current = computed(() => props.options.find(option => option.value === props.modelValue));

  function show() {
    if (props.disabled) return;
    active.value = Math.max(
      0,
      props.options.findIndex(option => option.value === props.modelValue)
    );
    open.value = true;
  }
  function close(refocus = false) {
    open.value = false;
    if (refocus) trigger.value?.focus();
  }
  function choose(value: T) {
    if (props.disabled) return;
    if (value !== props.modelValue) emit("update:modelValue", value);
    close(true);
  }
  function onKeydown(event: KeyboardEvent) {
    if (props.disabled) return;
    const count = props.options.length;
    if (!open.value) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) {
        event.preventDefault();
        show();
      }
      return;
    }
    if (event.key === "Tab") {
      close();
      return;
    }
    const move: Record<string, number> = {
      ArrowDown: (active.value + 1) % count,
      ArrowUp: (active.value - 1 + count) % count,
      Home: 0,
      End: count - 1,
    };
    if (event.key in move) {
      event.preventDefault();
      active.value = move[event.key];
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(props.options[active.value].value);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close(true);
    }
  }
  function onOutside(event: PointerEvent) {
    if (!root.value?.contains(event.target as Node)) close();
  }
  watch(open, value =>
    document[value ? "addEventListener" : "removeEventListener"]("pointerdown", onOutside as EventListener, true)
  );
  watch(
    () => props.disabled,
    value => {
      if (value) close();
    }
  );
  onBeforeUnmount(() => document.removeEventListener("pointerdown", onOutside as EventListener, true));
</script>
<template>
  <div ref="root" class="select-menu" :class="{ open }">
    <button
      ref="trigger"
      type="button"
      class="select-trigger"
      role="combobox"
      :disabled="disabled"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-controls="id"
      :aria-label="label"
      :aria-activedescendant="open ? `${id}-${active}` : undefined"
      @click="open ? close() : show()"
      @keydown="onKeydown"
    >
      <component :is="icon" v-if="icon" class="select-icon" :size="16" />
      <span class="select-value">{{ current?.label }}</span>
      <ChevronDown class="select-chevron" :size="15" />
    </button>
    <ul v-if="open" :id="id" class="select-list" role="listbox" :aria-label="label">
      <li
        v-for="(option, index) in options"
        :id="`${id}-${index}`"
        :key="option.value"
        role="option"
        :aria-selected="option.value === modelValue"
        :class="{ active: index === active, chosen: option.value === modelValue }"
        @pointerenter="active = index"
        @click="choose(option.value)"
      >
        <span class="select-label">{{ option.label }}</span><span v-if="option.meta" class="select-meta">{{
          option.meta
        }}</span
        ><Check v-if="option.value === modelValue" :size="15" :stroke-width="2.4" />
      </li>
    </ul>
  </div>
</template>
<style scoped>
  .select-menu {
    position: relative;
    flex: none;
    min-width: 150px;
  }
  .select-trigger {
    width: 100%;
    height: 40px;
    min-height: 40px;
    justify-content: flex-start;
    gap: 8px;
    padding: 0 10px 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--strong);
    font-size: 14px;
  }
  .select-trigger:hover:not(:disabled) {
    background: var(--bg);
    border-color: var(--selected-border);
    color: var(--strong);
  }
  .select-trigger:focus-visible,
  .select-menu.open .select-trigger {
    outline: none;
    border-color: var(--selected-border);
  }
  .select-icon {
    flex: none;
    color: var(--muted);
  }
  .select-value {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .select-chevron {
    flex: none;
    color: var(--muted);
    transition: transform 0.15s ease;
  }
  .open .select-chevron {
    transform: rotate(180deg);
  }
  .select-list {
    position: absolute;
    z-index: 30;
    top: calc(100% + 6px);
    right: 0;
    min-width: 100%;
    max-height: 240px;
    overflow-y: auto;
    margin: 0;
    padding: 5px;
    list-style: none;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--bg);
    box-shadow: 0 12px 28px #14243b24;
  }
  .select-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    min-height: 34px;
    padding: 6px 10px;
    border-radius: 8px;
    color: var(--text);
    font-size: 14px;
    white-space: nowrap;
    cursor: pointer;
  }
  .select-list li .select-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .select-list li .select-meta {
    flex: none;
    color: var(--muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .select-list li.active {
    background: var(--hover);
  }
  .select-list li.chosen {
    color: var(--accent-text);
    font-weight: 600;
  }
  .select-list li svg {
    flex: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .select-chevron {
      transition: none;
    }
  }
</style>
