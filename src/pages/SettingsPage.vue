<script setup lang="ts">
import { confirmation } from "../lib/confirmation";
import { useApp } from "../composables/useApp";
import LanProxyAddresses from "../components/settings/LanProxyAddresses.vue";
import TunServiceSettings from "../components/settings/TunServiceSettings.vue";
import ShortcutSettings from "../components/settings/ShortcutSettings.vue";
import SelectMenu from "../components/ui/SelectMenu.vue";
import HelpTip from "../components/ui/HelpTip.vue";
import { ArrowRight, CircleHelp, ExternalLink, Monitor, Sun, Moon } from "@lucide/vue";
import appIcon from "../assets/icon/favicon.png";
const { t, tx, tab, locale, theme, showTraySpeed, probeConcurrency, isMac, busy, connected, connectionMode, setConnectionMode, routeMode, setRouteMode, proxyDiagnostic, diagnosticBusy, refreshProxyDiagnostics, localProxyAddress, draftAllowLan, draftProxyPort, draftTun, draftTunMtu, draftTunExclude, isLinux, networkLoaded, networkSettings, networkDirty, networkError, saveNetworkSettings, autostartEnabled, autostartBusy, autostartReady, autostartError, toggleAutostart, installedVersion, updateOffer, updateStatus, installingUpdate, checkingUpdates, checkUpdates, installUpdate, githubUrl, openExternal } = useApp();
async function changeProxy(value: "system" | "tun") {
  const previous = connectionMode.value;
  if (previous === value || busy.value) return;
  if (!await confirmation.request({ title: "confirmation.proxyTitle", body: "confirmation.proxyBody" })) return;
  if (!busy.value && connectionMode.value === previous) await setConnectionMode(value);
}
async function changeRoute(value: "rule" | "global") {
  const previous = routeMode.value;
  if (previous === value || busy.value) return;
  if (!await confirmation.request({ title: "confirmation.routingTitle", body: "confirmation.routingBody" })) return;
  if (!busy.value && routeMode.value === previous) await setRouteMode(value);
}
</script>
<template>
  <div class="settings-page">
    <section class="settings-card" :aria-label="t('settings.appearanceLanguage')">
      <h2>{{ t("settings.appearanceLanguage") }}</h2>
      <div class="setting-row"><span>{{ t("settings.language") }}</span><SelectMenu v-model="locale" :label="t('settings.interfaceLanguage')" :options="[{ value: 'en', label: 'English' }, { value: 'ja', label: t('settings.languageJapanese') }, { value: 'ko', label: '한국어' }, { value: 'de', label: 'Deutsch' }, { value: 'fr', label: 'Français' }, { value: 'ru', label: 'Русский' }, { value: 'zh-CN', label: t('settings.languageChinese') }]" /></div>
      <div class="setting-row"><span>{{ t("settings.theme") }}</span><div class="seg" role="group" :aria-label="t('settings.theme')"><button type="button" :aria-pressed="theme === 'system'" :class="{ chosen: theme === 'system' }" @click="theme = 'system'"><Monitor :size="14" />{{ t("settings.themeSystem") }}</button><button type="button" :aria-pressed="theme === 'light'" :class="{ chosen: theme === 'light' }" @click="theme = 'light'"><Sun :size="14" />{{ t("settings.themeLight") }}</button><button type="button" :aria-pressed="theme === 'dark'" :class="{ chosen: theme === 'dark' }" @click="theme = 'dark'"><Moon :size="14" />{{ t("settings.themeDark") }}</button></div></div>
      <div class="setting-row"><span>{{ t("settings.showLiveSpeed") }}<HelpTip :text="t('settings.showUploadAndDownloadSpeed')" /></span><button type="button" role="switch" :aria-checked="showTraySpeed" :aria-label="t('settings.showLiveSpeed')" class="toggle-switch" :class="{ enabled: showTraySpeed }" @click="showTraySpeed = !showTraySpeed"><span /></button></div>
      <div class="setting-row"><span><label for="probe-concurrency">{{ t("settings.testConcurrency") }}</label><HelpTip :text="t('settings.testConcurrencyHelp')" /></span><input id="probe-concurrency" v-model.number="probeConcurrency" @change="probeConcurrency = Math.min(64, Math.max(1, Math.round(Number(probeConcurrency) || 8)))" type="number" min="1" max="64" step="1" inputmode="numeric" autocomplete="off" class="port-input" /></div>
    </section>
    <section class="settings-card" :aria-label="t('settings.connectionSettings')">
      <h2>{{ t("settings.connectionSettings") }}</h2>
      <div class="setting-row"><span>{{ t("common.proxyHandling") }}<HelpTip :text="t('settings.systemProxyCannotCaptureWebrtc')" /></span><div class="seg" role="group" :aria-label="t('common.proxyHandling')"><button type="button" :aria-pressed="connectionMode === 'system'" :class="{ chosen: connectionMode === 'system' }" @click="changeProxy('system')">{{ t("common.systemProxy") }}</button><button type="button" :aria-pressed="connectionMode === 'tun'" :class="{ chosen: connectionMode === 'tun' }" @click="changeProxy('tun')">TUN</button></div></div>
      <TunServiceSettings v-if="connectionMode === 'tun'" :locale="locale" :disabled="busy || connected" />
      <div class="setting-row"><span>{{ t("common.routing") }}</span><div class="seg" role="group" :aria-label="t('common.routing')"><button type="button" :aria-pressed="routeMode === 'rule'" :class="{ chosen: routeMode === 'rule' }" @click="changeRoute('rule')">{{ t("settings.ruleMode") }}</button><button type="button" :aria-pressed="routeMode === 'global'" :class="{ chosen: routeMode === 'global' }" @click="changeRoute('global')">{{ t("settings.globalMode") }}</button></div></div>
      <div class="setting-row"><span>{{ t("settings.systemProxyCheck") }}<small class="setting-description" role="status">{{ tx(proxyDiagnostic) }}</small></span><button type="button" :disabled="diagnosticBusy || busy" @click="refreshProxyDiagnostics">{{ t("settings.checkAgain") }}</button></div>
      <div class="setting-row"><span>{{ t("settings.localProxy") }}</span><strong>{{ localProxyAddress }}</strong></div>
    </section>
      <form class="network-settings-form" @submit.prevent="saveNetworkSettings">
        <section class="settings-card" :aria-label="t('settings.localAccess')">
          <h2>{{ t('settings.localAccess') }}</h2>
      <div class="setting-row"><span>{{ t("settings.allowLan") }}<HelpTip :text="t('settings.allowOtherDevicesOnYour')" /></span><button type="button" role="switch" :aria-checked="draftAllowLan" :aria-label="t('settings.allowLan')" class="toggle-switch" :class="{ enabled: draftAllowLan }" :disabled="busy || connected || !networkLoaded" @click="draftAllowLan = !draftAllowLan"><span /></button></div>
          <LanProxyAddresses v-if="draftAllowLan" :port="draftProxyPort" :pending="networkDirty" />
        <div class="setting-row"><span><label for="proxy-port">{{ t("settings.proxyPort") }}</label><HelpTip :text="t('settings.sharedHttpAndSocksPort')" /></span><input id="proxy-port" v-model="draftProxyPort" type="text" inputmode="numeric" autocomplete="off" class="port-input" :disabled="busy || connected || !networkLoaded" /></div>
        </section>
        <template v-if="connectionMode === 'tun'">
          <section class="settings-card" :aria-label="t('settings.tunOptions')">
            <h2>{{ t('settings.tunOptions') }}</h2>
          <div class="setting-row"><span>{{ t("settings.tunStack") }}<HelpTip :text="t('settings.tunStackHelp')" /></span><div class="seg" role="group" :aria-label="t('settings.tunStack')"><button type="button" :aria-pressed="draftTun.stack === 'mixed'" :class="{ chosen: draftTun.stack === 'mixed' }" :disabled="busy || connected || !networkLoaded" @click="draftTun.stack = 'mixed'">{{ t("settings.tunStackMixed") }}</button><button type="button" :aria-pressed="draftTun.stack === 'system'" :class="{ chosen: draftTun.stack === 'system' }" :disabled="busy || connected || !networkLoaded" @click="draftTun.stack = 'system'">{{ t("settings.tunStackSystem") }}</button><button type="button" :aria-pressed="draftTun.stack === 'gvisor'" :class="{ chosen: draftTun.stack === 'gvisor' }" :disabled="busy || connected || !networkLoaded" @click="draftTun.stack = 'gvisor'">gVisor</button></div></div>
          <div class="setting-row"><span><label for="tun-mtu">{{ t("settings.tunMtu") }}</label><HelpTip :text="t('settings.tunMtuHelp')" /></span><input id="tun-mtu" v-model="draftTunMtu" type="text" inputmode="numeric" autocomplete="off" class="port-input" :disabled="busy || connected || !networkLoaded" /></div>
          <div class="setting-row"><span>{{ t("settings.tunStrictRoute") }}<HelpTip :text="t('settings.tunStrictRouteHelp')" /></span><button type="button" role="switch" :aria-checked="draftTun.strictRoute" :aria-label="t('settings.tunStrictRoute')" class="toggle-switch" :class="{ enabled: draftTun.strictRoute }" :disabled="busy || connected || !networkLoaded" @click="draftTun.strictRoute = !draftTun.strictRoute"><span /></button></div>
          <div class="setting-row"><span>{{ t("settings.tunKillSwitch") }}<HelpTip :text="t('settings.tunKillSwitchHelp')" /></span><button type="button" role="switch" :aria-checked="draftTun.killSwitch" :aria-label="t('settings.tunKillSwitch')" class="toggle-switch" :class="{ enabled: draftTun.killSwitch }" :disabled="busy || connected || !networkLoaded" @click="draftTun.killSwitch = !draftTun.killSwitch"><span /></button></div>
          <div class="setting-row"><span>{{ t("settings.tunIpv6") }}<HelpTip :text="t('settings.tunIpv6Help')" /></span><button type="button" role="switch" :aria-checked="draftTun.ipv6" :aria-label="t('settings.tunIpv6')" class="toggle-switch" :class="{ enabled: draftTun.ipv6 }" :disabled="busy || connected || !networkLoaded" @click="draftTun.ipv6 = !draftTun.ipv6"><span /></button></div>
          <div v-if="isLinux" class="setting-row"><span>{{ t("settings.tunAutoRedirect") }}<HelpTip :text="t('settings.tunAutoRedirectHelp')" /></span><button type="button" role="switch" :aria-checked="draftTun.autoRedirect" :aria-label="t('settings.tunAutoRedirect')" class="toggle-switch" :class="{ enabled: draftTun.autoRedirect }" :disabled="busy || connected || !networkLoaded" @click="draftTun.autoRedirect = !draftTun.autoRedirect"><span /></button></div>
          <div class="setting-row tun-exclude-row"><span><label for="tun-exclude">{{ t("settings.tunExclude") }}</label><HelpTip :text="t('settings.tunExcludeHelp')" /></span><textarea id="tun-exclude" v-model="draftTunExclude" rows="3" spellcheck="false" autocomplete="off" class="exclude-input" placeholder="192.168.0.0/16" :disabled="busy || connected || !networkLoaded" /></div>

          </section>
          <section class="settings-card" :aria-label="t('settings.dnsSettings')">
            <h2>{{ t('settings.dnsSettings') }}</h2>
          <div class="setting-row"><span>{{ t("settings.dnsMode") }}<HelpTip :text="t('settings.dnsModeHelp')" /></span><div class="seg" role="group" :aria-label="t('settings.dnsMode')"><button type="button" :aria-pressed="draftTun.dnsMode === 'fakeip'" :class="{ chosen: draftTun.dnsMode === 'fakeip' }" :disabled="busy || connected || !networkLoaded" @click="draftTun.dnsMode = 'fakeip'">{{ t("settings.dnsFakeip") }}</button><button type="button" :aria-pressed="draftTun.dnsMode === 'remote'" :class="{ chosen: draftTun.dnsMode === 'remote' }" :disabled="busy || connected || !networkLoaded" @click="draftTun.dnsMode = 'remote'">{{ t("settings.dnsReal") }}</button></div></div>
          <div class="setting-row"><span>{{ t("settings.dnsDomestic") }}</span><SelectMenu :disabled="busy || connected || !networkLoaded" v-model="draftTun.dnsDomestic" :label="t('settings.dnsDomestic')" :options="[{ value: 'local', label: t('settings.dnsLocal') }, { value: 'ali', label: 'AliDNS' }, { value: 'dnspod', label: 'DNSPod' }]" /></div>
          <div v-if="draftTun.dnsMode === 'remote'" class="setting-row"><span>{{ t("settings.dnsRemote") }}</span><SelectMenu :disabled="busy || connected || !networkLoaded" v-model="draftTun.dnsRemote" :label="t('settings.dnsRemote')" :options="[{ value: 'cloudflare', label: 'Cloudflare' }, { value: 'google', label: 'Google' }, { value: 'quad9', label: 'Quad9' }]" /></div>

          </section>
        </template>
        <div class="network-save-bar">
        <div class="network-settings-actions"><span class="muted">{{ connected ? t("settings.disconnectToChangeNetworkSettings") : networkSettings.allowLan ? t("settings.lanDevicesUseThisComputers") : t("settings.onlyThisComputerCanAccess") }}</span><button type="submit" :disabled="busy || connected || !networkLoaded || !networkDirty">{{ t("settings.saveNetworkSettings") }}</button></div>
        <p v-if="networkError" class="form-error" role="alert">{{ tx(networkError) }}</p>

        </div>
      </form>
    <section class="settings-card" :aria-label="t('common.keyboardShortcuts')">
      <ShortcutSettings :locale="locale" :is-mac="isMac" />
    </section>
    <section class="settings-card" :aria-label="t('settings.startup')">
      <h2>{{ t("settings.startup") }}</h2>
      <div class="setting-row"><span>{{ t("settings.launchAtLogin") }}<HelpTip :text="t('settings.startAirportWhenYouLog')" /></span><button type="button" role="switch" :aria-checked="autostartEnabled" :aria-label="t('settings.launchAtLogin')" class="toggle-switch" :class="{ enabled: autostartEnabled }" :disabled="autostartBusy || !autostartReady" @click="toggleAutostart"><span /></button></div>
      <p v-if="autostartError" class="form-error" role="alert">{{ tx(autostartError) }}</p>
    </section>
    <section class="settings-card" :aria-label="t('settings.appUpdates')">
      <div class="settings-head"><h2>{{ t("settings.appUpdates") }}</h2><button type="button" :disabled="checkingUpdates || installingUpdate" @click="checkUpdates">{{ t("settings.checkForUpdates") }}</button></div>
      <p v-if="updateStatus" class="settings-note">{{ tx(updateStatus) }}</p>
      <div v-if="updateOffer" class="update-item"><span>{{ t("settings.updateAvailable", { version: updateOffer.version }) }}<small v-if="updateOffer.notes" class="setting-description">{{ updateOffer.notes }}</small></span><button type="button" class="primary" :disabled="installingUpdate" @click="installUpdate">{{ t("settings.installUpdate") }}</button></div>
    </section>
    <section class="settings-card about-card" :aria-label="t('settings.about')">
      <div class="about-head">
        <img :src="appIcon" alt="" class="about-logo" />
        <div class="about-identity"><strong>ClashNova</strong><small class="setting-description">{{ t("settings.currentVersion", { version: installedVersion }) }}</small></div>
      </div>
      <div v-if="githubUrl" class="about-links">
        <button type="button" v-if="githubUrl" class="about-link" @click="openExternal(githubUrl)">
          <svg class="brand-icon" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23a11.5 11.5 0 0 1 3-.405c1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" /></svg>
          <span>GitHub</span><ExternalLink :size="15" />
        </button>
      </div>
      <button type="button" class="help-link" @click="tab = 'help'"><CircleHelp :size="17" />{{ t("settings.viewHelp") }}<ArrowRight :size="15" /></button>
    </section>
  </div>
</template>
<style scoped>
.settings-page{width:100%;max-width:1000px;margin-inline:auto;gap:16px;padding-bottom:10px}
.network-settings-form{display:flex;flex-direction:column;gap:16px;min-width:0}
.settings-card{min-width:0;padding:18px 22px 4px;border:1px solid var(--line);border-radius:14px;background:var(--bg);box-shadow:none}
.settings-card h2,.settings-card :deep(.shortcut-settings h2){margin:0 0 6px;font-size:14px;line-height:22px;font-weight:650;color:var(--strong)}
.settings-card .setting-row,.settings-card :deep(.shortcut-settings .setting-row){min-height:58px;gap:20px;padding:12px 0;border-top:1px solid var(--line);font-size:14px;line-height:1.5}
.settings-card h2 + .setting-row,.settings-card :deep(.shortcut-settings h2 + .setting-row){border-top:0}
.settings-card .setting-row > span:first-child,.settings-card :deep(.shortcut-settings .setting-row > span:first-child),.settings-card :deep(.shortcut-settings .setting-row > label){font-weight:500;overflow-wrap:anywhere}
.settings-card .setting-row > strong{font:13px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-variant-numeric:tabular-nums;color:var(--muted);-webkit-user-select:text;user-select:text;flex:none}
.settings-card .setting-description,.settings-card :deep(.setting-description){max-width:540px;line-height:1.6;font-size:12px;margin-top:3px}
.settings-card .setting-row > button:not(.toggle-switch),.settings-head > button,.update-item > button{height:34px;min-height:34px;padding:0 12px;font-size:13px;flex:none}
.settings-card .port-input{width:200px;height:34px;border-radius:8px;font-size:13px;flex:none}
.settings-card :deep(.select-menu){width:200px;max-width:100%;min-width:0}
.settings-card :deep(.select-trigger){height:34px;min-height:34px;border-radius:8px;font-size:13px}
.settings-card .seg{padding:3px;border-radius:8px;max-width:100%}
.settings-card .seg > button{min-height:28px;padding:3px 10px;border-radius:6px;font-size:13px;gap:6px}
.settings-card :deep(.toggle-switch){width:38px;min-width:38px;height:22px;min-height:22px;flex:none}
.settings-card :deep(.toggle-switch > span){width:16px;height:16px}
.settings-card :deep(.toggle-switch.enabled > span){transform:translateX(16px)}
.settings-card .exclude-input{width:280px;max-width:55%;min-height:78px;flex:none;border-radius:8px}
.settings-card .tun-exclude-row{align-items:flex-start}
.settings-card :deep(.tun-service-row){align-items:center;flex-direction:row;flex-wrap:wrap;gap:12px}
.settings-card :deep(.tun-service-row > span:first-child){flex:1 1 240px}
.settings-card :deep(.tun-service-actions){align-self:center;flex-wrap:wrap;gap:8px;max-width:100%;flex-shrink:1;margin-inline-start:auto}
.settings-card :deep(.tun-service-actions button){min-height:32px;padding:5px 10px;font-size:12px}
.settings-card :deep(.shortcut-settings){margin:0;padding-bottom:0}
.settings-card :deep(.shortcut-actions){padding:12px 0 14px;margin-top:0;border-top:1px solid var(--line)}
.settings-card :deep(.shortcut-actions button){height:34px;min-height:34px;padding:0 12px;font-size:13px;flex:none}
.settings-card .form-error,.settings-card :deep(.shortcut-error){padding:0 0 10px;font-size:12px;line-height:1.6}
.network-save-bar{position:sticky;bottom:-10px;z-index:4;display:flex;flex-direction:column;gap:6px;margin-top:-2px;padding:12px 0 10px;background:var(--surface);border-top:1px solid var(--line)}
.network-settings-actions{padding:0;gap:16px;min-height:36px}
.network-settings-actions .muted{font-size:12px;line-height:1.6}
.network-settings-actions button{height:34px;min-height:34px;padding:0 14px;flex:none;font-size:13px;color:#fff;background:var(--accent);border-color:var(--accent)}
.network-settings-actions button:hover:not(:disabled){background:var(--primary-hover);border-color:var(--primary-hover);color:#fff}
.network-save-bar .form-error{padding:0;font-size:12px;line-height:1.6}
.settings-head{margin-bottom:8px;align-items:center}
.settings-head h2{margin:0}
.settings-note{font-size:13px;line-height:1.7;margin:0 0 12px}
.update-item{padding:14px 0;align-items:flex-start;color:var(--text);line-height:1.7;overflow-wrap:anywhere}
.about-card{padding:22px}
.about-head{display:flex;align-items:center;gap:12px}
.about-logo{width:44px;height:44px;border-radius:10px;flex:none}
.about-identity{display:flex;flex-direction:column;gap:2px}
.about-identity strong{font-size:18px;line-height:1.4;font-weight:650;letter-spacing:-.02em}
.about-links{display:flex;flex-wrap:wrap;gap:8px;margin-top:22px}
.about-link{display:inline-flex;align-items:center;gap:8px;height:34px;min-height:34px;padding:0 12px;border:1px solid var(--border);border-radius:8px;background:var(--bg);color:var(--text);font:inherit;font-size:13px;cursor:pointer}
.about-link:hover{border-color:var(--accent-text);color:var(--accent-text)}
.about-link .brand-icon{width:16px;height:16px;flex:none}
.about-link :deep(svg){transform:none}
.help-link{display:flex;align-items:center;gap:8px;width:100%;min-height:38px;padding:14px 0 0;margin:18px 0 0;border-top:1px solid var(--line);border-radius:0;font-size:13px;justify-content:flex-start}
.help-link > svg:last-child{margin-inline-start:auto}
@media(max-width:900px){.settings-card{padding-inline:18px}.settings-card .setting-row,.settings-card :deep(.shortcut-settings .setting-row){gap:14px}.settings-card .seg > button{padding-inline:8px}.settings-card .seg svg{display:none}.network-save-bar{bottom:-16px}}
</style>
