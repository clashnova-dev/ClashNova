<script setup lang="ts">
  // Unified button. Renders a native <button>; all styling lives in this file (scoped),
  // self-contained per variant. Defaults the type to "button" (a raw <button> defaults to
  // "submit" and can accidentally submit the surrounding form).
  defineOptions({ inheritAttrs: false });

  withDefaults(
    defineProps<{
      variant?: "default" | "primary" | "danger" | "ghost" | "link" | "icon";
      type?: "button" | "submit" | "reset";
      block?: boolean;
    }>(),
    { variant: "default", type: "button", block: false }
  );
</script>

<template>
  <button :type="type" :class="['base-button', `bb-${variant}`, { 'bb-block': block }]" v-bind="$attrs">
    <slot />
  </button>
</template>

<style scoped>
  /* Base look at ZERO specificity (:where) so a passed-through bespoke class or a
     contextual selector (e.g. .nav > button) can still override it after migration.
     Variant classes below keep normal specificity so they apply when chosen. */
  :where(.base-button) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-height: 40px;
    padding: 8px 14px;
    border: 1px solid var(--border);
    border-radius: 9px;
    color: var(--strong);
    background: var(--bg);
    cursor: pointer;
    transition:
      background 0.15s,
      color 0.15s,
      border-color 0.15s,
      transform 0.15s,
      box-shadow 0.15s;
  }
  :where(.base-button:hover:not(:disabled)) {
    background: var(--hover);
    border-color: var(--selected-border);
    color: var(--accent-text);
  }
  :where(.base-button:disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }
  :where(.base-button:focus-visible) {
    outline: 3px solid var(--focus);
    outline-offset: 3px;
  }
  .bb-block {
    width: 100%;
  }

  /* primary — the main call to action. */
  .bb-primary {
    border-color: var(--accent-text);
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .bb-primary:hover:not(:disabled) {
    background: var(--primary-hover);
    border-color: var(--accent-text);
    color: #fff;
  }

  /* danger — destructive actions (remove / delete). */
  .bb-danger {
    border-color: var(--danger-border);
    color: var(--danger);
  }
  .bb-danger:hover:not(:disabled) {
    background: var(--danger-bg);
    border-color: var(--danger-border);
    color: var(--danger);
  }

  /* ghost — borderless, for subtle inline actions. */
  .bb-ghost {
    background: transparent;
    border-color: transparent;
  }
  .bb-ghost:hover:not(:disabled) {
    background: var(--hover);
    border-color: transparent;
    color: var(--accent-text);
  }

  /* link — looks like inline text, no chrome. */
  .bb-link {
    gap: 5px;
    min-height: 28px;
    padding: 4px 0;
    border: 0;
    background: none;
    color: var(--accent-text);
    font-size: 14px;
    white-space: nowrap;
  }
  .bb-link:hover:not(:disabled) {
    background: none;
    border: 0;
    text-decoration: underline;
  }

  /* icon — square icon-only button. */
  .bb-icon {
    flex: none;
    width: 36px;
    min-height: 36px;
    padding: 8px;
  }
</style>
