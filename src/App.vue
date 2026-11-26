<template>
  <LoginView v-if="enableLogin && !access && !localMode" />
  <Layout v-else />
  <AppDialogs />
  <ConfirmationHost :locale="locale" />
  <NotificationHost :locale="locale" />
</template>

<script setup lang="ts">
  import { onMounted, onUnmounted } from "vue";
  import { useApp } from "./composables/useApp";
  import Layout from "./layout/index.vue";
  import AppDialogs from "./components/AppDialogs.vue";
  import LoginView from "./pages/login/index.vue";
  import NotificationHost from "./components/NotificationHost.vue";
  import ConfirmationHost from "./components/ui/ConfirmationHost.vue";

  const { enableLogin, access, localMode, locale, bootstrap, teardown } = useApp();
  onMounted(() => {
    void bootstrap();
    // On launch the window takes keyboard focus, which Chromium shows as a :focus-visible ring on
    // the first nav item. Drop that initial focus so there's no stray blue outline before any input.
    requestAnimationFrame(() => (document.activeElement as HTMLElement | null)?.blur?.());
  });
  onUnmounted(teardown);
</script>
<style src="./styles/app.scss" lang="scss"></style>
