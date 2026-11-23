<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, computed, nextTick, watch } from "vue";
import {
  Laptop, Smartphone, Server, Globe, RefreshCw, ArrowUp, Check, X, File as FileIcon,
  Radio, Link2, Unlink, Paperclip, FolderOpen, Copy,
  ClipboardPaste, Trash2, Pause, Play, MessageSquare, Plus, Settings, ShieldCheck,
} from "@lucide/vue";
import { useApp } from "../composables/useApp";
import FileTypeIcon from "../components/FileTypeIcon.vue";
import { notifications } from "../lib/notifications";
import type { NoticeKind } from "../lib/notification-center";
import type { MessageKey } from "../i18n";
import type {
  LanLocalDevice, LanDiscoveredDevice, LanTransferTask, LanHistoryEntry, LanSettings,
  LanDeviceType, LanTransferStateName, LanTrustedDevice, LanPairRequest, LanIncomingRequest, LanProgress,
} from "../lib/lan-transfer";

type ChatItem = LanHistoryEntry & { live?: boolean };

const { t, tx, bridge, locale } = useApp();
const localDevice = ref<LanLocalDevice | null>(null);
const settings = ref<LanSettings | null>(null);
const devices = ref<LanDiscoveredDevice[]>([]);
const trusted = ref<LanTrustedDevice[]>([]);
const selected = ref("");
const messages = ref<LanHistoryEntry[]>([]);
const liveTasks = ref<LanTransferTask[]>([]);
const progress = ref<Record<string, LanProgress>>({});
const draft = ref("");
const busy = ref(true);
const ready = ref(false);
const sending = ref(false);
const pairingFp = ref("");
const manualAddr = ref("");
const adding = ref(false);
const addDialog = ref<HTMLDialogElement>();
const settingsDialog = ref<HTMLDialogElement>();
const pairedDialog = ref<HTMLDialogElement>();
const nameInput = ref("");
const discoverableInput = ref(true);
const acceptInput = ref(true);
const autoAcceptInput = ref(true);
const savingSettings = ref(false);
const pairRequest = ref<LanPairRequest | null>(null);
const pairDialog = ref<HTMLDialogElement>();
const responding = ref(false);
const incoming = ref<LanIncomingRequest | null>(null);
const incomingDialog = ref<HTMLDialogElement>();
const timelineEl = ref<HTMLElement>();
const menu = ref<{ x: number; y: number; item: ChatItem } | null>(null);
const menuEl = ref<HTMLElement>();
const menuReady = ref(false);
let menuTrigger: HTMLElement | null = null;

let unlisten: Array<() => void> = [];
let disposed = false;

const states: Record<LanTransferStateName, MessageKey> = {
  PENDING: "lan.statePending", WAITING_APPROVAL: "lan.stateWaiting", TRANSFERRING: "lan.stateTransferring",
  PAUSED: "lan.statePaused", COMPLETED: "lan.stateCompleted", FAILED: "lan.stateFailed", CANCELED: "lan.stateCanceled",
};
const stateLabel = (s: LanTransferStateName) => Object.hasOwn(states, s) ? t(states[s]) : s;

function deviceIcon(type: LanDeviceType | null) {
  return type === "mobile" ? Smartphone : type === "server" || type === "headless" ? Server : type === "web" ? Globe : Laptop;
}
function fmtSize(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = Math.max(0, bytes), i = 0;
  while (value >= 1024 && i < units.length - 1) { value /= 1024; i++; }
  return `${i && value < 10 ? value.toFixed(1) : value.toFixed(0)} ${units[i]}`;
}
// 微信式友好时间：今天只显时分，昨天加“昨天”，一周内用星期，更早加月日。
function friendlyTime(sec: number | null): string {
  if (!sec) return "";
  const d = new Date(sec * 1000), now = new Date(), loc = locale.value;
  const hm = new Intl.DateTimeFormat(loc, { hour: "2-digit", minute: "2-digit", hour12: false }).format(d);
  const dayStart = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round((dayStart(now) - dayStart(d)) / 86400000);
  if (diffDays <= 0) return hm;
  if (diffDays === 1) return `${t("lan.yesterday")} ${hm}`;
  if (diffDays < 7) return `${new Intl.DateTimeFormat(loc, { weekday: "short" }).format(d)} ${hm}`;
  return `${new Intl.DateTimeFormat(loc, { month: "numeric", day: "numeric" }).format(d)} ${hm}`;
}

function showNotice(message: string, kind: NoticeKind = "success") {
  if (disposed) return;
  void notifications.show({ body: message, kind, native: false });
}
function showError(error: unknown) { showNotice(tx(error instanceof Error ? error.message : String(error)), "error"); }

// 已配对设备（以可信列表为准，叠加在线/地址信息）。
const pairedList = computed(() =>
  trusted.value.map(tr => {
    const dev = devices.value.find(d => d.fingerprint === tr.fingerprint);
    return { fingerprint: tr.fingerprint, alias: dev?.alias || tr.alias, deviceType: dev?.deviceType ?? null, online: !!dev, address: dev?.address };
  }));
// 发现到、但尚未配对的设备。
const discoveredUnpaired = computed(() =>
  devices.value.filter(d => !d.trusted && !trusted.value.some(tr => tr.fingerprint === d.fingerprint)));
const selectedPeer = computed(() => {
  if (!selected.value) return null;
  const dev = devices.value.find(d => d.fingerprint === selected.value);
  const tr = trusted.value.find(x => x.fingerprint === selected.value);
  if (!dev && !tr) return null;
  return { fingerprint: selected.value, alias: dev?.alias || tr?.alias || selected.value, deviceType: dev?.deviceType ?? null, online: !!dev, address: dev?.address };
});
const timeline = computed<ChatItem[]>(() => {
  const live: ChatItem[] = liveTasks.value
    .filter(tk => tk.peerFingerprint === selected.value)
    .map(tk => ({
      id: tk.id, direction: tk.direction, peerAlias: tk.peerAlias, peerFingerprint: tk.peerFingerprint,
      fileName: tk.fileName, size: tk.size, state: tk.state, finishedAt: null,
      savePath: tk.savePath ?? null, text: null, live: true,
    }));
  return [...messages.value, ...live];
});
function transferProgress(item: ChatItem) {
  const sample = progress.value[item.id];
  const total = Math.max(0, sample?.bytesTotal ?? item.size);
  const done = item.state === 'COMPLETED' ? total : Math.min(total, Math.max(0, sample?.bytesDone ?? 0));
  return { total, done, percent: total ? Math.min(100, Math.floor(done / total * 100)) : item.state === 'COMPLETED' ? 100 : 0 };
}
// 微信式时间分隔：首条有时间的消息，或与上一条间隔≥5 分钟时，在其上方居中显示时间。
function showTimeDivider(index: number): boolean {
  const cur = timeline.value[index];
  if (!cur?.finishedAt) return false;
  const prev = timeline.value[index - 1];
  if (!prev?.finishedAt) return true;
  return cur.finishedAt - prev.finishedAt >= 300;
}
const canSend = computed(() => !!selectedPeer.value?.online && ready.value);
const draftValid = computed(() => !!draft.value.trim() && new TextEncoder().encode(draft.value).length <= 65536);

async function keepListener(listener?: Promise<() => void>) {
  if (!listener) return;
  const stop = await listener;
  if (disposed) stop(); else unlisten.push(stop);
}
async function scrollToEnd() {
  await nextTick();
  if (timelineEl.value) timelineEl.value.scrollTop = timelineEl.value.scrollHeight;
}

async function reloadTrusted() {
  try { trusted.value = await (await bridge()).lanListTrusted(); } catch (e) { showError(e); }
}
async function reloadConversation() {
  if (!selected.value) { messages.value = []; return; }
  try {
    const fp = selected.value;
    const rows = await (await bridge()).lanGetConversation(fp);
    if (!disposed && selected.value === fp) { messages.value = rows; void scrollToEnd(); }
  } catch (e) { showError(e); }
}

async function load() {
  busy.value = true;
  try {
    const b = await bridge();
    await b.lanStart();
    if (disposed) return;
    const [device, config, tasks] = await Promise.all([b.lanGetLocalDevice(), b.lanGetSettings(), b.lanListTransfers()]);
    if (disposed) return;
    localDevice.value = device; settings.value = config; liveTasks.value = tasks.filter(x => !["COMPLETED", "FAILED", "CANCELED"].includes(x.state));
    await reloadTrusted();
    unlisten.forEach(stop => stop()); unlisten = [];
    await keepListener(b.onLanDevices?.(d => { if (!disposed) devices.value = d; }));
    await keepListener(b.onLanPair?.(req => {
      if (disposed || pairRequest.value?.requestId === req.requestId) return;
      pairRequest.value = req;
    }));
    await keepListener(b.onLanIncoming?.(r => { if (!disposed && !incoming.value) incoming.value = r; }));
    await keepListener(b.onLanTransfer?.(task => {
      if (disposed) return;
      const terminal = ["COMPLETED", "FAILED", "CANCELED"].includes(task.state);
      if (terminal) {
        liveTasks.value = liveTasks.value.filter(x => x.id !== task.id);
        if (task.state === 'COMPLETED') { const { [task.id]: _drop, ...rest } = progress.value; progress.value = rest; }
        if (task.peerFingerprint === selected.value) void reloadConversation();
      } else {
        const i = liveTasks.value.findIndex(x => x.id === task.id);
        if (i >= 0) liveTasks.value[i] = task; else { liveTasks.value.push(task); if (task.peerFingerprint === selected.value) void scrollToEnd(); }
      }
    }));
    await keepListener(b.onLanProgress?.(p => {
      if (!disposed) progress.value = { ...progress.value, [p.taskId]: p };
    }));
    await keepListener(b.onLanText?.(row => {
      if (disposed) return;
      if (row.peerFingerprint === selected.value) void reloadConversation();
      else showNotice(t("lan.textReceived"), "info");
    }));
    if (disposed) return;
    await b.lanRefreshDevices();
    if (!disposed) ready.value = true;
  } catch (error) { showError(error); }
  finally { if (!disposed) busy.value = false; }
}

async function refresh() {
  if (busy.value) return;
  if (!ready.value) return load();
  try { await (await bridge()).lanRefreshDevices(); await reloadTrusted(); }
  catch (error) { showError(error); }
}

function selectDevice(fp: string) {
  if (selected.value === fp) return;
  selected.value = fp; messages.value = []; closeMenu();
  void reloadConversation();
}

async function pairDevice(fp: string) {
  if (pairingFp.value) return;
  pairingFp.value = fp;
  try {
    await (await bridge()).lanPair(fp);
    if (disposed) return;
    await reloadTrusted();
    showNotice(t("lan.paired"));
    selectDevice(fp);
  } catch (error) { showError(error); }
  finally { if (!disposed) pairingFp.value = ""; }
}

async function respondPair(accept: boolean) {
  const req = pairRequest.value;
  if (!req || responding.value) return;
  responding.value = true;
  try {
    await (await bridge()).lanRespondPair(req.requestId, accept);
    if (disposed) return;
    pairRequest.value = null;
    if (accept) { await reloadTrusted(); showNotice(t("lan.paired")); }
  } catch (error) { showError(error); }
  finally { if (!disposed) responding.value = false; }
}

async function unpairDevice(fp: string) {
  if (!fp || !window.confirm(t("lan.unpair") + "?")) return;
  try {
    await (await bridge()).lanUnpair(fp);
    if (disposed) return;
    await reloadTrusted();
    if (selected.value === fp) { selected.value = ""; messages.value = []; }
    showNotice(t("lan.unpaired"));
  } catch (error) { showError(error); }
}
function unpairSelected() { void unpairDevice(selected.value); }

function openAdd() { manualAddr.value = ""; addDialog.value?.showModal(); }
function openPaired() { void reloadTrusted(); pairedDialog.value?.showModal(); }
function openSettings() {
  if (!settings.value) return;
  nameInput.value = settings.value.alias;
  discoverableInput.value = settings.value.discoverable;
  acceptInput.value = settings.value.acceptIncoming;
  autoAcceptInput.value = settings.value.autoAcceptTrusted;
  settingsDialog.value?.showModal();
  void reloadTrusted();
}
async function addManual() {
  const addr = manualAddr.value.trim();
  if (adding.value || !addr) return;
  adding.value = true;
  try { await (await bridge()).lanAddManualDevice(addr); manualAddr.value = ""; addDialog.value?.close(); showNotice(t("lan.added")); }
  catch (error) { showError(error); }
  finally { if (!disposed) adding.value = false; }
}
async function saveSettings() {
  if (!settings.value || savingSettings.value || !nameInput.value.trim()) return;
  savingSettings.value = true;
  try {
    const updated = await (await bridge()).lanUpdateSettings({ ...settings.value, alias: nameInput.value.trim(), discoverable: discoverableInput.value, acceptIncoming: acceptInput.value, autoAcceptTrusted: autoAcceptInput.value });
    if (disposed) return;
    settings.value = updated;
    settingsDialog.value?.close();
    showNotice(t("lan.saved"));
  } catch (error) { showError(error); }
  finally { if (!disposed) savingSettings.value = false; }
}

function onComposerEnter(event: KeyboardEvent) {
  if (event.isComposing || event.keyCode === 229) return;
  event.preventDefault(); void sendText();
}
async function sendText() {
  if (!canSend.value || !draftValid.value || sending.value) return;
  sending.value = true;
  const body = draft.value, fingerprint = selected.value;
  try {
    await (await bridge()).lanSendText(fingerprint, body);
    if (disposed) return;
    if (selected.value === fingerprint && draft.value === body) draft.value = "";
    await reloadConversation();
  } catch (error) { showError(error); }
  finally { if (!disposed) sending.value = false; }
}

async function pickAndSend() {
  if (!canSend.value) return;
  try {
    const b = await bridge();
    const paths = await b.lanPickFiles();
    if (disposed || !paths.length) return;
    await b.lanSendFiles(selected.value, paths);
  } catch (error) { showError(error); }
}

async function pasteFiles() {
  closeMenu();
  if (!canSend.value) return;
  try {
    const files = await (await bridge()).lanClipboardFiles();
    if (disposed) return;
    if (!files.length) { showNotice(t("lan.noClipFiles"), "info"); return; }
    await (await bridge()).lanSendFiles(selected.value, files);
  } catch (error) { showError(error); }
}
async function onComposerPaste(event: ClipboardEvent) {
  if (event.clipboardData?.files.length) { event.preventDefault(); await pasteFiles(); }
}

// —— 气泡右键菜单 ——
async function openMenu(event: MouseEvent | KeyboardEvent, item: ChatItem) {
  event.preventDefault();
  closeMenu();
  if (item.live) return;
  const trigger = event.currentTarget as HTMLElement;
  menuTrigger = trigger.querySelector<HTMLButtonElement>('button:not(:disabled)') ?? trigger;
  const bounds = trigger.getBoundingClientRect();
  menuReady.value = false;
  menu.value = { x: event instanceof MouseEvent ? event.clientX : bounds.left, y: event instanceof MouseEvent ? event.clientY : bounds.bottom, item };
  const opened = menu.value;
  await nextTick();
  if (menu.value !== opened || !menuEl.value) return;
  const rect = menuEl.value.getBoundingClientRect(), gap = 8;
  opened.x = Math.max(gap, Math.min(opened.x, window.innerWidth - rect.width - gap));
  opened.y = Math.max(gap, Math.min(opened.y, window.innerHeight - rect.height - gap));
  menuReady.value = true;
  await nextTick();
  if (menu.value === opened) menuEl.value?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus({ preventScroll: true });
}
function closeMenu() { menu.value = null; }
function onBubbleKeydown(event: KeyboardEvent, item: ChatItem) {
  if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) void openMenu(event, item);
}
function onMenuKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { event.preventDefault(); closeMenu(); menuTrigger?.focus({ preventScroll: true }); return; }
  if (event.key === 'Tab') { closeMenu(); return; }
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
  event.preventDefault();
  const items = Array.from(menuEl.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? []);
  if (!items.length) return;
  const index = items.indexOf(document.activeElement as HTMLButtonElement);
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length;
  items[next]?.focus();
}
async function openFile(item: ChatItem) { closeMenu(); if (item.savePath) { try { await (await bridge()).lanOpenPath(item.savePath); } catch (e) { showError(e); } } }
async function revealFile(item: ChatItem) { closeMenu(); if (item.savePath) { try { await (await bridge()).lanRevealPath(item.savePath); } catch (e) { showError(e); } } }
async function copyFile(item: ChatItem) { closeMenu(); if (item.savePath) { try { await (await bridge()).lanCopyFileToClipboard(item.savePath); showNotice(t("lan.fileCopied")); } catch (e) { showError(e); } } }
async function copyTextItem(item: ChatItem) {
  closeMenu();
  if (item.text == null) return;
  try { await navigator.clipboard.writeText(item.text); showNotice(t("lan.textCopied")); }
  catch (e) { showError(e); }
}
async function deleteItem(item: ChatItem) {
  closeMenu();
  if (!window.confirm(t("lan.deleteConfirm"))) return;
  try {
    await (await bridge()).lanDeleteFile(item.id, item.savePath ?? null);
    if (disposed) return;
    messages.value = messages.value.filter(m => !(m.id === item.id && m.direction === item.direction));
    showNotice(t("lan.deleted"));
  } catch (e) { showError(e); }
}

async function cancelTask(id: string) { try { await (await bridge()).lanCancelTransfer(id); } catch (e) { showError(e); } }
async function pauseTask(id: string) { try { await (await bridge()).lanPauseTransfer(id); } catch (e) { showError(e); } }
async function resumeTask(id: string) { try { await (await bridge()).lanResumeTransfer(id); } catch (e) { showError(e); } }

watch(pairRequest, async () => {
  await nextTick();
  if (disposed) return;
  if (pairRequest.value && !pairDialog.value?.open) pairDialog.value?.showModal();
  else if (!pairRequest.value) pairDialog.value?.close();
});
watch(incoming, async () => {
  await nextTick();
  if (disposed) return;
  if (incoming.value && !incomingDialog.value?.open) incomingDialog.value?.showModal();
  else if (!incoming.value) incomingDialog.value?.close();
});
async function respondIncoming(accept: boolean) {
  if (responding.value || !incoming.value) return;
  const req = incoming.value;
  responding.value = true;
  try { await (await bridge()).lanRespondIncoming(req.taskId, accept, null); if (!disposed) incoming.value = null; }
  catch (error) { showError(error); }
  finally { if (!disposed) responding.value = false; }
}

onMounted(() => {
  window.addEventListener("click", closeMenu);
  window.addEventListener("resize", closeMenu);
  window.addEventListener("blur", closeMenu);
  void load();
});
onBeforeUnmount(() => {
  disposed = true;
  unlisten.forEach(fn => fn()); unlisten = [];
  window.removeEventListener("click", closeMenu);
  window.removeEventListener("resize", closeMenu);
  window.removeEventListener("blur", closeMenu);
  pairDialog.value?.close(); incomingDialog.value?.close(); addDialog.value?.close(); settingsDialog.value?.close(); pairedDialog.value?.close();
});
</script>

<template>
  <section class="lan" :aria-label="t('nav.lanTransfer')">
    <div class="chat-shell">
      <!-- 左栏：已配对 + 可配对设备 -->
      <aside class="sidebar">
        <header class="sidebar-top">
          <div class="rail-heading"><strong class="rail-title">{{ t('lan.devices') }}</strong>
          <span v-if="localDevice && settings" class="local-state" role="img" :title="`${localDevice.alias} · ${t(settings.discoverable ? 'lan.visible' : 'lan.hidden')}`" :aria-label="`${localDevice.alias} · ${t(settings.discoverable ? 'lan.visible' : 'lan.hidden')}`"><span class="dot" :class="{ on: settings.discoverable }" /></span></div>
          <div class="rail-tools">
          <button type="button" class="icon-btn" :disabled="busy" :title="t('lan.refresh')" :aria-label="t('lan.refresh')" @click="refresh"><RefreshCw :size="16" :class="{ spin: busy }" /></button>
          <button type="button" class="icon-btn add-btn" :title="t('lan.addDevice')" :aria-label="t('lan.addDevice')" @click="openAdd"><Plus :size="17" /></button>
          <button type="button" class="icon-btn" :title="t('lan.pairedSection')" :aria-label="t('lan.pairedSection')" @click="openPaired"><ShieldCheck :size="16" /></button>
          <button type="button" class="icon-btn" :title="t('lan.settingsTitle')" :aria-label="t('lan.settingsTitle')" @click="openSettings"><Settings :size="16" /></button>
          </div>
        </header>

        <div class="sidebar-scroll">
          <template v-if="pairedList.length">
            <p class="group-label">{{ t("lan.pairedSection") }}<span class="count">{{ pairedList.length }}</span></p>
            <button v-for="d in pairedList" :key="d.fingerprint" type="button" class="conv" :aria-pressed="selected === d.fingerprint" :class="{ active: selected === d.fingerprint, offline: !d.online }" @click="selectDevice(d.fingerprint)">
              <span class="conv-meta"><strong :title="d.alias">{{ d.alias }}</strong><small :title="d.address"><span class="dot" :class="{ on: d.online }" /><span>{{ t(d.online ? "lan.peerOnline" : "lan.peerOffline") }}</span><span v-if="d.online && d.address" class="peer-address"> · {{ d.address }}</span></small></span>
            </button>
          </template>

          <template v-if="discoveredUnpaired.length">
            <p class="group-label">{{ t("lan.discoveredSection") }}<span class="count">{{ discoveredUnpaired.length }}</span></p>
            <div v-for="d in discoveredUnpaired" :key="d.fingerprint" class="conv discover">
              <span class="conv-meta"><strong :title="d.alias">{{ d.alias }}</strong><small :title="d.address">{{ d.address }}</small></span>
              <button type="button" class="pair-btn" :aria-label="`${t('lan.pair')} · ${d.alias}`" :disabled="!!pairingFp || !ready" :aria-busy="pairingFp === d.fingerprint" @click="pairDevice(d.fingerprint)"><Link2 :size="13" />{{ t(pairingFp === d.fingerprint ? "lan.pairing" : "lan.pair") }}</button>
            </div>
          </template>

          <div v-if="!pairedList.length && !discoveredUnpaired.length" class="sidebar-empty" role="status">
            <Radio :size="26" :stroke-width="1.4" />
            <strong>{{ t(busy ? "lan.searching" : "lan.noDevices") }}</strong>
            <p>{{ t("lan.deviceHint") }}</p>
          </div>
        </div>

      </aside>

      <!-- 右栏：会话 -->
      <div class="chat">
        <template v-if="selectedPeer">
          <header class="chat-head">
            <span class="chat-head-icon"><component :is="deviceIcon(selectedPeer.deviceType)" :size="18" /></span>
            <div class="chat-head-meta">
              <strong :title="selectedPeer.alias">{{ selectedPeer.alias }}</strong>
              <small :title="selectedPeer.address"><span class="dot" :class="{ on: selectedPeer.online }" />{{ t(selectedPeer.online ? "lan.peerOnline" : "lan.peerOffline") }}<span v-if="selectedPeer.address" class="peer-address"> · {{ selectedPeer.address }}</span></small>
            </div>
            <button type="button" class="icon-btn danger" :title="t('lan.unpair')" :aria-label="t('lan.unpair')" @click="unpairSelected"><Unlink :size="16" /></button>
          </header>

          <div ref="timelineEl" class="timeline" @scroll="closeMenu">
            <div v-if="!timeline.length" class="timeline-empty"><MessageSquare :size="26" :stroke-width="1.4" /><span>{{ t("lan.emptyChat") }}</span></div>
            <template v-for="(item, index) in timeline" :key="`${item.id}-${item.direction}`">
            <div v-if="showTimeDivider(index)" class="time-divider"><span>{{ friendlyTime(item.finishedAt) }}</span></div>
            <div class="row" :class="item.direction === 'send' ? 'me' : 'peer'">
              <div class="bubble" :class="{ file: item.text == null }" :tabindex="item.text != null || !item.savePath ? 0 : undefined" @contextmenu="openMenu($event, item)" @keydown="onBubbleKeydown($event, item)">
                <!-- 文本气泡 -->
                <p v-if="item.text != null" class="bubble-text">{{ item.text }}</p>
                <!-- 文件气泡 -->
                <template v-else>
                  <button type="button" class="file-row" :disabled="!item.savePath || item.live" :aria-label="`${t('lan.openFile')} · ${item.fileName}`" @click="openFile(item)">
                    <span class="file-ico"><FileTypeIcon :name="item.fileName" /></span>
                    <span class="file-info"><strong :title="item.fileName">{{ item.fileName }}</strong><span class="file-meta"><small>{{ fmtSize(item.size) }}</small><small v-if="!item.live" class="file-state" :data-state="item.state"><Check v-if="item.state === 'COMPLETED'" :size="12" />{{ stateLabel(item.state) }}</small></span></span>
                  </button>
                  <div v-if="item.live" class="prog" :class="{ failed: item.state === 'FAILED' || item.state === 'CANCELED' }">
                    <div class="prog-bar" role="progressbar" :aria-label="`${item.fileName} · ${stateLabel(item.state)}`" :aria-valuenow="transferProgress(item).percent" aria-valuemin="0" aria-valuemax="100"><i :style="{ transform: `scaleX(${transferProgress(item).percent / 100})` }" /></div>
                    <div class="progress-metrics"><span>{{ fmtSize(transferProgress(item).done) }} / {{ fmtSize(transferProgress(item).total) }}</span><span>{{ transferProgress(item).percent }}%</span></div>
                    <div class="prog-act">
                      <span>{{ stateLabel(item.state) }}</span>
                      <button v-if="item.state === 'TRANSFERRING'" type="button" :title="t('lan.pause')" :aria-label="t('lan.pause')" @click="pauseTask(item.id)"><Pause :size="13" /></button>
                      <button v-if="item.state === 'PAUSED'" type="button" :title="t('lan.resume')" :aria-label="t('lan.resume')" @click="resumeTask(item.id)"><Play :size="13" /></button>
                      <button type="button" :title="t('lan.cancel')" :aria-label="t('lan.cancel')" @click="cancelTask(item.id)"><X :size="13" /></button>
                    </div>
                  </div>
                </template>
              </div>
            </div>
            </template>
          </div>

          <footer class="composer">
            <button type="button" class="icon-btn" :disabled="!canSend" :title="t('lan.send')" :aria-label="t('lan.send')" @click="pickAndSend"><Paperclip :size="18" /></button>
            <div class="composer-input">
            <textarea v-model="draft" :aria-label="t('lan.msgPlaceholder')" :placeholder="canSend ? t('lan.msgPlaceholder') : t('lan.pairToStart')" :disabled="!canSend" rows="1" maxlength="65536" @paste="onComposerPaste" @keydown.enter.exact="onComposerEnter" />
            <button type="button" class="send-btn" :disabled="!canSend || !draftValid || sending" :title="t('lan.sendMessage')" :aria-label="t('lan.sendMessage')" @click="sendText"><ArrowUp :size="16" /></button></div>
          </footer>
        </template>

        <div v-else class="chat-placeholder">
          <span class="placeholder-mark"><MessageSquare :size="27" :stroke-width="1.5" /></span>
          <strong>{{ t("lan.pickChat") }}</strong>
          <p>{{ t("lan.pairToStart") }}</p>
        </div>
      </div>
    </div>

    <!-- 气泡右键菜单 -->
    <Teleport to="body">
    <div v-if="menu" ref="menuEl" class="ctx-menu" role="menu" :style="{ left: menu.x + 'px', top: menu.y + 'px', visibility: menuReady ? 'visible' : 'hidden' }" @click.stop @contextmenu.prevent @keydown="onMenuKeydown">
      <template v-if="menu.item.text != null">
        <button type="button" role="menuitem" @click="copyTextItem(menu.item)"><Copy :size="15" />{{ t("lan.copy") }}</button>
        <button type="button" role="menuitem" @click="pasteFiles"><ClipboardPaste :size="15" />{{ t("lan.paste") }}</button>
        <button type="button" role="menuitem" class="danger" @click="deleteItem(menu.item)"><Trash2 :size="15" />{{ t("lan.delete") }}</button>
      </template>
      <template v-else>
        <button type="button" role="menuitem" :disabled="!menu.item.savePath" :title="!menu.item.savePath ? t('native.lanFileUnavailable') : undefined" @click="copyFile(menu.item)"><Copy :size="15" />{{ t("lan.copy") }}</button>
        <button type="button" role="menuitem" class="danger" @click="deleteItem(menu.item)"><Trash2 :size="15" />{{ t("lan.delete") }}</button>
        <div class="ctx-separator" role="separator" />
        <button type="button" role="menuitem" :disabled="!menu.item.savePath" :title="!menu.item.savePath ? t('native.lanFileUnavailable') : undefined" @click="openFile(menu.item)"><FileIcon :size="15" />{{ t("lan.openFile") }}</button>
        <button type="button" role="menuitem" :disabled="!menu.item.savePath" :title="!menu.item.savePath ? t('native.lanFileUnavailable') : undefined" @click="revealFile(menu.item)"><FolderOpen :size="15" />{{ t("lan.openFolder") }}</button>
      </template>
    </div>
    </Teleport>

    <!-- 添加设备 -->
    <dialog ref="addDialog" class="incoming-dialog add-device-dialog" aria-labelledby="lan-add-title" @close="manualAddr = ''">
      <form @submit.prevent="addManual">
        <header class="incoming-heading"><span class="incoming-icon"><Plus :size="22" /></span><div><h2 id="lan-add-title">{{ t("lan.addDevice") }}</h2></div></header>
        <label class="field-label" for="lan-add-input">{{ t("lan.manualAdd") }}</label>
        <div class="dialog-field"><Link2 class="field-icon" :size="15" /><input id="lan-add-input" v-model="manualAddr" type="text" inputmode="url" :placeholder="t('lan.manualPlaceholder')" :disabled="adding" /></div>
        <div class="incoming-actions"><button type="button" :disabled="adding" @click="addDialog?.close()">{{ t("common.cancel") }}</button><button type="submit" class="primary" :disabled="adding || !manualAddr.trim()"><Plus :size="15" />{{ t("lan.addDevice") }}</button></div>
      </form>
    </dialog>

    <!-- 设置 -->
    <dialog ref="settingsDialog" class="incoming-dialog" aria-labelledby="lan-settings-title">
      <form @submit.prevent="saveSettings">
        <header class="incoming-heading"><span class="incoming-icon"><Settings :size="22" /></span><div><h2 id="lan-settings-title">{{ t("lan.settingsTitle") }}</h2></div></header>
        <label class="field-label" for="lan-name-input">{{ t("lan.deviceName") }}</label>
        <div class="dialog-field"><input id="lan-name-input" v-model="nameInput" type="text" maxlength="64" :placeholder="t('lan.deviceName')" :disabled="savingSettings" /></div>
        <label class="toggle-row"><span>{{ t("lan.visible") }}</span><input v-model="discoverableInput" type="checkbox" role="switch" :disabled="savingSettings" /><span class="switch" aria-hidden="true" /></label>
        <label class="toggle-row"><span>{{ t("lan.acceptIncoming") }}</span><input v-model="acceptInput" type="checkbox" role="switch" :disabled="savingSettings" /><span class="switch" aria-hidden="true" /></label>
        <label class="toggle-row"><span>{{ t("lan.autoAccept") }}</span><input v-model="autoAcceptInput" type="checkbox" role="switch" :disabled="savingSettings || !acceptInput" /><span class="switch" aria-hidden="true" /></label>
        <div class="incoming-actions"><button type="button" :disabled="savingSettings" @click="settingsDialog?.close()">{{ t("common.cancel") }}</button><button type="submit" class="primary" :disabled="savingSettings || !nameInput.trim()"><Check :size="15" />{{ t("lan.save") }}</button></div>
      </form>
    </dialog>

    <!-- 已配对设备管理 -->
    <dialog ref="pairedDialog" class="incoming-dialog paired-dialog" aria-labelledby="lan-paired-title">
      <header class="incoming-heading"><span class="incoming-icon"><ShieldCheck :size="22" /></span><div><h2 id="lan-paired-title">{{ t("lan.pairedSection") }} · {{ trusted.length }}</h2></div></header>
      <div class="paired-manage">
        <div v-for="d in trusted" :key="d.fingerprint" class="paired-item">
          <span class="paired-ico"><ShieldCheck :size="16" /></span>
          <span class="paired-meta"><strong :title="d.alias">{{ d.alias }}</strong><small :title="d.fingerprint">{{ d.fingerprint.slice(0, 12) }}…</small></span>
          <button type="button" class="paired-unpair" :title="t('lan.unpair')" :aria-label="`${t('lan.unpair')} · ${d.alias}`" @click="unpairDevice(d.fingerprint)"><Unlink :size="14" /></button>
        </div>
        <p v-if="!trusted.length" class="paired-empty">{{ t("lan.noPaired") }}</p>
      </div>
      <div class="incoming-actions"><button type="button" @click="pairedDialog?.close()">{{ t("common.close") }}</button></div>
    </dialog>

    <!-- 配对请求 -->
    <dialog ref="pairDialog" class="incoming-dialog" aria-labelledby="lan-pair-title" @cancel.prevent="respondPair(false)">
      <template v-if="pairRequest">
        <header class="incoming-heading"><span class="incoming-icon"><Link2 :size="22" /></span><div><h2 id="lan-pair-title">{{ t("lan.pairRequestTitle") }}</h2><p>{{ t("lan.from") }} <b>{{ pairRequest.peerAlias }}</b></p></div></header>
        <p class="pair-desc">{{ t("lan.pairDesc") }}</p>
        <div class="incoming-actions"><button type="button" :disabled="responding" @click="respondPair(false)"><X :size="15" />{{ t("lan.reject") }}</button><button type="button" class="primary" :disabled="responding" @click="respondPair(true)"><Check :size="15" />{{ t("lan.accept") }}</button></div>
      </template>
    </dialog>

    <!-- 未配对（三方 LocalSend）回退：逐次确认 -->
    <dialog ref="incomingDialog" class="incoming-dialog" aria-labelledby="lan-incoming-title" @cancel.prevent="respondIncoming(false)">
      <template v-if="incoming">
        <header class="incoming-heading"><span class="incoming-icon"><Radio :size="22" /></span><div><h2 id="lan-incoming-title">{{ t(incoming.text != null ? "lan.incomingTextTitle" : "lan.incomingTitle") }}</h2><p>{{ t("lan.from") }} <b>{{ incoming.peerAlias }}</b> · {{ fmtSize(incoming.totalSize) }}</p></div></header>
        <pre v-if="incoming.text != null" class="message-text">{{ incoming.text }}</pre>
        <ul v-else class="incoming-files"><li v-for="(f, i) in incoming.files" :key="i"><FileTypeIcon :name="f.name" :size="18" /><span :title="f.name">{{ f.name }}</span><small>{{ fmtSize(f.size) }}</small></li></ul>
        <div class="incoming-actions"><button type="button" :disabled="responding" @click="respondIncoming(false)"><X :size="15" />{{ t("lan.reject") }}</button><button type="button" class="primary" :disabled="responding" @click="respondIncoming(true)"><Check :size="15" />{{ t("lan.accept") }}</button></div>
      </template>
    </dialog>
  </section>
</template>

<style scoped>
.lan { display:flex; flex:1; flex-direction:column; gap:10px; min-width:0; min-height:0; container-type:inline-size; container-name:lan-chat; }
.chat-shell { display:flex; flex:1; min-width:0; min-height:0; border:1px solid var(--line); border-radius:12px; background:var(--bg); overflow:hidden; }
.icon-btn { display:grid; place-items:center; flex:none; width:32px; height:32px; min-height:32px; padding:0; border:0; border-radius:8px; background:transparent; color:var(--muted); }
.icon-btn:hover:not(:disabled) { background:var(--selected); color:var(--accent-text); }
.icon-btn:disabled { opacity:.45; }
.icon-btn.danger:hover:not(:disabled) { background:var(--danger-bg); color:var(--danger); }
.dot { display:inline-block; width:6px; height:6px; margin-right:5px; border-radius:50%; background:var(--muted); vertical-align:middle; }
.dot.on { background:var(--success); }

/* Device rail */
.sidebar { display:flex; flex:0 0 clamp(228px,26%,256px); flex-direction:column; min-width:0; min-height:0; border-right:1px solid var(--line); background:var(--bg); }
.sidebar-top { display:flex; align-items:center; gap:8px; min-height:56px; flex:none; padding:10px 12px; }
.rail-heading { display:flex; align-items:center; gap:7px; flex:1; min-width:0; }
.rail-title { overflow:hidden; white-space:nowrap; text-overflow:ellipsis; font-size:13px; font-weight:600; }
.local-state { display:grid; place-items:center; flex:none; }
.local-state .dot { margin:0; }
.rail-tools { display:flex; align-items:center; flex:none; gap:2px; }
.rail-tools .icon-btn { width:28px; height:28px; min-height:28px; }
.sidebar-scroll { flex:1; min-height:0; overflow:auto; padding:4px 8px 12px; scrollbar-width:thin; scrollbar-color:var(--scrollbar) transparent; }
.group-label { display:flex; align-items:center; gap:7px; margin:16px 10px 7px; color:var(--muted); font-size:12px; font-weight:500; }
.group-label:first-child { margin-top:4px; }
.group-label .count { font-weight:400; font-variant-numeric:tabular-nums; }
.conv { display:flex; align-items:center; gap:8px; width:100%; min-height:52px; margin:2px 0; padding:8px 10px; border:0; border-radius:8px; background:transparent; color:var(--text); text-align:left; cursor:pointer; }
.conv:hover { background:var(--hover); }
.conv.active { background:var(--nav-on); color:var(--text); }
.conv.active .conv-meta strong { color:var(--accent-text); }
.conv:focus-visible { outline:2px solid var(--focus); outline-offset:-2px; }
.conv-meta { display:flex; flex-direction:column; gap:3px; flex:1; min-width:0; }
.conv-meta strong { font-size:13px; font-weight:500; overflow:hidden; white-space:nowrap; text-overflow:ellipsis; }
.conv-meta small { color:var(--muted); font-size:12px; overflow:hidden; white-space:nowrap; text-overflow:ellipsis; }
.conv.discover { cursor:default; }
.pair-btn { display:inline-flex; align-items:center; gap:4px; flex:none; min-height:28px; padding:4px 6px; border:0; border-radius:8px; background:transparent; color:var(--accent-text); font-size:12px; }
.pair-btn:hover:not(:disabled) { background:var(--selected); }
.pair-btn:disabled { opacity:.6; }
.sidebar-empty { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:9px; min-height:180px; padding:24px 14px; text-align:center; color:var(--muted); }
.sidebar-empty strong { font-size:13px; font-weight:500; color:var(--text); }
.sidebar-empty p { max-width:30ch; margin:0; font-size:12px; line-height:1.6; }

/* Conversation */
.chat { display:flex; flex:1; flex-direction:column; min-width:0; min-height:0; background:var(--bg); }
.chat-head { display:flex; align-items:center; gap:9px; min-height:56px; flex:none; padding:10px 20px; border-bottom:1px solid var(--line); background:var(--bg); }
.chat-head-icon { display:grid; place-items:center; flex:none; width:24px; height:28px; color:var(--muted); }
.chat-head-meta { display:flex; flex-direction:column; gap:2px; flex:1; min-width:0; }
.chat-head-meta strong { font-size:13px; font-weight:600; overflow:hidden; white-space:nowrap; text-overflow:ellipsis; }
.chat-head-meta small { color:var(--muted); font-size:12px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.peer-address { font-variant-numeric:tabular-nums; }
.timeline { flex:1; min-height:0; overflow:auto; padding:20px 24px; display:flex; flex-direction:column; gap:10px; background:color-mix(in srgb,var(--surface) 65%,var(--bg)); scrollbar-width:thin; scrollbar-color:var(--scrollbar) transparent; }
.timeline-empty { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; flex:1; min-height:140px; color:var(--muted); font-size:12px; text-align:center; }
.row { display:flex; flex:none; flex-direction:column; gap:4px; max-width:min(86%,480px); min-width:0; }
.row.me { align-self:flex-end; align-items:flex-end; }
.row.peer { align-self:flex-start; align-items:flex-start; }
.bubble { max-width:100%; padding:9px 13px; border:1px solid transparent; border-radius:12px; background:var(--surface); min-width:0; }
.row.peer .bubble { border-bottom-left-radius:5px; }
.row.me .bubble { border-bottom-right-radius:5px; }
.row.me .bubble:not(.file) { background:var(--nav-on); }
.bubble-text { margin:0; font-size:13px; line-height:1.6; white-space:pre-wrap; overflow-wrap:anywhere; user-select:text; -webkit-user-select:text; }
.bubble.file,.row.me .bubble.file { padding:12px 14px; border-color:var(--line); background:var(--bg); }
.file-row { display:flex; justify-content:flex-start; align-items:center; gap:10px; width:280px; max-width:100%; min-width:0; min-height:40px; padding:0; border:0; border-radius:8px; background:transparent; color:var(--text); text-align:left; }
.file-row:hover:not(:disabled) { background:transparent; color:var(--accent-text); }
.file-row:disabled { opacity:1; cursor:default; }
.file-ico { display:grid; place-items:center; flex:none; width:36px; height:40px; color:var(--accent-text); }
.file-info { display:flex; flex-direction:column; gap:3px; flex:1; min-width:0; }
.file-info strong { font-size:13px; font-weight:500; overflow:hidden; white-space:nowrap; text-overflow:ellipsis; }
.file-meta { display:flex; align-items:center; flex-wrap:wrap; gap:4px 12px; margin-top:2px; }
.file-meta > small { color:var(--muted); font-size:12px; font-variant-numeric:tabular-nums; }
.file-state { display:inline-flex; align-items:center; gap:3px; margin-left:auto; font-size:12px; }
.file-state[data-state="COMPLETED"] { color:var(--success); }
.file-state[data-state="FAILED"],.file-state[data-state="CANCELED"] { color:var(--danger); }
.prog { margin-top:10px; }
.prog-bar { height:4px; border-radius:8px; background:var(--line); overflow:hidden; }
.progress-metrics { display:flex; align-items:center; justify-content:space-between; gap:10px; margin-top:6px; color:var(--muted); font-size:12px; font-variant-numeric:tabular-nums; }
.prog.failed .prog-bar i { background:var(--danger); }
.prog-bar i { display:block; width:100%; height:100%; background:var(--accent); transform-origin:left; transition:transform .2s ease; }
.prog-act { display:flex; align-items:center; gap:4px; margin-top:6px; color:var(--muted); font-size:12px; }
.prog-act span { flex:1; min-width:0; font-variant-numeric:tabular-nums; }
.prog-act button { display:grid; place-items:center; width:24px; height:24px; min-height:24px; padding:0; border:0; border-radius:8px; background:transparent; color:var(--muted); }
.prog-act button:hover { background:var(--bg); color:var(--text); }
.time-divider { align-self:center; flex:none; margin:2px 0 8px; color:var(--muted); font-size:12px; font-variant-numeric:tabular-nums; }
.add-btn { color:var(--accent-text); }
.composer { display:flex; align-items:center; gap:8px; flex:none; min-height:64px; padding:12px 20px; border-top:1px solid var(--line); background:var(--bg); }
.composer-input { display:flex; align-items:flex-end; gap:8px; flex:1; min-width:0; padding:6px; border:1px solid transparent; border-radius:10px; background:color-mix(in srgb,var(--surface) 70%,var(--bg)); }
.composer-input:hover { border-color:var(--line); }
.composer-input:focus-within { border-color:var(--border); }
.composer textarea { display:block; flex:1; width:0; min-width:0; min-height:28px; max-height:120px; resize:none; padding:4px 0; border:0; border-radius:0; outline:0; background:transparent; color:var(--text); font:inherit; font-size:13px; line-height:1.5; }
.composer textarea:disabled { opacity:.7; }
.send-btn { display:grid; place-items:center; flex:none; width:28px; height:28px; min-height:28px; padding:0; border:0; border-radius:8px; background:var(--accent); color:#fff; }
.send-btn:hover:not(:disabled) { background:var(--primary-hover); color:#fff; }
.send-btn:disabled { opacity:1; background:transparent; color:var(--muted); }
.chat-placeholder { display:flex; flex:1; flex-direction:column; align-items:center; justify-content:center; gap:12px; padding:28px; text-align:center; color:var(--muted); background:var(--bg); }
.placeholder-mark { display:grid; place-items:center; width:58px; height:58px; margin-bottom:4px; border-radius:12px; background:var(--surface); color:var(--strong); }
.chat-placeholder strong { font-size:13px; font-weight:500; color:var(--text); }
.chat-placeholder p { max-width:34ch; margin:0; font-size:12px; line-height:1.7; }

/* 右键菜单 */
.ctx-menu { position:fixed; z-index:50; width:max-content; min-width:min(168px,calc(100vw - 16px)); max-width:calc(100vw - 16px); max-height:calc(100dvh - 16px); overflow:auto; box-sizing:border-box; padding:5px; border:1px solid var(--line); border-radius:10px; background:var(--bg); box-shadow:var(--shadow-hover); }
.ctx-menu button { display:flex; align-items:center; justify-content:flex-start; gap:9px; width:100%; min-height:32px; padding:6px 9px; border:0; border-radius:8px; background:transparent; color:var(--text); font-size:13px; text-align:left; }
.ctx-menu button:hover:not(:disabled),.ctx-menu button:focus-visible { background:var(--selected); color:var(--accent-text); }
.ctx-menu button:disabled { opacity:.5; cursor:default; }
.ctx-separator { margin:4px; border-top:1px solid var(--line); }
.ctx-menu button svg { flex:none; color:var(--muted); }
.ctx-menu button:hover svg { color:inherit; }
.ctx-menu button.danger:hover { background:var(--danger-bg); color:var(--danger); }

/* 对话框 */
.incoming-dialog { width:400px; max-width:calc(100vw - 32px); max-height:calc(100vh - 32px); overflow:auto; padding:22px; border:0; border-radius:14px; background:var(--bg); color:var(--text); box-shadow:var(--shadow-hover); }
.incoming-dialog::backdrop { background:#18293e66; }
.incoming-heading { display:flex; align-items:flex-start; gap:12px; }
.incoming-icon { display:grid; place-items:center; flex:none; width:40px; height:40px; border-radius:10px; background:var(--selected); color:var(--accent-text); }
.incoming-heading h2 { margin:0; font-size:16px; font-weight:600; }
.incoming-heading p { margin:5px 0 0; color:var(--muted); font-size:12px; overflow-wrap:anywhere; }
.incoming-heading b { color:var(--text); font-weight:500; }
.pair-desc { margin:16px 0 20px; color:var(--muted); font-size:13px; line-height:1.6; }
.message-text { margin:16px 0; padding:12px; max-height:240px; overflow:auto; border-radius:8px; background:var(--surface); font-family:inherit; font-size:13px; line-height:1.6; white-space:pre-wrap; overflow-wrap:anywhere; user-select:text; -webkit-user-select:text; }
.incoming-files { padding:4px 12px; margin:16px 0; list-style:none; max-height:200px; overflow:auto; border-radius:8px; background:var(--surface); }
.incoming-files li { display:flex; align-items:center; gap:8px; padding:9px 0; font-size:12px; }
.incoming-files svg { flex:none; color:var(--muted); }
.incoming-files span { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.incoming-files small { flex:none; color:var(--muted); font-variant-numeric:tabular-nums; }
.incoming-actions { display:flex; justify-content:flex-end; gap:8px; }
.incoming-actions button { display:inline-flex; align-items:center; gap:6px; min-height:34px; padding:6px 13px; border:1px solid var(--line); border-radius:9px; background:var(--bg); color:var(--text); font-size:13px; }
.incoming-actions button.primary { border-color:transparent; background:var(--accent); color:#fff; }
.incoming-actions button:disabled { opacity:.5; }
.add-device-dialog .incoming-heading { align-items:center; }
.add-device-dialog .incoming-actions { margin-top:20px; }
.field-label { display:block; margin:16px 0 7px; color:var(--muted); font-size:12px; }
.dialog-field { display:flex; align-items:center; gap:8px; padding:0 12px; border:1px solid var(--border); border-radius:9px; background:var(--bg); }
.dialog-field:focus-within { outline:2px solid var(--focus); outline-offset:1px; border-color:var(--selected-border); }
.field-icon { flex:none; color:var(--muted); }
.dialog-field input { flex:1; width:0; min-width:0; min-height:40px; padding:0; border:0; outline:0; background:transparent; color:var(--text); font:inherit; font-size:13px; }
.toggle-row { display:flex; align-items:center; gap:10px; margin:20px 0 6px; font-size:13px; cursor:pointer; }
.toggle-row > span:first-child { flex:1; }
.toggle-row input { position:absolute; width:1px; height:1px; opacity:0; }
.switch { position:relative; flex:none; width:38px; height:22px; border-radius:11px; background:var(--line); transition:background .15s ease; }
.switch::after { content:""; position:absolute; top:2px; left:2px; width:18px; height:18px; border-radius:50%; background:#fff; box-shadow:0 1px 2px rgba(16,24,40,.25); transition:transform .15s ease; }
.toggle-row input:checked ~ .switch { background:var(--accent); }
.toggle-row input:checked ~ .switch::after { transform:translateX(16px); }
.toggle-row input:focus-visible ~ .switch { outline:2px solid var(--focus); outline-offset:2px; }
.paired-head { display:flex; align-items:center; gap:7px; margin-top:20px; }
.paired-head .count { color:var(--muted); font-weight:400; font-variant-numeric:tabular-nums; }
.paired-manage { margin-top:8px; max-height:172px; overflow:auto; border:1px solid var(--line); border-radius:9px; }
.paired-item { display:flex; align-items:center; gap:10px; padding:8px 10px; border-bottom:1px solid var(--line); }
.paired-item:last-child { border-bottom:0; }
.paired-ico { display:grid; place-items:center; flex:none; width:30px; height:30px; border-radius:8px; background:var(--surface); color:var(--success); }
.paired-meta { display:flex; flex-direction:column; gap:1px; flex:1; min-width:0; }
.paired-meta strong { font-size:13px; font-weight:500; overflow:hidden; white-space:nowrap; text-overflow:ellipsis; }
.paired-meta small { color:var(--muted); font-size:11px; font-variant-numeric:tabular-nums; }
.paired-unpair { display:grid; place-items:center; flex:none; width:30px; height:30px; min-height:30px; padding:0; border:0; border-radius:8px; background:transparent; color:var(--muted); }
.paired-unpair:hover { background:var(--danger-bg); color:var(--danger); }
.paired-empty { margin:0; padding:16px; text-align:center; color:var(--muted); font-size:12px; }
.paired-dialog[open] { display:flex; flex-direction:column; height:min(420px,calc(100dvh - 64px)); overflow:hidden; }
.paired-dialog .incoming-heading { flex:none; }
.paired-dialog .paired-manage { flex:1; min-height:0; max-height:none; margin-top:16px; }
.paired-dialog .incoming-actions { flex:none; margin-top:16px; }
.spin { animation:lan-spin .9s linear infinite; }
@keyframes lan-spin { to { transform:rotate(360deg); } }
@media (prefers-reduced-motion:reduce) { .spin { animation:none; } .prog-bar i { transition:none; } }

@container lan-chat (max-width:540px) {
  .chat-shell { flex-direction:column; }
  .sidebar { flex:0 0 200px; width:100%; border-right:0; border-bottom:1px solid var(--line); }
  .sidebar-top { min-height:54px; padding:8px 12px; }
  .sidebar-scroll { padding:4px 8px; }
  .chat-head { min-height:54px; padding:8px 12px; }
  .timeline { padding:14px; }
  .row { max-width:90%; }
  .composer { padding:9px 12px; }
}
</style>
