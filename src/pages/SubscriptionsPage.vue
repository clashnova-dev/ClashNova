<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { confirmation } from "../lib/confirmation";
import { useApp } from "../composables/useApp";
import SubscriptionCard from "../components/SubscriptionCard.vue";
import ProfileEditor from "../components/ProfileEditor.vue";
import ProfileInfoDialog from "../components/ProfileInfoDialog.vue";
import ProfileNodesDialog from "../components/ProfileNodesDialog.vue";
import { encode } from "uqr";
import { Plus, QrCode, RefreshCw, Trash2, X } from "@lucide/vue";
const { t, locale, connected, access, baseUrl, activeSub, airportUsage, profileKind, subUrl, busy, refreshData, copySubscription, externalProfiles, activeExternalId, selectProfile, refreshExternal, removeExternal, openProfileDialog, subscriptionHost } = useApp();
const count = computed(() => (access.value ? 1 : 0) + externalProfiles.value.length);
const shareUrl = ref("");
const shareName = ref("");
const qrDialog = ref<HTMLDialogElement | null>(null);
const qr = computed(() => shareUrl.value ? qrModules(shareUrl.value) : null);
function qrModules(text: string) {
  const code = encode(text, { ecc: "M", border: 2 });
  const parts: string[] = [];
  code.data.forEach((row, y) => row.forEach((dark, x) => { if (dark) parts.push(`M${x} ${y}h1v1h-1z`); }));
  return { size: code.size, d: parts.join("") };
}
async function openShare(url: string, name: string) {
  if (!await confirmation.request({ title: "confirmation.exposeTitle", body: "confirmation.exposeBody", confirmLabel: "subscriptions.shareSubscription" })) return;
  if (subUrl.value !== url && !externalProfiles.value.some(profile => profile.url === url)) return;
  shareUrl.value = url;
  shareName.value = name;
  nextTick(() => qrDialog.value?.showModal());
}
async function refreshAccount() {
  if (busy.value) return;
  if (connected.value && !await confirmation.request({ title: "confirmation.refreshTitle", body: "confirmation.refreshBody", confirmLabel: "subscriptions.refreshSubscription" })) return;
  if (!busy.value) await refreshData();
}
function closeShare() { qrDialog.value?.close(); }
function resetShare() { shareUrl.value = ""; shareName.value = ""; }
type Profile = (typeof externalProfiles.value)[number];
const profileEditor = ref<InstanceType<typeof ProfileEditor> | null>(null);
function openEditor(profile: Profile) { profileEditor.value?.open(profile); }
const profileInfo = ref<InstanceType<typeof ProfileInfoDialog> | null>(null);
function openInfo(profile: Profile) { profileInfo.value?.open(profile); }
const profileNodes = ref<InstanceType<typeof ProfileNodesDialog> | null>(null);
function openNodes(profile: Profile) { profileNodes.value?.open(profile); }
function onNodesAdvanced(nid: string) { const p = externalProfiles.value.find(x => x.id === nid); if (p) openEditor(p); }

// 订阅卡片右键菜单:实现当前可用的子集(使用 / 分享二维码 / 编辑文件 / 更新 / 删除)。
type MenuItem = { label: string; run: () => void; danger?: boolean; disabled?: boolean };
const menu = ref<{ x: number; y: number; items: MenuItem[] } | null>(null);
function openMenu(event: MouseEvent, profile: Profile) {
  event.preventDefault();
  const items: MenuItem[] = [
    { label: t("subscriptions.useSubscription"), run: () => selectProfile(profile), disabled: busy.value || activeExternalId.value === profile.id },
    ...(profile.url ? [{ label: t("subscriptions.shareSubscription"), run: () => openShare(profile.url, profile.name) }] : []),
    { label: t("subscriptions.editInfoTitle"), run: () => openInfo(profile) },
    { label: t("subscriptions.editTitle"), run: () => openEditor(profile) },
    { label: t("subscriptions.editNodesTitle"), run: () => openNodes(profile) },
    ...(profile.url ? [{ label: t("subscriptions.updateSubscription"), run: () => refreshExternal(profile), disabled: busy.value }] : []),
    { label: t("subscriptions.removeSubscription"), run: () => removeExternal(profile), danger: true, disabled: busy.value },
  ];
  menu.value = {
    x: Math.max(8, Math.min(event.clientX, window.innerWidth - 196)),
    y: Math.max(8, Math.min(event.clientY, window.innerHeight - items.length * 38 - 16)),
    items,
  };
}
function closeMenu() { menu.value = null; }
function runItem(item: MenuItem) { closeMenu(); if (!item.disabled) item.run(); }
function onMenuKey(event: KeyboardEvent) { if (event.key === "Escape") closeMenu(); }
watch(menu, open => { if (open) window.addEventListener("keydown", onMenuKey); else window.removeEventListener("keydown", onMenuKey); });
onBeforeUnmount(() => window.removeEventListener("keydown", onMenuKey));
</script>
<template>
  <div class="subs-page">
    <div class="subs-head">
      <div><h2>{{ t("subscriptions.mySubscriptions") }}</h2><span>{{ t("subscriptions.count", { count }) }}</span></div>
      <button class="primary" @click="openProfileDialog"><Plus :size="16" />{{ t("subscriptions.add") }}</button>
    </div>
    <div v-if="count" class="subs-grid">
      <SubscriptionCard v-if="access" kind="airport" :label="t('subscriptions.airportSubscription')" :name="activeSub?.planName || t('subscriptions.airportSubscription')" :selected="profileKind === 'airport'" :source="subscriptionHost(baseUrl)" :usage="airportUsage" :locale="locale">
        <div class="profile-card-actions"><button :disabled="busy" @click="refreshAccount"><RefreshCw :size="15" />{{ profileKind === "airport" ? t("subscriptions.refreshSubscription") : t("subscriptions.useSubscription") }}</button><button v-if="subUrl" class="icon-button" :disabled="busy" :aria-label="t('subscriptions.shareSubscription')" :title="t('subscriptions.shareSubscription')" @click="openShare(subUrl, activeSub?.planName || t('subscriptions.airportSubscription'))"><QrCode :size="16" /></button><button v-if="subUrl" :disabled="busy" @click="copySubscription">{{ t("subscriptions.copySubscriptionUrl") }}</button></div>
      </SubscriptionCard>
      <SubscriptionCard v-for="profile in externalProfiles" :key="profile.id" kind="external" :label="profile.url ? t('subscriptions.remote') : t('subscriptions.localFile')" :name="profile.name" :selected="activeExternalId === profile.id" :source="subscriptionHost(profile.url)" :updated-at="profile.updatedAt" :usage="profile.usage" :locale="locale" @contextmenu="openMenu($event, profile)">
        <div class="profile-card-actions"><button class="profile-activate" :disabled="busy || activeExternalId === profile.id" @click="selectProfile(profile)">{{ activeExternalId === profile.id ? t("subscriptions.enabled") : t("subscriptions.useSubscription") }}</button><button v-if="profile.url" class="icon-button" :disabled="busy" :aria-label="t('subscriptions.updateNamed', { name: profile.name })" :title="t('subscriptions.updateSubscription')" @click="refreshExternal(profile)"><RefreshCw :size="16" /></button><button v-if="profile.url" class="icon-button" :disabled="busy" :aria-label="t('subscriptions.shareSubscription')" :title="t('subscriptions.shareSubscription')" @click="openShare(profile.url, profile.name)"><QrCode :size="16" /></button><button class="icon-button remove-button" :disabled="busy" :aria-label="t('subscriptions.removeNamed', { name: profile.name })" :title="t('subscriptions.removeSubscription')" @click="removeExternal(profile)"><Trash2 :size="16" /></button></div>
      </SubscriptionCard>
    </div>
    <div v-else class="empty-note">{{ t("subscriptions.pasteASubscriptionUrlOr") }}</div>
    <dialog ref="qrDialog" class="subscription-dialog qr-share" aria-labelledby="share-qr-title" @close="resetShare" @click="($event.target === qrDialog) && closeShare()">
      <div class="dialog-heading"><h2 id="share-qr-title">{{ t("subscriptions.shareSubscription") }}</h2><button class="icon-button" type="button" :aria-label="t('common.close')" @click="closeShare"><X :size="20" /></button></div>
      <p class="dialog-description">{{ t("subscriptions.shareQrHint") }}</p>
      <svg v-if="qr" class="qr-code" :viewBox="`0 0 ${qr.size} ${qr.size}`" role="img" :aria-label="shareName"><path :d="qr.d" /></svg>
      <p class="qr-name">{{ shareName }}</p>
    </dialog>
    <ProfileEditor ref="profileEditor" />
    <ProfileInfoDialog ref="profileInfo" />
    <ProfileNodesDialog ref="profileNodes" @advanced="onNodesAdvanced" />
    <div v-if="menu" class="ctx-backdrop" @click="closeMenu" @contextmenu.prevent="closeMenu">
      <ul class="ctx-menu" :style="{ left: menu.x + 'px', top: menu.y + 'px' }" role="menu" @click.stop>
        <li v-for="(item, i) in menu.items" :key="i"><button type="button" role="menuitem" :class="{ danger: item.danger }" :disabled="item.disabled" @click="runItem(item)">{{ item.label }}</button></li>
      </ul>
    </div>
  </div>
</template>
<style scoped>
.qr-share { width: min(360px, calc(100vw - 48px)); text-align: center; }
.qr-code { display: block; width: 220px; height: 220px; margin: 4px auto 0; padding: 12px; box-sizing: content-box; background: #fff; border: 1px solid var(--line); border-radius: 8px; shape-rendering: crispEdges; }
.qr-code path { fill: #1c2533; }
.qr-name { margin: 12px 0 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted); font-size: 13px; }
.ctx-backdrop { position: fixed; inset: 0; z-index: 50; }
.ctx-menu { position: fixed; min-width: 184px; margin: 0; padding: 6px; list-style: none; background: var(--bg); border: 1px solid var(--line); border-radius: 10px; box-shadow: var(--shadow-hover); }
.ctx-menu li { margin: 0; }
.ctx-menu button { display: block; width: 100%; text-align: left; padding: 8px 12px; min-height: 34px; border: 0; border-radius: 6px; background: transparent; color: var(--text); font-size: 14px; }
.ctx-menu button:hover:not(:disabled) { background: var(--hover); }
.ctx-menu button:disabled { color: var(--muted); opacity: 0.5; cursor: default; }
.ctx-menu button.danger { color: var(--danger); }
.ctx-menu button.danger:hover:not(:disabled) { background: var(--danger-bg); }
</style>
