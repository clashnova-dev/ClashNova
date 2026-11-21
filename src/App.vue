<template>
  <LoginPage v-if="enableLogin && !access && !localMode" />
  <div v-else class="shell" :class="{ mac: isMac }">
    <aside class="nav">
      <div v-if="isMac" class="mac-traffic-space" data-tauri-drag-region aria-hidden="true" />
      <div class="logo"><img class="logo-img" :src="icon" /><span><strong>ClashNova</strong></span></div>
      <button v-for="item in navigation" :key="item.key" :class="{ on: tab === item.key || (item.key === 'settings' && tab === 'help') }" @click="tab = item.key"><component :is="item.icon" class="nav-glyph" :size="21" :stroke-width="1.8" />{{ tx(item.label) }}</button>
    </aside>
    <div class="workspace">
      <header class="app-head" data-tauri-drag-region><h1 data-tauri-drag-region>{{ t(title) }}</h1><div class="app-head-right"><span class="pill status-chip" :class="phaseTone" role="status"><span class="status-dot" aria-hidden="true" /><span>{{ statusChipText }}</span></span><div v-if="enableLogin" class="account-wrap"><button class="account" @click="accountMenu = !accountMenu"><CircleUserRound :size="23" :stroke-width="1.6" /><span>{{ email || t('subscriptions.generic') }}</span><ChevronDown :size="15" /></button><div v-if="accountMenu" class="account-menu"><button @click="tab = 'sub'; accountMenu = false">{{ t("subscriptions.manage") }}</button><button @click="logout(); accountMenu = false">{{ localMode ? t('app.backToSignIn') : t('app.signOut') }}</button></div></div><div v-if="isNative && !isMac" class="window-controls"><button :aria-label="t('common.minimize')" @click="windowAction('minimize')"><Minus :size="18" /></button><button :aria-label="t('app.maximizeOrRestore')" @click="windowAction('toggleMaximize')"><Square :size="15" /></button><button :aria-label="t('common.closeToTray')" @click="windowAction('close')"><X :size="18" /></button></div></div></header>
      <div v-if="guardBlocking" class="page-error" role="alert"><span>{{ t("tun.guardBlocking") }}</span><button class="page-error-action" :disabled="busy" @click="releaseGuard">{{ t("tun.guardRelease") }}</button></div>
      <main ref="mainContent" class="main">
        <Overview v-if="tab === 'home'" />
        <Nodes v-else-if="tab === 'nodes'" />
        <Subscriptions v-else-if="tab === 'sub'" />
        <ConnectionsPage v-else-if="tab === 'connections'" :locale="locale" :node-names="connectionNodeNames" />
        <Logs v-else-if="tab === 'logs'" />
        <NetworkCheck v-else-if="tab === 'network'" />
        <SettingsPage v-else-if="tab === 'settings'" />
        <HelpPage v-else :locale="locale" @back="tab = 'settings'" />
      </main>
    </div>
  </div>
  <dialog ref="profileDialog" class="subscription-dialog" @close="resetProfileDialog" aria-labelledby="add-profile-title" @cancel="busy && $event.preventDefault()" @click="!busy && $event.target === profileDialog && closeProfileDialog()">
    <div class="dialog-heading"><h2 id="add-profile-title">{{ t("subscriptions.add") }}</h2><button class="icon-button" :disabled="busy" :aria-label="t('app.closeAddSubscription')" @click="closeProfileDialog"><X :size="20" /></button></div>
    <p class="dialog-description">{{ t("app.addASubscriptionUrlOr") }}</p>
    <form class="profile-form" @submit.prevent="addExternal">
      <label>{{ t("app.subscriptionName") }}<input ref="profileNameInput" v-model.trim="newProfileName" :disabled="busy" :placeholder="t('app.forExampleEveryday')" maxlength="80" required /></label>
      <label>{{ t("app.subscriptionUrl") }}<input v-model.trim="newProfileUrl" :disabled="busy || !!selectedFile" type="url" placeholder="https://example.com/subscribe" /></label>
      <div class="file-import"><span>{{ t("app.orImportALocalFile") }}</span><label class="file-picker"><Plus :size="16" />{{ selectedFile?.name || t('app.chooseFile') }}<input :disabled="busy" type="file" accept=".json,.yaml,.yml,.txt,application/json,text/yaml,text/plain" @change="chooseFile" /></label><button v-if="selectedFile" type="button" class="link-button" @click="selectedFile = null">{{ t("app.useAUrlInstead") }}</button></div>
      <p class="hint">{{ t("app.supportsSingBoxJsonClash") }}</p>
      <p v-if="profileError" class="form-error" role="alert">{{ tx(profileError) }}</p>
      <div class="dialog-actions"><button type="button" :disabled="busy" @click="closeProfileDialog">{{ t("common.cancel") }}</button><button class="primary" :disabled="busy" type="submit">{{ busy ? t('app.importing') : t('app.importUse') }}</button></div>
    </form>
  </dialog>
  <dialog ref="importDialog" class="subscription-dialog link-confirm" aria-labelledby="import-link-title" @cancel="busy && $event.preventDefault()" @close="dismissImport" @click="!busy && $event.target === importDialog && dismissImport()">
    <div class="dialog-heading"><h2 id="import-link-title">{{ t("deepLink.importTitle") }}</h2><button class="icon-button" :disabled="busy" :aria-label="t('deepLink.close')" @click="dismissImport"><X :size="20" /></button></div>
    <p class="dialog-description">{{ importTarget ? t("deepLink.importUrlBody") : t("deepLink.importBody") }}</p>
    <div v-if="importTarget" class="import-target"><strong v-if="importTarget.name">{{ importTarget.name }}</strong><span>{{ importTarget.host }}</span></div>
    <p v-if="importError" class="form-error" role="alert">{{ tx(importError) }}</p>
    <div class="dialog-actions"><button type="button" :disabled="busy" @click="dismissImport">{{ t("common.cancel") }}</button><button class="primary" :disabled="busy" @click="confirmImport">{{ busy ? t('app.importing') : t('deepLink.confirmImport') }}</button></div>
  </dialog>
  <ConfirmationHost :locale="locale" />
  <NotificationHost :locale="locale" />
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { useApp } from "./composables/useApp";
import icon from "./assets/icon/favicon.png";
import ConnectionsPage from "./pages/ConnectionsPage.vue";
import HelpPage from "./pages/HelpPage.vue";
import NotificationHost from "./components/NotificationHost.vue";
import ConfirmationHost from "./components/ui/ConfirmationHost.vue";
import LoginPage from "./pages/LoginPage.vue";
import Overview from "./pages/OverviewPage.vue";
import Nodes from "./pages/NodesPage.vue";
import Subscriptions from "./pages/SubscriptionsPage.vue";
import Logs from "./pages/LogsPage.vue";
import NetworkCheck from "./pages/NetworkCheckPage.vue";
import SettingsPage from "./pages/SettingsPage.vue";
import { Minus, X, Square, CircleUserRound, ChevronDown, Plus } from "@lucide/vue";
const { enableLogin, locale, phaseTone, statusChipText, guardBlocking, releaseGuard, access, accountMenu, addExternal, busy, chooseFile, closeProfileDialog, importDialog, importError, importTarget, confirmImport, dismissImport, connectionNodeNames, email, isMac, isNative, label, localMode, logout, logs, mainContent, navigation, newProfileName, newProfileUrl, nodes, profileDialog, profileError, profileNameInput, resetProfileDialog, selectedFile, t, tx, tab, title, windowAction, bootstrap, teardown } = useApp();
onMounted(bootstrap);
onUnmounted(teardown);
</script>
<style src="./styles/app.css"></style>
