<template>
  <div class="setting-row">
    <span
      >{{ t("notifications.systemNotifications")
      }}<HelpTip :text="t('notifications.connectionAndServiceUpdatesIn')" /><small
        v-if="hint"
        class="setting-description"
        role="status"
        >{{ hint }}</small
      ></span
    >
    <div class="notice-settings-actions">
      <BaseButton :disabled="busy" @click="test">{{ t("notifications.testNotification") }}</BaseButton><ToggleSwitch
        :model-value="notificationsEnabled"
        :label="t('notifications.systemNotifications')"
        :disabled="busy"
        @update:model-value="toggle"
      />
    </div>
  </div>
</template>
<script setup lang="ts">
  import HelpTip from "../ui/HelpTip.vue";
  import ToggleSwitch from "../ui/ToggleSwitch.vue";
  import BaseButton from "../ui/BaseButton.vue";
  import { ref } from "vue";
  import { notifications, notificationsEnabled, enableNotifications } from "../../lib/notifications";
  import { translator, type Locale } from "../../i18n";
  const props = defineProps<{ locale: Locale }>();
  const { t, tx } = translator(() => props.locale);
  const busy = ref(false),
    hint = ref("");
  async function toggle() {
    busy.value = true;
    try {
      const ok = await enableNotifications(!notificationsEnabled.value);
      hint.value = ok ? "" : t("notifications.allowAirportNotificationsInSystem");
    } finally {
      busy.value = false;
    }
  }
  async function test() {
    busy.value = true;
    try {
      const allowed = await notifications.permission(true);
      hint.value = !allowed
        ? t("notifications.allowAirportNotificationsInSystem")
        : !notificationsEnabled.value
          ? t("notifications.systemNotificationsAreDisabledOnly")
          : "";
      const result = await notifications.show({
        title: "ClashNova",
        body: t("notifications.thisIsATestNotification"),
        native: true,
        kind: "info",
      });
      if (result === "failed") hint.value = t("notifications.systemNotificationFailedCheckSystem");
      else if (result === "sent") hint.value = t("notifications.notificationSentIfItIs");
    } finally {
      busy.value = false;
    }
  }
</script>
<style scoped>
  .notice-settings-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-shrink: 0;
  }
</style>
