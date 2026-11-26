<template>
  <div class="shell" :class="{ mac: isMac }">
    <AppSidebar />
    <div class="workspace">
      <AppHeader />
      <AppMain />
    </div>
  </div>
</template>
<script setup lang="ts">
  import { useApp } from "../composables/useApp.ts";
  import AppSidebar from "./sidebar/index.vue";
  import AppHeader from "./header/index.vue";
  import AppMain from "./main/index.vue";

  const { isMac } = useApp();
</script>
