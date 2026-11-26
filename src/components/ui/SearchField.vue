<script setup lang="ts">
  import { computed, useId } from "vue";
  import { Search, X, CaseSensitive, WholeWord, Regex } from "@lucide/vue";
  import { translator, type Locale } from "../../i18n";
  import type { SearchOptions } from "../../lib/connection-search";
  const props = defineProps<{
    modelValue: string;
    options: SearchOptions;
    locale: Locale;
    label: string;
    placeholder: string;
    invalid?: boolean;
  }>();
  const emit = defineEmits<{
    "update:modelValue": [value: string];
    "update:options": [value: SearchOptions];
  }>();
  const { t } = translator(() => props.locale),
    errorId = useId();
  const modes = [
    { key: "caseSensitive", icon: CaseSensitive, label: "connections.caseSensitive" },
    { key: "wholeWord", icon: WholeWord, label: "connections.wholeWord" },
    { key: "regex", icon: Regex, label: "connections.regex" },
  ] as const;
  const invalid = computed(() => props.invalid === true);
</script>
<template>
  <div class="search-field" :class="{ invalid }">
    <Search :size="16" aria-hidden="true" />
    <input
      :value="modelValue"
      :aria-label="label"
      :placeholder="placeholder"
      :aria-invalid="invalid"
      :aria-describedby="invalid ? errorId : undefined"
      @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    />
    <button
      v-if="modelValue"
      type="button"
      :aria-label="t('common.clearSearch')"
      :title="t('common.clearSearch')"
      @click="emit('update:modelValue', '')"
    >
      <X :size="14" />
    </button>
    <div class="search-modes">
      <button
        v-for="mode in modes"
        :key="mode.key"
        type="button"
        :class="{ on: options[mode.key] }"
        :aria-pressed="options[mode.key]"
        :aria-label="t(mode.label)"
        :title="t(mode.label)"
        @click="emit('update:options', { ...options, [mode.key]: !options[mode.key] })"
      >
        <component :is="mode.icon" :size="17" />
      </button>
    </div>
    <span v-if="invalid" :id="errorId" class="sr-only">{{ t("connections.invalidRegex") }}</span>
  </div>
</template>
<style scoped>
  .search-field {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 210px;
    flex: 0 1 420px;
    width: 420px;
    max-width: 100%;
    height: 32px;
    padding: 0 5px 0 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    color: var(--muted);
  }
  .search-field:focus-within {
    border-color: var(--accent);
  }
  .search-field input {
    flex: 1;
    width: 0;
    min-width: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font-size: 13px;
  }
  .search-field input::placeholder {
    color: var(--muted);
  }
  .search-field button {
    padding: 0;
    min-height: 24px;
    width: 24px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
  }
  .search-modes {
    display: flex;
    flex: none;
    gap: 1px;
  }
  .search-modes button.on {
    color: var(--accent-text);
    background: var(--selected);
  }
  .search-field.invalid {
    border-color: var(--danger);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
