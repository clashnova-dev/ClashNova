<template>
  <header class="app-head" data-tauri-drag-region>
    <h1 data-tauri-drag-region>{{ t(title) }}</h1>
    <div class="app-head-right">
      <span class="pill status-chip" :class="phaseTone" role="status"
        ><span class="status-dot" aria-hidden="true" /><span>{{ statusChipText }}</span></span
      >
      <div v-if="enableLogin" class="account-wrap">
        <BaseButton class="account" @click="accountMenu = !accountMenu">
          <CircleUserRound :size="23" :stroke-width="1.6" /><span>{{ email || t("subscriptions.generic") }}</span
          ><ChevronDown :size="15" />
        </BaseButton>
        <div v-if="accountMenu" class="account-menu">
          <BaseButton
            @click="
              tab = 'sub';
              accountMenu = false;
            "
          >
            {{ t("subscriptions.manage") }}</BaseButton><BaseButton
            @click="
              logout();
              accountMenu = false;
            "
          >
            {{ localMode ? t("app.backToSignIn") : t("app.signOut") }}
          </BaseButton>
        </div>
      </div>
      <div v-if="isNative && !isMac" class="window-controls">
        <BaseButton :aria-label="t('common.minimize')" @click="windowAction('minimize')">
          <Minus :size="18" /></BaseButton
        ><BaseButton :aria-label="t('app.maximizeOrRestore')" @click="windowAction('toggleMaximize')">
          <Square :size="15" /></BaseButton
        ><BaseButton :aria-label="t('common.closeToTray')" @click="windowAction('close')">
          <X :size="18" />
        </BaseButton>
      </div>
    </div>
  </header>
</template>
<script setup lang="ts">
  import { Minus, X, Square, CircleUserRound, ChevronDown } from "@lucide/vue";
  import { useApp } from "../../composables/useApp";
  import BaseButton from "../../components/ui/BaseButton.vue";

  const {
    t,
    title,
    phaseTone,
    statusChipText,
    enableLogin,
    accountMenu,
    email,
    logout,
    localMode,
    isNative,
    isMac,
    windowAction,
    tab,
  } = useApp();
</script>
