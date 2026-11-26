<script setup lang="ts">
  import { onMounted, ref } from "vue";
  import HelpTip from "../ui/HelpTip.vue";
  import BaseButton from "../ui/BaseButton.vue";
  import { confirmation } from "../../lib/confirmation";
  import {
    tunStatus,
    tunServiceBusy,
    refreshTunService,
    installTunService,
    uninstallTunService,
  } from "../../lib/tun-service";
  import { notifications } from "../../lib/notifications";
  import { translator, type Locale } from "../../i18n";
  const props = defineProps<{ locale: Locale; disabled: boolean }>();
  const { t, tx } = translator(() => props.locale);
  const pending = ref(false);
  async function refresh() {
    try {
      await refreshTunService();
    } catch (error) {
      await notifications.show({ body: tx(String(error)), kind: "error" });
    }
  }
  async function install() {
    if (props.disabled || tunServiceBusy.value || pending.value) return;
    pending.value = true;
    const repairing = tunStatus.value.ready;
    try {
      if (
        !(await confirmation.request({
          title: repairing ? "tun.repairService" : "tun.authorizeInstallation",
          body: repairing ? "confirmation.repairBody" : "confirmation.installBody",
          confirmLabel: repairing ? "tun.repairService" : "tun.authorizeInstallation",
        }))
      )
        return;
      if (props.disabled || tunServiceBusy.value || tunStatus.value.ready !== repairing) return;
      await installTunService(props.locale);
    } catch {
      /* Shared notification contains the failure. */
    } finally {
      pending.value = false;
    }
  }
  async function remove() {
    if (props.disabled || tunServiceBusy.value || pending.value || !tunStatus.value.ready) return;
    pending.value = true;
    try {
      if (
        !(await confirmation.request({
          title: "tun.uninstallTunService",
          body: "tun.tunWillBeUnavailableSystem",
          confirmLabel: "tun.uninstallService",
          danger: true,
        }))
      )
        return;
      if (!props.disabled && !tunServiceBusy.value && tunStatus.value.ready) await uninstallTunService(props.locale);
    } finally {
      pending.value = false;
    }
  }
  onMounted(() => {
    void refresh();
  });
</script>
<template>
  <div class="setting-row tun-service-row">
    <span
      >{{ t("tun.tunService") }}<HelpTip :text="t('tun.administratorAuthorizationIsRequiredOnce')" /><small
        class="setting-description"
        role="status"
        >{{ tx(tunStatus.message) }}</small
      ></span
    >
    <div class="tun-service-actions">
      <BaseButton :disabled="disabled || tunServiceBusy || pending" @click="refresh">
        {{ t("nodes.test") }}
      </BaseButton>
      <BaseButton :disabled="disabled || tunServiceBusy || pending" @click="install">
        {{
          tunServiceBusy
            ? t("tun.waitingForSystemAuthorization")
            : t(tunStatus.ready ? "tun.repairService" : "tun.authorizeInstallation")
        }}
      </BaseButton>
      <BaseButton v-if="tunStatus.ready" :disabled="disabled || tunServiceBusy || pending" @click="remove">
        {{ t("tun.uninstallService") }}
      </BaseButton>
    </div>
  </div>
</template>
<style scoped>
  .tun-service-actions {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }
  .tun-service-row {
    align-items: center;
    gap: 20px;
  }
</style>
