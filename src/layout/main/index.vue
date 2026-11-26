<template>
  <div v-if="guardBlocking" class="page-error" role="alert">
    <span>{{ t("tun.guardBlocking") }}</span><BaseButton
      class="page-error-action"
      :disabled="busy"
      @click="releaseGuard"
    >
      {{ t("tun.guardRelease") }}
    </BaseButton>
  </div>
  <main ref="mainContent" class="main">
    <Overview v-if="tab === 'home'" />
    <Nodes v-else-if="tab === 'nodes'" />
    <Subscriptions v-else-if="tab === 'sub'" />
    <ConnectionsPage v-else-if="tab === 'connections'" :locale="locale" :node-names="connectionNodeNames" />
    <RulesPage v-else-if="tab === 'rules'" />
    <Logs v-else-if="tab === 'logs'" />
    <NetworkCheck v-else-if="tab === 'network'" />
    <LanTransfer v-else-if="tab === 'lan'" />
    <SettingsPage v-else-if="tab === 'settings'" />
    <HelpView v-else :locale="locale" @back="tab = 'settings'" />
  </main>
</template>
<script setup lang="ts">
  import { useApp } from "../../composables/useApp.ts";
  import BaseButton from "../../components/ui/BaseButton.vue";
  import Overview from "../../pages/overview/index.vue";
  import Nodes from "../../pages/nodes/index.vue";
  import Subscriptions from "../../pages/subscriptions/index.vue";
  import ConnectionsPage from "../../pages/connections/index.vue";
  import RulesPage from "../../pages/rules/index.vue";
  import Logs from "../../pages/logs/index.vue";
  import NetworkCheck from "../../pages/network-check/index.vue";
  import LanTransfer from "../../pages/lan/index.vue";
  import SettingsPage from "../../pages/settings/index.vue";
  import HelpView from "../../pages/help/index.vue";

  const { guardBlocking, releaseGuard, busy, t, mainContent, tab, locale, connectionNodeNames } = useApp();
</script>
