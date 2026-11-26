<template>
  <aside class="nav">
    <div v-if="isMac" class="mac-traffic-space" data-tauri-drag-region aria-hidden="true" />
    <div class="logo">
      <img class="logo-img" :src="icon" /><span><strong>ClashNova</strong></span>
    </div>
    <BaseButton
      v-for="item in navigation"
      :key="item.key"
      :class="{ on: tab === item.key || (item.key === 'settings' && tab === 'help') }"
      :title="tx(item.label)"
      @click="tab = item.key"
    >
      <component :is="item.icon" class="nav-glyph" :size="21" :stroke-width="1.8" /><span class="nav-label">{{
        tx(item.label)
      }}</span>
    </BaseButton>
  </aside>
</template>
<script setup lang="ts">
  import { useApp } from "../../composables/useApp";
  import icon from "../../assets/icon/favicon.png";
  import BaseButton from "../../components/ui/BaseButton.vue";

  const { isMac, navigation, tab, tx } = useApp();
</script>
