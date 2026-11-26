<script setup lang="ts">
  import { useApp } from "../../composables/useApp";
  import { Minus, X } from "@lucide/vue";
  import BaseButton from "../../components/ui/BaseButton.vue";
  const { t, tx, isMac, isNative, windowAction, email, password, rememberMe, loginError, busy, login, enterLocal } =
    useApp();
</script>
<template>
  <div class="login-page" :class="{ mac: isMac }">
    <!-- No title bar: this strip is the drag handle and leaves room for the macOS window buttons. -->
    <div class="login-drag" data-tauri-drag-region>
      <div v-if="isNative && !isMac" class="window-controls">
        <BaseButton :aria-label="t('common.minimize')" @click="windowAction('minimize')">
          <Minus :size="18" /></BaseButton
        ><BaseButton :aria-label="t('common.closeToTray')" @click="windowAction('close')">
          <X :size="18" />
        </BaseButton>
      </div>
    </div>
    <div class="login-body">
      <div class="login-card">
        <h1>{{ t("login.signInToAirport") }}</h1>
        <p>{{ t("login.signInToManageSubscriptions") }}</p>
        <form @submit.prevent="login">
          <label
            >{{ t("login.email")
            }}<input v-model.trim="email" type="email" required autocomplete="username" placeholder="name@example.com"
          /></label>
          <label
            >{{ t("login.password")
            }}<input
              v-model="password"
              type="password"
              required
              autocomplete="current-password"
              :placeholder="t('login.enterYourPassword')"
          /></label>
          <label class="remember-row"><input v-model="rememberMe" type="checkbox" />{{ t("login.rememberMe") }}</label>
          <p v-if="loginError" class="form-error" role="alert">{{ tx(loginError) }}</p>
          <BaseButton variant="primary" class="login-submit" :disabled="busy" type="submit">
            {{ busy ? t("login.signingIn") : t("login.signIn") }}
          </BaseButton>
        </form>
        <BaseButton class="local-entry" @click="enterLocal">{{ t("login.useASubscription") }}</BaseButton>
      </div>
    </div>
  </div>
</template>
<style scoped lang="scss" src="./index.scss"></style>
