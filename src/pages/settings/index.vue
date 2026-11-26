<script setup lang="ts">
  import { confirmation } from "../../lib/confirmation";
  import { useApp } from "../../composables/useApp";
  import LanStorageSettings from "../../components/settings/LanStorageSettings.vue";
  import LanProxyAddresses from "../../components/settings/LanProxyAddresses.vue";
  import TunServiceSettings from "../../components/settings/TunServiceSettings.vue";
  import ShortcutSettings from "../../components/settings/ShortcutSettings.vue";
  import SelectMenu from "../../components/ui/SelectMenu.vue";
  import ToggleSwitch from "../../components/ui/ToggleSwitch.vue";
  import HelpTip from "../../components/ui/HelpTip.vue";
  import BaseButton from "../../components/ui/BaseButton.vue";
  import { ArrowRight, CircleHelp, ExternalLink, Monitor, Sun, Moon } from "@lucide/vue";
  import appIcon from "../../assets/icon/favicon.png";
  import { xUrl } from "../../constants/domains";
  const {
    t,
    tx,
    tab,
    locale,
    theme,
    showTraySpeed,
    probeConcurrency,
    isMac,
    busy,
    connected,
    connectionMode,
    setConnectionMode,
    routeMode,
    setRouteMode,
    proxyDiagnostic,
    diagnosticBusy,
    refreshProxyDiagnostics,
    localProxyAddress,
    draftAllowLan,
    draftProxyPort,
    draftTun,
    draftTunMtu,
    draftTunExclude,
    isLinux,
    networkLoaded,
    networkSettings,
    networkDirty,
    networkError,
    saveNetworkSettings,
    autostartEnabled,
    autostartBusy,
    autostartReady,
    autostartError,
    toggleAutostart,
    installedVersion,
    updateOffer,
    updateStatus,
    installingUpdate,
    checkingUpdates,
    checkUpdates,
    installUpdate,
    githubUrl,
    openExternal,
  } = useApp();
  async function changeProxy(value: "system" | "tun") {
    const previous = connectionMode.value;
    if (previous === value || busy.value) return;
    if (
      !(await confirmation.request({
        title: "confirmation.proxyTitle",
        body: "confirmation.proxyBody",
      }))
    )
      return;
    if (!busy.value && connectionMode.value === previous) await setConnectionMode(value);
  }
  async function changeRoute(value: "rule" | "global") {
    const previous = routeMode.value;
    if (previous === value || busy.value) return;
    if (
      !(await confirmation.request({
        title: "confirmation.routingTitle",
        body: "confirmation.routingBody",
      }))
    )
      return;
    if (!busy.value && routeMode.value === previous) await setRouteMode(value);
  }
</script>
<template>
  <div class="settings-page">
    <section class="settings-card" :aria-label="t('settings.appearanceLanguage')">
      <h2>{{ t("settings.appearanceLanguage") }}</h2>
      <div class="setting-row">
        <span>{{ t("settings.language") }}</span><SelectMenu
          v-model="locale"
          :label="t('settings.interfaceLanguage')"
          :options="[
            { value: 'en', label: 'English' },
            { value: 'ja', label: t('settings.languageJapanese') },
            { value: 'ko', label: '한국어' },
            { value: 'de', label: 'Deutsch' },
            { value: 'fr', label: 'Français' },
            { value: 'ru', label: 'Русский' },
            { value: 'zh-CN', label: t('settings.languageChinese') },
          ]"
        />
      </div>
      <div class="setting-row">
        <span>{{ t("settings.theme") }}</span>
        <div class="seg" role="group" :aria-label="t('settings.theme')">
          <BaseButton
            :aria-pressed="theme === 'system'"
            :class="{ chosen: theme === 'system' }"
            @click="theme = 'system'"
          >
            <Monitor :size="14" />{{ t("settings.themeSystem") }}</BaseButton
          ><BaseButton
            :aria-pressed="theme === 'light'"
            :class="{ chosen: theme === 'light' }"
            @click="theme = 'light'"
          >
            <Sun :size="14" />{{ t("settings.themeLight") }}</BaseButton
          ><BaseButton :aria-pressed="theme === 'dark'" :class="{ chosen: theme === 'dark' }" @click="theme = 'dark'">
            <Moon :size="14" />{{ t("settings.themeDark") }}
          </BaseButton>
        </div>
      </div>
      <div class="setting-row">
        <span>{{ t("settings.showLiveSpeed") }}<HelpTip :text="t('settings.showUploadAndDownloadSpeed')" /></span
        ><ToggleSwitch v-model="showTraySpeed" :label="t('settings.showLiveSpeed')" />
      </div>
      <div class="setting-row">
        <span
          ><label for="probe-concurrency">{{ t("settings.testConcurrency") }}</label
          ><HelpTip :text="t('settings.testConcurrencyHelp')" /></span
        ><input
          id="probe-concurrency"
          v-model.number="probeConcurrency"
          @change="probeConcurrency = Math.min(64, Math.max(1, Math.round(Number(probeConcurrency) || 8)))"
          type="number"
          min="1"
          max="64"
          step="1"
          inputmode="numeric"
          autocomplete="off"
          class="port-input"
        />
      </div>
    </section>
    <LanStorageSettings />
    <section class="settings-card" :aria-label="t('settings.connectionSettings')">
      <h2>{{ t("settings.connectionSettings") }}</h2>
      <div class="setting-row">
        <span>{{ t("common.proxyHandling") }}<HelpTip :text="t('settings.systemProxyCannotCaptureWebrtc')" /></span>
        <div class="seg" role="group" :aria-label="t('common.proxyHandling')">
          <BaseButton
            :aria-pressed="connectionMode === 'system'"
            :class="{ chosen: connectionMode === 'system' }"
            @click="changeProxy('system')"
          >
            {{ t("common.systemProxy") }}</BaseButton><BaseButton
            :aria-pressed="connectionMode === 'tun'"
            :class="{ chosen: connectionMode === 'tun' }"
            @click="changeProxy('tun')"
          >
            TUN
          </BaseButton>
        </div>
      </div>
      <TunServiceSettings v-if="connectionMode === 'tun'" :locale="locale" :disabled="busy || connected" />
      <div class="setting-row">
        <span>{{ t("common.routing") }}</span>
        <div class="seg" role="group" :aria-label="t('common.routing')">
          <BaseButton
            :aria-pressed="routeMode === 'rule'"
            :class="{ chosen: routeMode === 'rule' }"
            @click="changeRoute('rule')"
          >
            {{ t("settings.ruleMode") }}</BaseButton><BaseButton
            :aria-pressed="routeMode === 'global'"
            :class="{ chosen: routeMode === 'global' }"
            @click="changeRoute('global')"
          >
            {{ t("settings.globalMode") }}
          </BaseButton>
        </div>
      </div>
      <div class="setting-row">
        <span
          >{{ t("settings.systemProxyCheck")
          }}<small class="setting-description" role="status">{{ tx(proxyDiagnostic) }}</small></span
        ><BaseButton :disabled="diagnosticBusy || busy" @click="refreshProxyDiagnostics">
          {{ t("settings.checkAgain") }}
        </BaseButton>
      </div>
      <div class="setting-row">
        <span>{{ t("settings.localProxy") }}</span><strong>{{ localProxyAddress }}</strong>
      </div>
    </section>
    <form class="network-settings-form" @submit.prevent="saveNetworkSettings">
      <section class="settings-card" :aria-label="t('settings.localAccess')">
        <h2>{{ t("settings.localAccess") }}</h2>
        <div class="setting-row">
          <span>{{ t("settings.allowLan") }}<HelpTip :text="t('settings.allowOtherDevicesOnYour')" /></span
          ><ToggleSwitch
            v-model="draftAllowLan"
            :label="t('settings.allowLan')"
            :disabled="busy || connected || !networkLoaded"
          />
        </div>
        <LanProxyAddresses v-if="draftAllowLan" :port="draftProxyPort" :pending="networkDirty" />
        <div class="setting-row">
          <span
            ><label for="proxy-port">{{ t("settings.proxyPort") }}</label
            ><HelpTip :text="t('settings.sharedHttpAndSocksPort')" /></span
          ><input
            id="proxy-port"
            v-model="draftProxyPort"
            type="text"
            inputmode="numeric"
            autocomplete="off"
            class="port-input"
            :disabled="busy || connected || !networkLoaded"
          />
        </div>
      </section>
      <template v-if="connectionMode === 'tun'">
        <section class="settings-card" :aria-label="t('settings.tunOptions')">
          <h2>{{ t("settings.tunOptions") }}</h2>
          <div class="setting-row">
            <span>{{ t("settings.tunStack") }}<HelpTip :text="t('settings.tunStackHelp')" /></span>
            <div class="seg" role="group" :aria-label="t('settings.tunStack')">
              <BaseButton
                :aria-pressed="draftTun.stack === 'mixed'"
                :class="{ chosen: draftTun.stack === 'mixed' }"
                :disabled="busy || connected || !networkLoaded"
                @click="draftTun.stack = 'mixed'"
              >
                {{ t("settings.tunStackMixed") }}</BaseButton><BaseButton
                :aria-pressed="draftTun.stack === 'system'"
                :class="{ chosen: draftTun.stack === 'system' }"
                :disabled="busy || connected || !networkLoaded"
                @click="draftTun.stack = 'system'"
              >
                {{ t("settings.tunStackSystem") }}</BaseButton
              ><BaseButton
                :aria-pressed="draftTun.stack === 'gvisor'"
                :class="{ chosen: draftTun.stack === 'gvisor' }"
                :disabled="busy || connected || !networkLoaded"
                @click="draftTun.stack = 'gvisor'"
              >
                gVisor
              </BaseButton>
            </div>
          </div>
          <div class="setting-row">
            <span
              ><label for="tun-mtu">{{ t("settings.tunMtu") }}</label
              ><HelpTip :text="t('settings.tunMtuHelp')" /></span
            ><input
              id="tun-mtu"
              v-model="draftTunMtu"
              type="text"
              inputmode="numeric"
              autocomplete="off"
              class="port-input"
              :disabled="busy || connected || !networkLoaded"
            />
          </div>
          <div class="setting-row">
            <span>{{ t("settings.tunStrictRoute") }}<HelpTip :text="t('settings.tunStrictRouteHelp')" /></span
            ><ToggleSwitch
              v-model="draftTun.strictRoute"
              :label="t('settings.tunStrictRoute')"
              :disabled="busy || connected || !networkLoaded"
            />
          </div>
          <div class="setting-row">
            <span>{{ t("settings.tunKillSwitch") }}<HelpTip :text="t('settings.tunKillSwitchHelp')" /></span
            ><ToggleSwitch
              v-model="draftTun.killSwitch"
              :label="t('settings.tunKillSwitch')"
              :disabled="busy || connected || !networkLoaded"
            />
          </div>
          <div class="setting-row">
            <span>{{ t("settings.tunIpv6") }}<HelpTip :text="t('settings.tunIpv6Help')" /></span
            ><ToggleSwitch
              v-model="draftTun.ipv6"
              :label="t('settings.tunIpv6')"
              :disabled="busy || connected || !networkLoaded"
            />
          </div>
          <div v-if="isLinux" class="setting-row">
            <span>{{ t("settings.tunAutoRedirect") }}<HelpTip :text="t('settings.tunAutoRedirectHelp')" /></span
            ><ToggleSwitch
              v-model="draftTun.autoRedirect"
              :label="t('settings.tunAutoRedirect')"
              :disabled="busy || connected || !networkLoaded"
            />
          </div>
          <div class="setting-row tun-exclude-row">
            <span
              ><label for="tun-exclude">{{ t("settings.tunExclude") }}</label
              ><HelpTip :text="t('settings.tunExcludeHelp')" /></span
            ><textarea
              id="tun-exclude"
              v-model="draftTunExclude"
              rows="3"
              spellcheck="false"
              autocomplete="off"
              class="exclude-input"
              placeholder="192.168.0.0/16"
              :disabled="busy || connected || !networkLoaded"
            />
          </div>
        </section>
        <section class="settings-card" :aria-label="t('settings.dnsSettings')">
          <h2>{{ t("settings.dnsSettings") }}</h2>
          <div class="setting-row">
            <span>{{ t("settings.dnsMode") }}<HelpTip :text="t('settings.dnsModeHelp')" /></span>
            <div class="seg" role="group" :aria-label="t('settings.dnsMode')">
              <BaseButton
                :aria-pressed="draftTun.dnsMode === 'fakeip'"
                :class="{ chosen: draftTun.dnsMode === 'fakeip' }"
                :disabled="busy || connected || !networkLoaded"
                @click="draftTun.dnsMode = 'fakeip'"
              >
                {{ t("settings.dnsFakeip") }}</BaseButton><BaseButton
                :aria-pressed="draftTun.dnsMode === 'remote'"
                :class="{ chosen: draftTun.dnsMode === 'remote' }"
                :disabled="busy || connected || !networkLoaded"
                @click="draftTun.dnsMode = 'remote'"
              >
                {{ t("settings.dnsReal") }}
              </BaseButton>
            </div>
          </div>
          <div class="setting-row">
            <span>{{ t("settings.dnsDomestic") }}</span><SelectMenu
              :disabled="busy || connected || !networkLoaded"
              v-model="draftTun.dnsDomestic"
              :label="t('settings.dnsDomestic')"
              :options="[
                { value: 'local', label: t('settings.dnsLocal') },
                { value: 'ali', label: 'AliDNS' },
                { value: 'dnspod', label: 'DNSPod' },
              ]"
            />
          </div>
          <div v-if="draftTun.dnsMode === 'remote'" class="setting-row">
            <span>{{ t("settings.dnsRemote") }}</span><SelectMenu
              :disabled="busy || connected || !networkLoaded"
              v-model="draftTun.dnsRemote"
              :label="t('settings.dnsRemote')"
              :options="[
                { value: 'cloudflare', label: 'Cloudflare' },
                { value: 'google', label: 'Google' },
                { value: 'quad9', label: 'Quad9' },
              ]"
            />
          </div>
        </section>
      </template>
      <div class="network-save-bar">
        <div class="network-settings-actions">
          <span class="muted">{{
            connected
              ? t("settings.disconnectToChangeNetworkSettings")
              : networkSettings.allowLan
                ? t("settings.lanDevicesUseThisComputers")
                : t("settings.onlyThisComputerCanAccess")
          }}</span><BaseButton type="submit" :disabled="busy || connected || !networkLoaded || !networkDirty">
            {{ t("settings.saveNetworkSettings") }}
          </BaseButton>
        </div>
        <p v-if="networkError" class="form-error" role="alert">{{ tx(networkError) }}</p>
      </div>
    </form>
    <section class="settings-card" :aria-label="t('common.keyboardShortcuts')">
      <ShortcutSettings :locale="locale" :is-mac="isMac" />
    </section>
    <section class="settings-card" :aria-label="t('settings.startup')">
      <h2>{{ t("settings.startup") }}</h2>
      <div class="setting-row">
        <span>{{ t("settings.launchAtLogin") }}<HelpTip :text="t('settings.startAirportWhenYouLog')" /></span
        ><ToggleSwitch
          :model-value="autostartEnabled"
          :label="t('settings.launchAtLogin')"
          :disabled="autostartBusy || !autostartReady"
          @update:model-value="toggleAutostart"
        />
      </div>
      <p v-if="autostartError" class="form-error" role="alert">{{ tx(autostartError) }}</p>
    </section>
    <section class="settings-card" :aria-label="t('settings.appUpdates')">
      <div class="settings-head">
        <h2>{{ t("settings.appUpdates") }}</h2>
        <BaseButton :disabled="checkingUpdates || installingUpdate" @click="checkUpdates">
          {{ t("settings.checkForUpdates") }}
        </BaseButton>
      </div>
      <p v-if="updateStatus" class="settings-note">{{ tx(updateStatus) }}</p>
      <div v-if="updateOffer" class="update-item">
        <span
          >{{ t("settings.updateAvailable", { version: updateOffer.version })
          }}<small v-if="updateOffer.notes" class="setting-description">{{ updateOffer.notes }}</small></span
        ><BaseButton variant="primary" :disabled="installingUpdate" @click="installUpdate">
          {{ t("settings.installUpdate") }}
        </BaseButton>
      </div>
    </section>
    <section class="settings-card about-card" :aria-label="t('settings.about')">
      <div class="about-head">
        <img :src="appIcon" alt="" class="about-logo" />
        <div class="about-identity">
          <strong>ClashNova</strong
          ><small class="setting-description">{{ t("settings.currentVersion", { version: installedVersion }) }}</small>
        </div>
      </div>
      <div class="about-links">
        <BaseButton v-if="githubUrl" class="about-link" @click="openExternal(githubUrl)">
          <svg class="brand-icon" viewBox="0 0 24 24" aria-hidden="true">
            <path
              fill="currentColor"
              d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23a11.5 11.5 0 0 1 3-.405c1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"
            />
          </svg>
          <span>GitHub</span><ExternalLink :size="15" />
        </BaseButton>
        <BaseButton v-if="xUrl" class="about-link" @click="openExternal(xUrl)">
          <svg class="brand-icon" viewBox="0 0 24 24" aria-hidden="true">
            <path
              fill="currentColor"
              d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"
            />
          </svg>
          <span>X</span><ExternalLink :size="15" />
        </BaseButton>
      </div>
      <BaseButton class="help-link" @click="tab = 'help'">
        <CircleHelp :size="17" />{{ t("settings.viewHelp") }}<ArrowRight :size="15" />
      </BaseButton>
    </section>
  </div>
</template>
<style scoped lang="scss" src="./index.scss"></style>
