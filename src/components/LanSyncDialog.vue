<template>
  <dialog
    ref="dialog"
    class="subscription-dialog"
    aria-labelledby="lan-sync-title"
    @close="onClose"
    @click="$event.target === dialog && close()"
  >
    <div class="dialog-heading">
      <h2 id="lan-sync-title">{{ t("lan.sync") }}</h2>
      <BaseButton variant="icon" :aria-label="t('common.close')" @click="close"><X :size="20" /></BaseButton>
    </div>
    <p class="dialog-description">{{ t("lan.syncDesc") }}</p>
    <div v-if="devices.length" class="lan-sync-list">
      <div v-for="d in devices" :key="d.fingerprint" class="lan-sync-row">
        <span class="lan-sync-meta"
          ><strong>{{ d.alias }}</strong
          ><small>{{ d.address }}</small></span
        >
        <BaseButton variant="primary" :disabled="!!syncing" @click="pull(d.fingerprint)">
          {{ syncing === d.fingerprint ? t("lan.syncing") : t("lan.syncAction") }}
        </BaseButton>
      </div>
    </div>
    <p v-else class="hint">{{ t("lan.syncNoDevices") }}</p>
  </dialog>
</template>
<script setup lang="ts">
  import { ref, onBeforeUnmount } from "vue";
  import { useApp } from "../composables/useApp";
  import { useSubscriptionsStore } from "../stores/subscriptions";
  import BaseButton from "./ui/BaseButton.vue";
  import { X } from "@lucide/vue";
  import { notifications } from "../lib/notifications";
  import type { LanDiscoveredDevice } from "../lib/lan-transfer";

  // 拉取方弹窗：列出局域网设备，点「同步」从该设备拉取其配置并合并应用到本机。
  const { t, tx, bridge } = useApp();
  const subs = useSubscriptionsStore();
  const dialog = ref<HTMLDialogElement | null>(null);
  const devices = ref<LanDiscoveredDevice[]>([]);
  const syncing = ref("");
  let stop: (() => void) | null = null;

  async function open() {
    const b = await bridge();
    try {
      await b.lanStart();
    } catch {
      /* 已在运行 */
    }
    // 先订阅设备变化，再触发刷新：lanRefreshDevices 只触发后端重新广播，
    // 设备列表经 onLanDevices 事件回来（其本身返回 void）。
    stop?.();
    stop =
      (await b.onLanDevices?.(list => {
        devices.value = list;
      })) ?? null;
    try {
      await b.lanRefreshDevices();
    } catch {
      /* ignore */
    }
    void b.lanScanSubnet?.().catch(() => {});
    dialog.value?.showModal();
  }
  function close() {
    dialog.value?.close();
  }
  function onClose() {
    stop?.();
    stop = null;
  }
  async function pull(fp: string) {
    if (syncing.value) return;
    syncing.value = fp;
    try {
      const payload = await (await bridge()).lanSyncPull(fp);
      const count = await subs.applyLanSyncPayload(payload);
      notifications.show({ body: t("lan.synced", { count }), kind: "success", native: false });
      close();
    } catch (e) {
      const key = e instanceof Error ? e.message : String(e);
      const body = key === "native.lanRejected" ? t("lan.syncRejected") : tx(key);
      notifications.show({ body, kind: "error", native: false });
    } finally {
      syncing.value = "";
    }
  }
  onBeforeUnmount(() => stop?.());
  defineExpose({ open });
</script>
<style scoped lang="scss">
  .lan-sync-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 4px;
    max-height: 360px;
    overflow-y: auto;
  }
  .lan-sync-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border, rgba(127, 127, 127, 0.2));
    border-radius: 10px;
  }
  .lan-sync-meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
    strong {
      font-size: 14px;
    }
    small {
      color: var(--muted, #888);
      font-size: 12px;
    }
  }
</style>
