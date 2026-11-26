<script setup lang="ts">
  import { onMounted, onBeforeUnmount, ref, computed, nextTick, watch, type Component } from "vue";
  import {
    Laptop,
    Smartphone,
    Server,
    Globe,
    RefreshCw,
    ArrowUp,
    Check,
    X,
    File as FileIcon,
    Radio,
    Link2,
    Unlink,
    Paperclip,
    FolderOpen,
    Copy,
    ClipboardPaste,
    Trash2,
    Pause,
    Play,
    MessageSquare,
    Plus,
    Settings,
    ShieldCheck,
    QrCode,
  } from "@lucide/vue";
  import { renderSVG } from "uqr";
  import { useApp } from "../../composables/useApp";
  import FileTypeIcon from "../../components/FileTypeIcon.vue";
  import BaseButton from "../../components/ui/BaseButton.vue";
  import { notifications } from "../../lib/notifications";
  import type { NoticeKind } from "../../lib/notification-center";
  import type { MessageKey } from "../../i18n";
  import type {
    LanLocalDevice,
    LanDiscoveredDevice,
    LanTransferTask,
    LanHistoryEntry,
    LanSettings,
    LanDeviceType,
    LanTransferStateName,
    LanTrustedDevice,
    LanPairRequest,
    LanIncomingRequest,
    LanProgress,
  } from "../../lib/lan-transfer";
  import { MAX_TEXT_BYTES } from "../../lib/lan-transfer";

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
  const qrDialog = ref<HTMLDialogElement>();
  const nameInput = ref("");
  const discoverableInput = ref(true);
  const acceptInput = ref(true);
  const autoAcceptInput = ref(true);
  const savingSettings = ref(false);
  const pairRequest = ref<LanPairRequest | null>(null);
  const pairQueue = ref<LanPairRequest[]>([]);
  const pairDialog = ref<HTMLDialogElement>();
  const responding = ref(false);
  const incoming = ref<LanIncomingRequest | null>(null);
  const incomingQueue = ref<LanIncomingRequest[]>([]);
  const incomingDialog = ref<HTMLDialogElement>();
  const timelineEl = ref<HTMLElement>();
  const menu = ref<{ x: number; y: number; item: ChatItem } | null>(null);
  const menuEl = ref<HTMLElement>();
  const menuReady = ref(false);
  let menuTrigger: HTMLElement | null = null;

  let unlisten: Array<() => void> = [];
  let disposed = false;

  const states: Record<LanTransferStateName, MessageKey> = {
    PENDING: "lan.statePending",
    WAITING_APPROVAL: "lan.stateWaiting",
    TRANSFERRING: "lan.stateTransferring",
    PAUSED: "lan.statePaused",
    COMPLETED: "lan.stateCompleted",
    FAILED: "lan.stateFailed",
    CANCELED: "lan.stateCanceled",
  };
  const stateLabel = (s: LanTransferStateName) => (Object.hasOwn(states, s) ? t(states[s]) : s);

  // Terminal transfer states: once reached, remove from the active list and clear the progress cache.
  const TERMINAL_STATES = new Set<LanTransferStateName>(["COMPLETED", "FAILED", "CANCELED"]);
  const isTerminal = (s: LanTransferStateName) => TERMINAL_STATES.has(s);

  const DEVICE_ICONS: Partial<Record<LanDeviceType, Component>> = {
    mobile: Smartphone,
    server: Server,
    headless: Server,
    web: Globe,
  };
  function deviceIcon(type: LanDeviceType | null): Component {
    return (type && DEVICE_ICONS[type]) || Laptop;
  }
  function fmtSize(bytes: number): string {
    const units = ["B", "KB", "MB", "GB", "TB"];
    let value = Math.max(0, bytes),
      i = 0;
    while (value >= 1024 && i < units.length - 1) {
      value /= 1024;
      i++;
    }
    return `${i && value < 10 ? value.toFixed(1) : value.toFixed(0)} ${units[i]}`;
  }
  // WeChat-style friendly time: today shows only hour:minute, yesterday prefixes "yesterday", within a week uses the weekday, older adds month/day.
  function friendlyTime(sec: number | null): string {
    if (!sec) return "";
    const d = new Date(sec * 1000),
      now = new Date(),
      loc = locale.value;
    const hm = new Intl.DateTimeFormat(loc, {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    }).format(d);
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
  function showError(error: unknown) {
    showNotice(tx(error instanceof Error ? error.message : String(error)), "error");
  }

  // Paired devices (driven by the trusted list, overlaid with online/address info).
  const pairedList = computed(() =>
    trusted.value.map(tr => {
      const dev = devices.value.find(d => d.fingerprint === tr.fingerprint);
      return {
        fingerprint: tr.fingerprint,
        alias: dev?.alias || tr.alias,
        deviceType: dev?.deviceType ?? null,
        online: !!dev,
        address: dev?.address,
      };
    })
  );
  // Discovered but not-yet-paired devices.
  const discoveredUnpaired = computed(() =>
    devices.value.filter(d => !d.trusted && !trusted.value.some(tr => tr.fingerprint === d.fingerprint))
  );
  const selectedPeer = computed(() => {
    if (!selected.value) return null;
    const dev = devices.value.find(d => d.fingerprint === selected.value);
    const tr = trusted.value.find(x => x.fingerprint === selected.value);
    if (!dev && !tr) return null;
    return {
      fingerprint: selected.value,
      alias: dev?.alias || tr?.alias || selected.value,
      deviceType: dev?.deviceType ?? null,
      online: !!dev,
      address: dev?.address,
    };
  });
  const timeline = computed<ChatItem[]>(() => {
    const live: ChatItem[] = liveTasks.value
      .filter(tk => tk.peerFingerprint === selected.value)
      .map(tk => ({
        id: tk.id,
        direction: tk.direction,
        peerAlias: tk.peerAlias,
        peerFingerprint: tk.peerFingerprint,
        fileName: tk.fileName,
        size: tk.size,
        state: tk.state,
        finishedAt: null,
        savePath: tk.savePath ?? null,
        text: null,
        live: true,
      }));
    return [...messages.value, ...live];
  });
  function transferProgress(item: ChatItem) {
    const sample = progress.value[item.id];
    const completed = item.state === "COMPLETED";
    const total = Math.max(0, sample?.bytesTotal ?? item.size);
    const done = completed ? total : Math.min(total, Math.max(0, sample?.bytesDone ?? 0));
    const percent = total ? Math.min(100, Math.floor((done / total) * 100)) : completed ? 100 : 0;
    return { total, done, percent };
  }
  // WeChat-style time divider: show a centered time above the first timestamped message, or when the gap from the previous one is >= 5 minutes.
  function showTimeDivider(index: number): boolean {
    const cur = timeline.value[index];
    if (!cur?.finishedAt) return false;
    const prev = timeline.value[index - 1];
    if (!prev?.finishedAt) return true;
    return cur.finishedAt - prev.finishedAt >= 300;
  }
  const canSend = computed(() => !!selectedPeer.value?.online && ready.value);
  const draftValid = computed(
    () => !!draft.value.trim() && new TextEncoder().encode(draft.value).length <= MAX_TEXT_BYTES
  );

  async function keepListener(listener?: Promise<() => void>) {
    if (!listener) return;
    const stop = await listener;
    if (disposed) stop();
    else unlisten.push(stop);
  }
  async function scrollToEnd() {
    await nextTick();
    if (timelineEl.value) timelineEl.value.scrollTop = timelineEl.value.scrollHeight;
  }

  async function reloadTrusted() {
    try {
      trusted.value = await (await bridge()).lanListTrusted();
    } catch (e) {
      showError(e);
    }
  }
  async function reloadConversation() {
    if (!selected.value) {
      messages.value = [];
      return;
    }
    try {
      const fp = selected.value;
      const rows = await (await bridge()).lanGetConversation(fp);
      if (!disposed && selected.value === fp) {
        messages.value = rows;
        void scrollToEnd();
      }
    } catch (e) {
      showError(e);
    }
  }

  async function load() {
    busy.value = true;
    try {
      const b = await bridge();
      await b.lanStart();
      if (disposed) return;
      const [device, config, tasks] = await Promise.all([
        b.lanGetLocalDevice(),
        b.lanGetSettings(),
        b.lanListTransfers(),
      ]);
      if (disposed) return;
      localDevice.value = device;
      settings.value = config;
      liveTasks.value = tasks.filter(x => !isTerminal(x.state));
      await reloadTrusted();
      unlisten.forEach(stop => stop());
      unlisten = [];
      await keepListener(
        b.onLanDevices?.(d => {
          if (!disposed) devices.value = d;
        })
      );
      await keepListener(
        b.onLanPair?.(req => {
          if (disposed) return;
          // Don't enqueue the same request twice (showing or already queued); if one is being handled, queue it to avoid overwriting/losing it.
          if (
            pairRequest.value?.requestId === req.requestId ||
            pairQueue.value.some(p => p.requestId === req.requestId)
          )
            return;
          if (pairRequest.value) pairQueue.value.push(req);
          else pairRequest.value = req;
        })
      );
      await keepListener(
        b.onLanIncoming?.(r => {
          if (disposed) return;
          // Don't enqueue the same request twice; if one is awaiting confirmation, queue it so concurrent incoming requests aren't silently dropped.
          if (incoming.value?.taskId === r.taskId || incomingQueue.value.some(x => x.taskId === r.taskId)) return;
          if (incoming.value) incomingQueue.value.push(r);
          else incoming.value = r;
        })
      );
      await keepListener(
        b.onLanTransfer?.(task => {
          if (disposed) return;
          if (isTerminal(task.state)) {
            liveTasks.value = liveTasks.value.filter(x => x.id !== task.id);
            // Always clear the progress cache on terminal states, so failed/canceled entries don't linger.
            const { [task.id]: _drop, ...rest } = progress.value;
            progress.value = rest;
            if (task.peerFingerprint === selected.value) void reloadConversation();
          } else {
            const i = liveTasks.value.findIndex(x => x.id === task.id);
            if (i >= 0) liveTasks.value[i] = task;
            else {
              liveTasks.value.push(task);
              if (task.peerFingerprint === selected.value) void scrollToEnd();
            }
          }
        })
      );
      await keepListener(
        b.onLanProgress?.(p => {
          if (!disposed) progress.value = { ...progress.value, [p.taskId]: p };
        })
      );
      await keepListener(
        b.onLanText?.(row => {
          if (disposed) return;
          if (row.peerFingerprint === selected.value) void reloadConversation();
          else showNotice(t("lan.textReceived"), "info");
        })
      );
      if (disposed) return;
      await b.lanRefreshDevices();
      if (!disposed) ready.value = true;
    } catch (error) {
      showError(error);
    } finally {
      if (!disposed) busy.value = false;
    }
  }

  async function refresh() {
    if (busy.value) return;
    if (!ready.value) return load();
    try {
      await (await bridge()).lanRefreshDevices();
      await reloadTrusted();
    } catch (error) {
      showError(error);
    }
  }

  function selectDevice(fp: string) {
    if (selected.value === fp) return;
    selected.value = fp;
    messages.value = [];
    closeMenu();
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
    } catch (error) {
      showError(error);
    } finally {
      if (!disposed) pairingFp.value = "";
    }
  }

  async function respondPair(accept: boolean) {
    const req = pairRequest.value;
    if (!req || responding.value) return;
    responding.value = true;
    try {
      await (await bridge()).lanRespondPair(req.requestId, accept);
      if (disposed) return;
      pairRequest.value = pairQueue.value.shift() ?? null;
      if (accept) {
        await reloadTrusted();
        showNotice(t("lan.paired"));
      }
    } catch (error) {
      showError(error);
    } finally {
      if (!disposed) responding.value = false;
    }
  }

  async function unpairDevice(fp: string) {
    if (!fp || !window.confirm(t("lan.unpair") + "?")) return;
    try {
      await (await bridge()).lanUnpair(fp);
      if (disposed) return;
      await reloadTrusted();
      if (selected.value === fp) {
        selected.value = "";
        messages.value = [];
      }
      showNotice(t("lan.unpaired"));
    } catch (error) {
      showError(error);
    }
  }
  function unpairSelected() {
    void unpairDevice(selected.value);
  }

  function openAdd() {
    manualAddr.value = "";
    addDialog.value?.showModal();
  }
  function openPaired() {
    void reloadTrusted();
    pairedDialog.value?.showModal();
  }

  // QR direct connect: this device's QR payload (clashnova://lan?…). Scanning it on mobile allows a unicast direct connection,
  // with no multicast required (works even when iOS hasn't granted multicast permission). alias uses encodeURIComponent so the
  // mobile side's Uri.queryParameters decodes it correctly.
  const qrPayload = computed(() => {
    const d = localDevice.value;
    if (!d || !d.ip || !d.fingerprint) return "";
    return `clashnova://lan?v=1&ip=${d.ip}&port=${d.port}&fp=${d.fingerprint}&alias=${encodeURIComponent(d.alias)}`;
  });
  const qrSvg = computed(() => (qrPayload.value ? renderSVG(qrPayload.value, { border: 2 }) : ""));
  function openQr() {
    qrDialog.value?.showModal();
  }
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
    try {
      await (await bridge()).lanAddManualDevice(addr);
      manualAddr.value = "";
      addDialog.value?.close();
      showNotice(t("lan.added"));
    } catch (error) {
      showError(error);
    } finally {
      if (!disposed) adding.value = false;
    }
  }
  async function saveSettings() {
    if (!settings.value || savingSettings.value || !nameInput.value.trim()) return;
    savingSettings.value = true;
    try {
      const updated = await (
        await bridge()
      ).lanUpdateSettings({
        ...settings.value,
        alias: nameInput.value.trim(),
        discoverable: discoverableInput.value,
        acceptIncoming: acceptInput.value,
        autoAcceptTrusted: autoAcceptInput.value,
      });
      if (disposed) return;
      settings.value = updated;
      settingsDialog.value?.close();
      showNotice(t("lan.saved"));
    } catch (error) {
      showError(error);
    } finally {
      if (!disposed) savingSettings.value = false;
    }
  }

  function onComposerEnter(event: KeyboardEvent) {
    if (event.isComposing || event.keyCode === 229) return;
    event.preventDefault();
    void sendText();
  }
  async function sendText() {
    if (!canSend.value || !draftValid.value || sending.value) return;
    sending.value = true;
    const body = draft.value,
      fingerprint = selected.value;
    try {
      await (await bridge()).lanSendText(fingerprint, body);
      if (disposed) return;
      if (selected.value === fingerprint && draft.value === body) draft.value = "";
      await reloadConversation();
    } catch (error) {
      showError(error);
    } finally {
      if (!disposed) sending.value = false;
    }
  }

  async function pickAndSend() {
    if (!canSend.value) return;
    try {
      const b = await bridge();
      const paths = await b.lanPickFiles();
      if (disposed || !paths.length) return;
      await b.lanSendFiles(selected.value, paths);
    } catch (error) {
      showError(error);
    }
  }

  async function pasteFiles() {
    closeMenu();
    if (!canSend.value) return;
    try {
      const files = await (await bridge()).lanClipboardFiles();
      if (disposed) return;
      if (!files.length) {
        showNotice(t("lan.noClipFiles"), "info");
        return;
      }
      await (await bridge()).lanSendFiles(selected.value, files);
    } catch (error) {
      showError(error);
    }
  }
  async function onComposerPaste(event: ClipboardEvent) {
    if (event.clipboardData?.files.length) {
      event.preventDefault();
      await pasteFiles();
    }
  }

  // —— Bubble context menu ——
  async function openMenu(event: MouseEvent | KeyboardEvent, item: ChatItem) {
    event.preventDefault();
    closeMenu();
    if (item.live) return;
    const trigger = event.currentTarget as HTMLElement;
    menuTrigger = trigger.querySelector<HTMLButtonElement>("button:not(:disabled)") ?? trigger;
    const bounds = trigger.getBoundingClientRect();
    menuReady.value = false;
    menu.value = {
      x: event instanceof MouseEvent ? event.clientX : bounds.left,
      y: event instanceof MouseEvent ? event.clientY : bounds.bottom,
      item,
    };
    const opened = menu.value;
    await nextTick();
    if (menu.value !== opened || !menuEl.value) return;
    const rect = menuEl.value.getBoundingClientRect(),
      gap = 8;
    opened.x = Math.max(gap, Math.min(opened.x, window.innerWidth - rect.width - gap));
    opened.y = Math.max(gap, Math.min(opened.y, window.innerHeight - rect.height - gap));
    menuReady.value = true;
    await nextTick();
    if (menu.value === opened)
      menuEl.value?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus({ preventScroll: true });
  }
  function closeMenu() {
    menu.value = null;
  }
  function onBubbleKeydown(event: KeyboardEvent, item: ChatItem) {
    if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) void openMenu(event, item);
  }
  function onMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu();
      menuTrigger?.focus({ preventScroll: true });
      return;
    }
    if (event.key === "Tab") {
      closeMenu();
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const items = Array.from(menuEl.value?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []);
    if (!items.length) return;
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? items.length - 1
          : (index + (event.key === "ArrowUp" ? -1 : 1) + items.length) % items.length;
    items[next]?.focus();
  }
  async function openFile(item: ChatItem) {
    closeMenu();
    if (item.savePath) {
      try {
        await (await bridge()).lanOpenPath(item.savePath);
      } catch (e) {
        showError(e);
      }
    }
  }
  async function revealFile(item: ChatItem) {
    closeMenu();
    if (item.savePath) {
      try {
        await (await bridge()).lanRevealPath(item.savePath);
      } catch (e) {
        showError(e);
      }
    }
  }
  async function copyFile(item: ChatItem) {
    closeMenu();
    if (item.savePath) {
      try {
        await (await bridge()).lanCopyFileToClipboard(item.savePath);
        showNotice(t("lan.fileCopied"));
      } catch (e) {
        showError(e);
      }
    }
  }
  async function copyTextItem(item: ChatItem) {
    closeMenu();
    if (item.text == null) return;
    try {
      await navigator.clipboard.writeText(item.text);
      showNotice(t("lan.textCopied"));
    } catch (e) {
      showError(e);
    }
  }
  async function deleteItem(item: ChatItem) {
    closeMenu();
    if (!window.confirm(t("lan.deleteConfirm"))) return;
    try {
      await (await bridge()).lanDeleteFile(item.id, item.savePath ?? null);
      if (disposed) return;
      messages.value = messages.value.filter(m => !(m.id === item.id && m.direction === item.direction));
      showNotice(t("lan.deleted"));
    } catch (e) {
      showError(e);
    }
  }

  async function cancelTask(id: string) {
    try {
      await (await bridge()).lanCancelTransfer(id);
    } catch (e) {
      showError(e);
    }
  }
  async function pauseTask(id: string) {
    try {
      await (await bridge()).lanPauseTransfer(id);
    } catch (e) {
      showError(e);
    }
  }
  async function resumeTask(id: string) {
    try {
      await (await bridge()).lanResumeTransfer(id);
    } catch (e) {
      showError(e);
    }
  }

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
    try {
      await (await bridge()).lanRespondIncoming(req.taskId, accept, null);
      if (!disposed) incoming.value = incomingQueue.value.shift() ?? null;
    } catch (error) {
      showError(error);
    } finally {
      if (!disposed) responding.value = false;
    }
  }

  onMounted(() => {
    window.addEventListener("click", closeMenu);
    window.addEventListener("resize", closeMenu);
    window.addEventListener("blur", closeMenu);
    void load();
  });
  onBeforeUnmount(() => {
    disposed = true;
    unlisten.forEach(fn => fn());
    unlisten = [];
    window.removeEventListener("click", closeMenu);
    window.removeEventListener("resize", closeMenu);
    window.removeEventListener("blur", closeMenu);
    pairDialog.value?.close();
    incomingDialog.value?.close();
    addDialog.value?.close();
    settingsDialog.value?.close();
    pairedDialog.value?.close();
    qrDialog.value?.close();
  });
</script>

<template>
  <section class="lan" :aria-label="t('nav.lanTransfer')">
    <div class="chat-shell">
      <!-- Left column: paired + pairable devices -->
      <aside class="sidebar">
        <header class="sidebar-top">
          <div class="rail-heading">
            <strong class="rail-title">{{ t("lan.devices") }}</strong>
            <span
              v-if="localDevice && settings"
              class="local-state"
              role="img"
              :title="`${localDevice.alias} · ${t(settings.discoverable ? 'lan.visible' : 'lan.hidden')}`"
              :aria-label="`${localDevice.alias} · ${t(settings.discoverable ? 'lan.visible' : 'lan.hidden')}`"
              ><span class="dot" :class="{ on: settings.discoverable }"
            /></span>
          </div>
          <div class="rail-tools">
            <BaseButton
              class="icon-btn"
              :disabled="busy"
              :title="t('lan.refresh')"
              :aria-label="t('lan.refresh')"
              @click="refresh"
            >
              <RefreshCw :size="16" :class="{ spin: busy }" />
            </BaseButton>
            <BaseButton
              class="icon-btn add-btn"
              :title="t('lan.addDevice')"
              :aria-label="t('lan.addDevice')"
              @click="openAdd"
            >
              <Plus :size="17" />
            </BaseButton>
            <BaseButton class="icon-btn" :title="t('lan.myQr')" :aria-label="t('lan.myQr')" @click="openQr">
              <QrCode :size="16" />
            </BaseButton>
            <BaseButton
              class="icon-btn"
              :title="t('lan.pairedSection')"
              :aria-label="t('lan.pairedSection')"
              @click="openPaired"
            >
              <ShieldCheck :size="16" />
            </BaseButton>
            <BaseButton
              class="icon-btn"
              :title="t('lan.settingsTitle')"
              :aria-label="t('lan.settingsTitle')"
              @click="openSettings"
            >
              <Settings :size="16" />
            </BaseButton>
          </div>
        </header>

        <div class="sidebar-scroll">
          <template v-if="pairedList.length">
            <p class="group-label">
              {{ t("lan.pairedSection") }}<span class="count">{{ pairedList.length }}</span>
            </p>
            <BaseButton
              v-for="d in pairedList"
              :key="d.fingerprint"
              class="conv"
              :aria-pressed="selected === d.fingerprint"
              :class="{ active: selected === d.fingerprint, offline: !d.online }"
              @click="selectDevice(d.fingerprint)"
            >
              <span class="conv-meta"
                ><strong :title="d.alias">{{ d.alias }}</strong
                ><small :title="d.address"
                  ><span class="dot" :class="{ on: d.online }" /><span>{{
                    t(d.online ? "lan.peerOnline" : "lan.peerOffline")
                  }}</span
                  ><span v-if="d.online && d.address" class="peer-address"> · {{ d.address }}</span></small
                ></span
              >
            </BaseButton>
          </template>

          <template v-if="discoveredUnpaired.length">
            <p class="group-label">
              {{ t("lan.discoveredSection") }}<span class="count">{{ discoveredUnpaired.length }}</span>
            </p>
            <div v-for="d in discoveredUnpaired" :key="d.fingerprint" class="conv discover">
              <span class="conv-meta"
                ><strong :title="d.alias">{{ d.alias }}</strong
                ><small :title="d.address">{{ d.address }}</small></span
              >
              <BaseButton
                class="pair-btn"
                :aria-label="`${t('lan.pair')} · ${d.alias}`"
                :disabled="!!pairingFp || !ready"
                :aria-busy="pairingFp === d.fingerprint"
                @click="pairDevice(d.fingerprint)"
              >
                <Link2 :size="13" />{{ t(pairingFp === d.fingerprint ? "lan.pairing" : "lan.pair") }}
              </BaseButton>
            </div>
          </template>

          <div v-if="!pairedList.length && !discoveredUnpaired.length" class="sidebar-empty" role="status">
            <Radio :size="26" :stroke-width="1.4" />
            <strong>{{ t(busy ? "lan.searching" : "lan.noDevices") }}</strong>
            <p>{{ t("lan.deviceHint") }}</p>
          </div>
        </div>
      </aside>

      <!-- Right column: conversation -->
      <div class="chat">
        <template v-if="selectedPeer">
          <header class="chat-head">
            <span class="chat-head-icon"><component :is="deviceIcon(selectedPeer.deviceType)" :size="18" /></span>
            <div class="chat-head-meta">
              <strong :title="selectedPeer.alias">{{ selectedPeer.alias }}</strong>
              <small :title="selectedPeer.address"
                ><span class="dot" :class="{ on: selectedPeer.online }" />{{
                  t(selectedPeer.online ? "lan.peerOnline" : "lan.peerOffline")
                }}<span v-if="selectedPeer.address" class="peer-address"> · {{ selectedPeer.address }}</span></small
              >
            </div>
            <BaseButton
              class="icon-btn danger"
              :title="t('lan.unpair')"
              :aria-label="t('lan.unpair')"
              @click="unpairSelected"
            >
              <Unlink :size="16" />
            </BaseButton>
          </header>

          <div ref="timelineEl" class="timeline" @scroll="closeMenu">
            <div v-if="!timeline.length" class="timeline-empty">
              <MessageSquare :size="26" :stroke-width="1.4" /><span>{{ t("lan.emptyChat") }}</span>
            </div>
            <template v-for="(item, index) in timeline" :key="`${item.id}-${item.direction}`">
              <div v-if="showTimeDivider(index)" class="time-divider">
                <span>{{ friendlyTime(item.finishedAt) }}</span>
              </div>
              <div class="row" :class="item.direction === 'send' ? 'me' : 'peer'">
                <div
                  class="bubble"
                  :class="{ file: item.text == null }"
                  :tabindex="item.text != null || !item.savePath ? 0 : undefined"
                  @contextmenu="openMenu($event, item)"
                  @keydown="onBubbleKeydown($event, item)"
                >
                  <!-- Text bubble -->
                  <p v-if="item.text != null" class="bubble-text">{{ item.text }}</p>
                  <!-- File bubble -->
                  <template v-else>
                    <BaseButton
                      class="file-row"
                      :disabled="!item.savePath || item.live"
                      :aria-label="`${t('lan.openFile')} · ${item.fileName}`"
                      @click="openFile(item)"
                    >
                      <span class="file-ico"><FileTypeIcon :name="item.fileName" /></span>
                      <span class="file-info"
                        ><strong :title="item.fileName">{{ item.fileName }}</strong
                        ><span class="file-meta"
                          ><small>{{ fmtSize(item.size) }}</small
                          ><small v-if="!item.live" class="file-state" :data-state="item.state"
                            ><Check v-if="item.state === 'COMPLETED'" :size="12" />{{ stateLabel(item.state) }}</small
                          ></span
                        ></span
                      >
                    </BaseButton>
                    <div
                      v-if="item.live"
                      class="prog"
                      :class="{ failed: item.state === 'FAILED' || item.state === 'CANCELED' }"
                    >
                      <div
                        class="prog-bar"
                        role="progressbar"
                        :aria-label="`${item.fileName} · ${stateLabel(item.state)}`"
                        :aria-valuenow="transferProgress(item).percent"
                        aria-valuemin="0"
                        aria-valuemax="100"
                      >
                        <i :style="{ transform: `scaleX(${transferProgress(item).percent / 100})` }" />
                      </div>
                      <div class="progress-metrics">
                        <span
                          >{{ fmtSize(transferProgress(item).done) }} /
                          {{ fmtSize(transferProgress(item).total) }}</span
                        ><span>{{ transferProgress(item).percent }}%</span>
                      </div>
                      <div class="prog-act">
                        <span>{{ stateLabel(item.state) }}</span>
                        <BaseButton
                          v-if="item.state === 'TRANSFERRING'"
                          :title="t('lan.pause')"
                          :aria-label="t('lan.pause')"
                          @click="pauseTask(item.id)"
                        >
                          <Pause :size="13" />
                        </BaseButton>
                        <BaseButton
                          v-if="item.state === 'PAUSED'"
                          :title="t('lan.resume')"
                          :aria-label="t('lan.resume')"
                          @click="resumeTask(item.id)"
                        >
                          <Play :size="13" />
                        </BaseButton>
                        <BaseButton :title="t('lan.cancel')" :aria-label="t('lan.cancel')" @click="cancelTask(item.id)">
                          <X :size="13" />
                        </BaseButton>
                      </div>
                    </div>
                  </template>
                </div>
              </div>
            </template>
          </div>

          <footer class="composer">
            <BaseButton
              class="icon-btn"
              :disabled="!canSend"
              :title="t('lan.send')"
              :aria-label="t('lan.send')"
              @click="pickAndSend"
            >
              <Paperclip :size="18" />
            </BaseButton>
            <div class="composer-input">
              <textarea
                v-model="draft"
                :aria-label="t('lan.msgPlaceholder')"
                :placeholder="canSend ? t('lan.msgPlaceholder') : t('lan.pairToStart')"
                :disabled="!canSend"
                rows="1"
                :maxlength="MAX_TEXT_BYTES"
                @paste="onComposerPaste"
                @keydown.enter.exact="onComposerEnter"
              />
              <BaseButton
                class="send-btn"
                :disabled="!canSend || !draftValid || sending"
                :title="t('lan.sendMessage')"
                :aria-label="t('lan.sendMessage')"
                @click="sendText"
              >
                <ArrowUp :size="16" />
              </BaseButton>
            </div>
          </footer>
        </template>

        <div v-else class="chat-placeholder">
          <span class="placeholder-mark"><MessageSquare :size="27" :stroke-width="1.5" /></span>
          <strong>{{ t("lan.pickChat") }}</strong>
          <p>{{ t("lan.pairToStart") }}</p>
        </div>
      </div>
    </div>

    <!-- Bubble context menu -->
    <Teleport to="body">
      <div
        v-if="menu"
        ref="menuEl"
        class="ctx-menu"
        role="menu"
        :style="{
          left: menu.x + 'px',
          top: menu.y + 'px',
          visibility: menuReady ? 'visible' : 'hidden',
        }"
        @click.stop
        @contextmenu.prevent
        @keydown="onMenuKeydown"
      >
        <template v-if="menu.item.text != null">
          <BaseButton role="menuitem" @click="copyTextItem(menu.item)">
            <Copy :size="15" />{{ t("lan.copy") }}
          </BaseButton>
          <BaseButton role="menuitem" @click="pasteFiles">
            <ClipboardPaste :size="15" />{{ t("lan.paste") }}
          </BaseButton>
          <BaseButton role="menuitem" class="danger" @click="deleteItem(menu.item)">
            <Trash2 :size="15" />{{ t("lan.delete") }}
          </BaseButton>
        </template>
        <template v-else>
          <BaseButton
            role="menuitem"
            :disabled="!menu.item.savePath"
            :title="!menu.item.savePath ? t('native.lanFileUnavailable') : undefined"
            @click="copyFile(menu.item)"
          >
            <Copy :size="15" />{{ t("lan.copy") }}
          </BaseButton>
          <BaseButton role="menuitem" class="danger" @click="deleteItem(menu.item)">
            <Trash2 :size="15" />{{ t("lan.delete") }}
          </BaseButton>
          <div class="ctx-separator" role="separator" />
          <BaseButton
            role="menuitem"
            :disabled="!menu.item.savePath"
            :title="!menu.item.savePath ? t('native.lanFileUnavailable') : undefined"
            @click="openFile(menu.item)"
          >
            <FileIcon :size="15" />{{ t("lan.openFile") }}
          </BaseButton>
          <BaseButton
            role="menuitem"
            :disabled="!menu.item.savePath"
            :title="!menu.item.savePath ? t('native.lanFileUnavailable') : undefined"
            @click="revealFile(menu.item)"
          >
            <FolderOpen :size="15" />{{ t("lan.openFolder") }}
          </BaseButton>
        </template>
      </div>
    </Teleport>

    <!-- Add device -->
    <dialog
      ref="addDialog"
      class="incoming-dialog add-device-dialog"
      aria-labelledby="lan-add-title"
      @close="manualAddr = ''"
    >
      <form @submit.prevent="addManual">
        <header class="incoming-heading">
          <span class="incoming-icon"><Plus :size="22" /></span>
          <div>
            <h2 id="lan-add-title">{{ t("lan.addDevice") }}</h2>
          </div>
        </header>
        <label class="field-label" for="lan-add-input">{{ t("lan.manualAdd") }}</label>
        <div class="dialog-field">
          <Link2 class="field-icon" :size="15" /><input
            id="lan-add-input"
            v-model="manualAddr"
            type="text"
            inputmode="url"
            :placeholder="t('lan.manualPlaceholder')"
            :disabled="adding"
          />
        </div>
        <div class="incoming-actions">
          <BaseButton :disabled="adding" @click="addDialog?.close()"> {{ t("common.cancel") }}</BaseButton><BaseButton
            type="submit"
            class="primary"
            :disabled="adding || !manualAddr.trim()"
          >
            <Plus :size="15" />{{ t("lan.addDevice") }}
          </BaseButton>
        </div>
      </form>
    </dialog>

    <!-- My QR code: for mobile to scan and connect directly -->
    <dialog ref="qrDialog" class="incoming-dialog qr-dialog" aria-labelledby="lan-qr-title">
      <header class="incoming-heading">
        <span class="incoming-icon"><QrCode :size="22" /></span>
        <div>
          <h2 id="lan-qr-title">{{ t("lan.myQr") }}</h2>
        </div>
      </header>
      <div v-if="qrPayload" class="qr-box">
        <!-- eslint-disable-next-line vue/no-v-html -->
        <div class="qr-svg" v-html="qrSvg" />
        <div class="qr-meta">
          <strong>{{ localDevice?.alias }}</strong><span>{{ localDevice?.ip }}:{{ localDevice?.port }}</span>
        </div>
        <p class="qr-hint">{{ t("lan.myQrHint") }}</p>
      </div>
      <p v-else class="qr-hint qr-noip">{{ t("lan.qrNoIp") }}</p>
      <div class="incoming-actions">
        <BaseButton class="primary" @click="qrDialog?.close()">
          {{ t("common.done") }}
        </BaseButton>
      </div>
    </dialog>

    <!-- Settings -->
    <dialog ref="settingsDialog" class="incoming-dialog" aria-labelledby="lan-settings-title">
      <form @submit.prevent="saveSettings">
        <header class="incoming-heading">
          <span class="incoming-icon"><Settings :size="22" /></span>
          <div>
            <h2 id="lan-settings-title">{{ t("lan.settingsTitle") }}</h2>
          </div>
        </header>
        <label class="field-label" for="lan-name-input">{{ t("lan.deviceName") }}</label>
        <div class="dialog-field">
          <input
            id="lan-name-input"
            v-model="nameInput"
            type="text"
            maxlength="64"
            :placeholder="t('lan.deviceName')"
            :disabled="savingSettings"
          />
        </div>
        <label class="toggle-row"
          ><span>{{ t("lan.visible") }}</span
          ><input v-model="discoverableInput" type="checkbox" role="switch" :disabled="savingSettings" /><span
            class="switch"
            aria-hidden="true"
        /></label>
        <label class="toggle-row"
          ><span>{{ t("lan.acceptIncoming") }}</span
          ><input v-model="acceptInput" type="checkbox" role="switch" :disabled="savingSettings" /><span
            class="switch"
            aria-hidden="true"
        /></label>
        <label class="toggle-row"
          ><span>{{ t("lan.autoAccept") }}</span
          ><input
            v-model="autoAcceptInput"
            type="checkbox"
            role="switch"
            :disabled="savingSettings || !acceptInput" /><span class="switch" aria-hidden="true"
        /></label>
        <div class="incoming-actions">
          <BaseButton :disabled="savingSettings" @click="settingsDialog?.close()">
            {{ t("common.cancel") }}</BaseButton><BaseButton
            type="submit"
            class="primary"
            :disabled="savingSettings || !nameInput.trim()"
          >
            <Check :size="15" />{{ t("lan.save") }}
          </BaseButton>
        </div>
      </form>
    </dialog>

    <!-- Paired device management -->
    <dialog ref="pairedDialog" class="incoming-dialog paired-dialog" aria-labelledby="lan-paired-title">
      <header class="incoming-heading">
        <span class="incoming-icon"><ShieldCheck :size="22" /></span>
        <div>
          <h2 id="lan-paired-title">{{ t("lan.pairedSection") }} · {{ trusted.length }}</h2>
        </div>
      </header>
      <div class="paired-manage">
        <div v-for="d in trusted" :key="d.fingerprint" class="paired-item">
          <span class="paired-ico"><ShieldCheck :size="16" /></span>
          <span class="paired-meta"
            ><strong :title="d.alias">{{ d.alias }}</strong
            ><small :title="d.fingerprint">{{ t("lan.trusted") }} · {{ d.fingerprint.slice(0, 12) }}…</small></span
          >
          <BaseButton
            class="paired-unpair"
            :title="t('lan.unpair')"
            :aria-label="`${t('lan.unpair')} · ${d.alias}`"
            @click="unpairDevice(d.fingerprint)"
          >
            <Unlink :size="14" />
          </BaseButton>
        </div>
        <p v-if="!trusted.length" class="paired-empty">{{ t("lan.noPaired") }}</p>
      </div>
      <div class="incoming-actions">
        <BaseButton @click="pairedDialog?.close()">{{ t("common.close") }}</BaseButton>
      </div>
    </dialog>

    <!-- Pair request -->
    <dialog
      ref="pairDialog"
      class="incoming-dialog"
      aria-labelledby="lan-pair-title"
      @cancel.prevent="respondPair(false)"
    >
      <template v-if="pairRequest">
        <header class="incoming-heading">
          <span class="incoming-icon"><Link2 :size="22" /></span>
          <div>
            <h2 id="lan-pair-title">{{ t("lan.pairRequestTitle") }}</h2>
            <p>
              {{ t("lan.from") }} <b>{{ pairRequest.peerAlias }}</b>
            </p>
          </div>
        </header>
        <p class="pair-desc">{{ t("lan.pairDesc") }}</p>
        <div class="incoming-actions">
          <BaseButton :disabled="responding" @click="respondPair(false)">
            <X :size="15" />{{ t("lan.reject") }}</BaseButton
          ><BaseButton class="primary" :disabled="responding" @click="respondPair(true)">
            <Check :size="15" />{{ t("lan.accept") }}
          </BaseButton>
        </div>
      </template>
    </dialog>

    <!-- Unpaired (third-party LocalSend) fallback: confirm one at a time -->
    <dialog
      ref="incomingDialog"
      class="incoming-dialog"
      aria-labelledby="lan-incoming-title"
      @cancel.prevent="respondIncoming(false)"
    >
      <template v-if="incoming">
        <header class="incoming-heading">
          <span class="incoming-icon"><Radio :size="22" /></span>
          <div>
            <h2 id="lan-incoming-title">
              {{ t(incoming.text != null ? "lan.incomingTextTitle" : "lan.incomingTitle") }}
            </h2>
            <p>
              {{ t("lan.from") }} <b>{{ incoming.peerAlias }}</b> ·
              {{ fmtSize(incoming.totalSize) }}
            </p>
          </div>
        </header>
        <pre v-if="incoming.text != null" class="message-text">{{ incoming.text }}</pre>
        <ul v-else class="incoming-files">
          <li v-for="(f, i) in incoming.files" :key="i">
            <FileTypeIcon :name="f.name" :size="18" /><span :title="f.name">{{ f.name }}</span
            ><small>{{ fmtSize(f.size) }}</small>
          </li>
        </ul>
        <div class="incoming-actions">
          <BaseButton :disabled="responding" @click="respondIncoming(false)">
            <X :size="15" />{{ t("lan.reject") }}</BaseButton
          ><BaseButton class="primary" :disabled="responding" @click="respondIncoming(true)">
            <Check :size="15" />{{ t("lan.accept") }}
          </BaseButton>
        </div>
      </template>
    </dialog>
  </section>
</template>
<style scoped lang="scss" src="./index.scss"></style>
