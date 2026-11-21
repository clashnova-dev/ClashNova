<script setup lang="ts">
import { useApp } from "../composables/useApp";
import { Minus, X } from "@lucide/vue";
const { t, tx, isMac, isNative, windowAction, email, password, rememberMe, loginError, busy, login, enterLocal } = useApp();
</script>
<template>
  <div class="login-page" :class="{ mac: isMac }">
    <!-- No title bar: this strip is the drag handle and leaves room for the macOS window buttons. -->
    <div class="login-drag" data-tauri-drag-region><div v-if="isNative && !isMac" class="window-controls"><button :aria-label="t('common.minimize')" @click="windowAction('minimize')"><Minus :size="18" /></button><button :aria-label="t('common.closeToTray')" @click="windowAction('close')"><X :size="18" /></button></div></div>
    <div class="login-body">
      <div class="login-card">
        <h1>{{ t("login.signInToAirport") }}</h1>
        <p>{{ t("login.signInToManageSubscriptions") }}</p>
        <form @submit.prevent="login">
          <label>{{ t("login.email") }}<input v-model.trim="email" type="email" required autocomplete="username" placeholder="name@example.com" /></label>
          <label>{{ t("login.password") }}<input v-model="password" type="password" required autocomplete="current-password" :placeholder="t('login.enterYourPassword')" /></label>
          <label class="remember-row"><input v-model="rememberMe" type="checkbox" />{{ t("login.rememberMe") }}</label>
          <p v-if="loginError" class="form-error" role="alert">{{ tx(loginError) }}</p>
          <button class="primary login-submit" :disabled="busy" type="submit">{{ busy ? t("login.signingIn") : t("login.signIn") }}</button>
        </form>
        <button class="local-entry" @click="enterLocal">{{ t("login.useASubscription") }}</button>
      </div>
    </div>
  </div>
</template>
<style scoped>
/* In the app the login window is exactly this card (see LOGIN_WINDOW in useApp.ts),
   so the form fills the window. Wider viewports, such as a browser preview, get a framed card.
   Auto margins centre it, so a viewport shorter than the form scrolls from the top. */
.login-page { height:100vh; display:flex; flex-direction:column; background:var(--bg); }
.login-drag { flex:none; height:44px; display:flex; justify-content:flex-end; }
.login-body { flex:1; min-height:0; display:flex; overflow:auto; scrollbar-width:thin; scrollbar-color:var(--scrollbar) transparent; }
.login-card { width:100%; margin:auto 0; padding:4px 28px 28px; }
.login-card h1 { margin:0; font-size:25px; line-height:32px; letter-spacing:-.025em; }
.login-card > p { margin:6px 0 12px; color:var(--muted); font-size:15px; }
.login-card form { display:grid; gap:12px; }
.login-card label { display:grid; gap:6px; font-size:15px; font-weight:600; }
.login-card input { width:100%; min-height:40px; padding:8px 12px; border:1px solid var(--border); border-radius:8px; background:var(--bg); color:inherit; font-weight:400; }
.login-card .remember-row { display:flex; align-items:center; gap:8px; font-size:14px; font-weight:400; cursor:pointer; }
.login-card .remember-row input { width:16px; height:16px; min-height:0; padding:0; accent-color:var(--accent); cursor:pointer; }
.login-submit { min-height:42px; }
.local-entry { width:100%; margin-top:10px; }
@media(min-width:560px) {
  .login-page { background:var(--surface); }
  .login-body { padding:20px 24px; }
  .login-card { width:420px; margin:auto; padding:22px 30px 24px; border:1px solid var(--line); border-radius:16px; background:var(--bg); box-shadow:var(--shadow-card); }
}
</style>
