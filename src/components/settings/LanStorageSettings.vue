<script setup lang="ts">
  import { onMounted, onUnmounted, ref } from "vue";
  import { FolderOpen } from "@lucide/vue";
  import BaseButton from "../ui/BaseButton.vue";
  import { useApp } from "../../composables/useApp";
  import { notifications } from "../../lib/notifications";
  import HelpTip from "../ui/HelpTip.vue";
  const { t, tx, bridge } = useApp();
  const directory = ref(""),
    loading = ref(true),
    choosing = ref(false);
  let alive = true;
  onMounted(async () => {
    try {
      const settings = await (await bridge()).lanGetSettings();
      if (alive) directory.value = settings.defaultSaveDir;
    } catch (error) {
      if (alive)
        void notifications.show({
          body: tx(error instanceof Error ? error.message : String(error)),
          kind: "error",
        });
    } finally {
      if (alive) loading.value = false;
    }
  });
  onUnmounted(() => {
    alive = false;
  });
  async function choose() {
    if (choosing.value || loading.value) return;
    choosing.value = true;
    try {
      const selected = await (await bridge()).lanPickSaveDirectory();
      if (alive && selected) {
        directory.value = selected;
        void notifications.show({
          body: t("settings.lanDirectorySaved"),
          kind: "success",
        });
      }
    } catch (error) {
      if (alive)
        void notifications.show({
          body: tx(error instanceof Error ? error.message : String(error)),
          kind: "error",
        });
    } finally {
      if (alive) choosing.value = false;
    }
  }
</script>
<template>
  <section class="settings-card lan-storage-card" :aria-label="t('nav.lanTransfer')">
    <h2>{{ t("nav.lanTransfer") }}</h2>
    <div class="setting-row storage-setting">
      <span>{{ t("settings.lanSaveDirectory") }}<HelpTip :text="t('settings.lanSaveDirectoryHint')" /></span>
      <div class="storage-control">
        <span class="storage-path" :title="directory" role="status">{{
          loading ? t("settings.lanDirectoryLoading") : directory || t("settings.lanDefaultDirectory")
        }}</span><BaseButton :disabled="loading || choosing" @click="choose">
          <FolderOpen :size="15" />{{ t("settings.lanChooseDirectory") }}
        </BaseButton>
      </div>
    </div>
  </section>
</template>
<style scoped>
  .lan-storage-card h2 {
    margin: 0 0 6px;
    font-size: 14px;
    line-height: 22px;
    font-weight: 650;
    color: var(--strong);
  }
  .storage-setting {
    min-height: 58px;
    gap: 16px;
    padding: 12px 0;
    border-top: 0;
    font-size: 14px;
  }
  .storage-setting > span {
    font-weight: 500;
  }
  .storage-control {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 12px;
    min-width: 0;
    max-width: 65%;
  }
  .storage-path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font-size: 13px;
    user-select: text;
    -webkit-user-select: text;
  }
  .storage-control button {
    flex: none;
    height: 34px;
    min-height: 34px;
    padding: 0 12px;
    font-size: 13px;
  }
  @media (max-width: 839px) {
    .storage-setting {
      align-items: flex-start;
      flex-direction: column;
    }
    .storage-control {
      max-width: 100%;
      width: 100%;
    }
    .storage-path {
      flex: 1;
    }
  }
</style>
