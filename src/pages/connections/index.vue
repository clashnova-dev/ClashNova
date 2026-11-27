<template>
  <section class="pnm-page">
    <div class="pnm-overview">
      <div class="pnm-topbar">
        <div class="pnm-segment" :aria-label="t('connections.processTitle')">
          <BaseButton :aria-pressed="view === 'apps'" :class="{ on: view === 'apps' }" @click="changeView('apps')">
            <AppWindow :size="16" />{{ t("connections.apps") }}
          </BaseButton>
          <BaseButton
            :aria-pressed="view === 'connections'"
            :class="{ on: view === 'connections' }"
            @click="changeView('connections')"
          >
            <Network :size="16" />{{ t("connections.count") }}
          </BaseButton>
        </div>
        <dl class="pnm-stats">
          <div>
            <dt>{{ t("connections.statApps") }}</dt>
            <dd>{{ groups.filter(group => group.rows.length).length }}</dd>
          </div>
          <div>
            <dt>{{ t("connections.statConns") }}</dt>
            <dd>{{ totalConns }}</dd>
          </div>
          <div>
            <dt>{{ t("connections.statRules") }}</dt>
            <dd>{{ configuredCount }}</dd>
          </div>
        </dl>
        <SelectMenu
          v-if="view === 'apps'"
          v-model="filter"
          class="pnm-filter"
          :label="t('connections.filterAll')"
          :options="filterOptions"
        />
        <BaseButton
          v-else-if="connectionApp === null"
          class="pnm-close-all"
          :disabled="closing || !snapshot.active.length"
          @click="requestCloseAll"
        >
          <X :size="15" />{{ t("connections.closeAll") }}
        </BaseButton>
      </div>
    </div>
    <p v-if="error || loadError || snapshot.error" class="pnm-error" role="alert">
      {{ tx(error || loadError || snapshot.error) }}
    </p>
    <p v-if="search.error" class="pnm-error" role="alert">{{ t("connections.invalidRegex") }}</p>
    <div class="pnm-list">
      <div class="pnm-list-head">
        <div class="pnm-search" :class="{ invalid: search.error }">
          <Search :size="16" /><input
            v-model="query"
            :aria-label="t('connections.filterConnections')"
            :aria-invalid="search.error"
            :placeholder="t('connections.searchHostProcessOrChain')"
          /><BaseButton v-if="query" :aria-label="t('common.close')" @click="query = ''">
            <X :size="14" />
          </BaseButton>
          <div class="pnm-search-modes">
            <BaseButton
              v-for="mode in searchModes"
              :key="mode.key"
              :class="{ on: searchOptions[mode.key] }"
              :aria-label="t(mode.label)"
              :title="t(mode.label)"
              :aria-pressed="searchOptions[mode.key]"
              @click="searchOptions[mode.key] = !searchOptions[mode.key]"
            >
              <component :is="mode.icon" :size="17" />
            </BaseButton>
          </div>
        </div>
        <BaseButton
          v-if="view === 'connections' && connectionApp !== null"
          class="pnm-app-scope"
          :aria-label="`${t('connections.filterAll')} · ${connectionApp || t('connections.unknownApp')}`"
          @click="connectionApp = null"
        >
          <AppWindow :size="14" /><span :title="connectionApp || t('connections.unknownApp')">{{
            connectionApp || t("connections.unknownApp")
          }}</span
          ><X :size="13" />
        </BaseButton>
        <div v-if="view === 'connections'" class="pnm-segment pnm-status">
          <BaseButton
            :aria-pressed="status === 'active'"
            :class="{ on: status === 'active' }"
            @click="status = 'active'"
          >
            {{ t("connections.active") }} <span>{{ scopedActive.length }}</span>
          </BaseButton>
          <BaseButton
            :aria-pressed="status === 'closed'"
            :class="{ on: status === 'closed' }"
            @click="status = 'closed'"
          >
            {{ t("connections.closed") }} <span>{{ scopedClosed.length }}</span>
          </BaseButton>
        </div>
        <span v-else class="pnm-list-label"
          >{{ t("connections.appList") }} <span>{{ filteredGroups.length }}</span></span
        >
      </div>
      <div class="pnm-inspector-layout" @keydown.esc.stop.prevent="closeConnection">
        <div class="pnm-table-scroll">
          <table v-if="view === 'apps'" class="pnm-table pnm-app-table">
            <colgroup>
              <col style="width: 36%" />
              <col style="width: 12%" />
              <col style="width: 19%" />
              <col style="width: 19%" />
              <col style="width: 14%" />
            </colgroup>
            <thead>
              <tr>
                <th>{{ t("connections.apps") }}</th>
                <th class="number">{{ t("connections.count") }}</th>
                <th class="number">{{ t("connections.downloadSpeed") }}</th>
                <th class="number">{{ t("connections.uploadSpeed") }}</th>
                <th class="policy-cell">{{ t("connections.policy") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="group in filteredGroups"
                :key="group.app"
                class="pnm-app-row"
                @click="showAppConnections(group.app)"
              >
                <td>
                  <BaseButton
                    class="pnm-app-identity"
                    :aria-label="`${group.app || t('connections.unknownApp')} · ${t('connections.count')}`"
                    @click.stop="showAppConnections(group.app)"
                  >
                    <AppIcon :path="group.path" /><span class="pnm-app-text"
                      ><strong :title="group.app || t('connections.unknownApp')">{{
                        group.app || t("connections.unknownApp")
                      }}</strong
                      ><small v-if="!group.rows.length">{{ t("connections.noActiveConnections") }}</small
                      ><small v-else-if="group.exe && group.exe !== group.app" :title="group.exe">{{
                        group.exe
                      }}</small></span
                    >
                  </BaseButton>
                </td>
                <td class="number">{{ group.rows.length }}</td>
                <td class="number">{{ bytes(group.downloadSpeed) }}/s</td>
                <td class="number">{{ bytes(group.uploadSpeed) }}/s</td>
                <td class="policy-cell">
                  <BaseButton
                    class="pnm-policy-trigger"
                    :aria-label="`${group.app || t('connections.unknownApp')} · ${t('connections.netPolicy')}`"
                    aria-haspopup="dialog"
                    :aria-expanded="detailOpen && selectedApp === group.app"
                    @click.stop="openDetail(group, $event)"
                  >
                    {{ policyShort(group.app) }}<ChevronDown :size="14" />
                  </BaseButton>
                </td>
              </tr>
            </tbody>
          </table>
          <table v-else class="pnm-table pnm-connection-table">
            <colgroup>
              <col style="width: 25%" />
              <col style="width: 13%" />
              <col style="width: 13%" />
              <col style="width: 14%" />
              <col style="width: 14%" />
              <col style="width: 21%" />
            </colgroup>
            <thead>
              <tr>
                <th>{{ t("connections.host") }}</th>
                <th class="number">{{ t("connections.downloaded") }}</th>
                <th class="number">{{ t("connections.uploaded") }}</th>
                <th class="number">{{ t("connections.downloadSpeed") }}</th>
                <th class="number">{{ t("connections.uploadSpeed") }}</th>
                <th>{{ t("connections.chain") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="row in filteredRows"
                :key="row.id"
                v-memo="[row.download, row.upload, row.downloadSpeed, row.uploadSpeed, selection?.row.id === row.id]"
                class="pnm-connection-row"
                :class="{ selected: selection?.row.id === row.id }"
                @click="openConnection(row, $event)"
              >
                <td :title="host(row)">
                  <BaseButton
                    class="pnm-connection-host"
                    :aria-pressed="selection?.row.id === row.id"
                    :aria-label="`${t('connections.detailTitle')} · ${host(row)}`"
                    @click.stop="openConnection(row, $event)"
                  >
                    {{ host(row) }}
                  </BaseButton>
                </td>
                <td class="number">{{ bytes(row.download) }}</td>
                <td class="number">{{ bytes(row.upload) }}</td>
                <td class="number">{{ bytes(status === "closed" ? 0 : row.downloadSpeed) }}/s</td>
                <td class="number">{{ bytes(status === "closed" ? 0 : row.uploadSpeed) }}/s</td>
                <td :title="chain(row)">{{ chain(row) }}</td>
              </tr>
            </tbody>
          </table>
          <div v-if="view === 'apps' ? !filteredGroups.length : !filteredRows.length" class="pnm-empty">
            <Network :size="24" /><span>{{
              t(
                query
                  ? "connections.noMatchingConnections"
                  : snapshot.running
                    ? status === "closed" && view === "connections"
                      ? "connections.noClosedConnections"
                      : "connections.noActiveConnections"
                    : "connections.connectToSeeNetworkRequests"
              )
            }}</span>
          </div>
        </div>
      </div>
    </div>
    <div
      ref="dialog"
      popover="auto"
      role="dialog"
      :aria-label="t('connections.appDetails')"
      class="pnm-popover"
      @toggle="detailOpen = $event.newState === 'open'"
    >
      <template v-if="activeApp">
        <div class="pnm-dialog-head">
          <div class="pnm-detail-head">
            <AppIcon :path="activeApp.path" />
            <div>
              <h3>{{ activeApp.app || t("connections.unknownApp") }}</h3>
              <span class="pnm-app-exe">{{ activeApp.exe }}</span>
            </div>
          </div>
          <BaseButton class="pnm-dialog-close" :aria-label="t('common.close')" @click="closeDetail()">
            <X :size="19" />
          </BaseButton>
        </div>
        <dl class="pnm-detail-stats">
          <div>
            <dt>{{ t("connections.activeConns") }}</dt>
            <dd>{{ activeApp.rows.length }}</dd>
          </div>
          <div>
            <dt>{{ t("connections.totalTraffic") }}</dt>
            <dd>{{ bytes(activeApp.download + activeApp.upload) }}</dd>
          </div>
        </dl>
        <h4>{{ t("connections.netPolicy") }}</h4>
        <p v-if="!activeApp.app" class="pnm-policy-notice">
          <Info :size="14" /><span>{{ t("connections.unknownPolicyHint") }}</span>
        </p>
        <div class="pnm-policies">
          <div
            v-for="opt in policyMeta"
            :key="opt.value"
            class="pnm-policy"
            :class="{ on: policyOf(activeApp.app) === opt.value }"
          >
            <BaseButton
              class="pnm-policy-choice"
              :disabled="generating || !activeApp.app"
              :aria-pressed="policyOf(activeApp.app) === opt.value"
              @click="pick(activeApp.app, opt.value)"
            >
              <component :is="opt.icon" class="pnm-policy-icon" :size="18" :stroke-width="1.8" />
              <span class="pnm-policy-label">{{ t(opt.label) }}</span>
              <Check
                v-if="policyOf(activeApp.app) === opt.value"
                class="pnm-policy-check"
                :size="16"
                :stroke-width="2"
              /><span v-else class="pnm-policy-check" />
            </BaseButton>
            <HelpTip :text="t(opt.desc)" inline />
          </div>
        </div>
        <div class="pnm-policy-footer">
          <p v-if="applyMsg" class="pnm-apply" :class="applyTone" role="status">
            {{ t(applyMsg) }}
          </p>
          <BaseButton v-if="activeApp.app" class="pnm-generate" :disabled="generating || !dirty" @click="save">
            {{ generating ? t("connections.generating") : t("connections.savePolicy") }}
          </BaseButton>
          <BaseButton v-else class="pnm-dismiss" @click="closeDetail()">
            {{ t("common.close") }}
          </BaseButton>
        </div>
      </template>
    </div>
    <div
      ref="connDialog"
      popover="auto"
      role="dialog"
      :aria-label="t('connections.detailTitle')"
      class="pnm-popover pnm-conn-popover"
      @toggle="onConnToggle"
    >
      <ConnectionDetails
        v-if="selection"
        :row="selection.row"
        :active="selectedConnectionActive"
        :locale="locale"
        :chain="chain(selection.row)"
        @close="closeConnection"
        @close-connection="closeThisConnection"
      />
    </div>
    <div
      ref="closePopover"
      popover="auto"
      role="dialog"
      :aria-label="t('connections.closeAllConnections')"
      class="pnm-popover pnm-confirm"
    >
      <strong>{{ t("connections.closeAllConnections") }}</strong>
      <p>{{ t("connections.activeDownloadsAndRequestsWill") }}</p>
      <div>
        <BaseButton @click="closePopover?.hidePopover()"> {{ t("common.cancel") }}</BaseButton><BaseButton
          :disabled="closing"
          @click="closeAll"
        >
          {{ t("connections.closeAll") }}
        </BaseButton>
      </div>
    </div>
  </section>
</template>
<script setup lang="ts">
  import { computed, nextTick, onMounted, onUnmounted, ref, shallowRef, type Component } from "vue";
  import { invoke } from "@tauri-apps/api/core";
  import {
    AppWindow,
    ArrowUpRight,
    Ban,
    Check,
    Info,
    CaseSensitive,
    WholeWord,
    Regex,
    ChevronDown,
    Search,
    Network,
    Route,
    ShieldCheck,
    X,
  } from "@lucide/vue";
  import AppIcon from "../../components/AppIcon.vue";
  import { connectionSearch, type SearchOptions } from "../../lib/connection-search";
  import { bindConnectionWatch } from "../../lib/connection-watch";
  import { clampIntoViewport } from "../../lib/floating";
  import SelectMenu from "../../components/ui/SelectMenu.vue";
  import BaseButton from "../../components/ui/BaseButton.vue";
  import HelpTip from "../../components/ui/HelpTip.vue";
  import ConnectionDetails from "../../components/ConnectionDetails.vue";
  import {
    connectionEndpoint,
    refreshConnectionSelection,
    type ConnectionRow,
    type ConnectionSelection,
  } from "../../lib/connection-details";
  import { useApp } from "../../composables/useApp";
  import { applicationGroups, connectionsForApp, type ApplicationGroup, type AppPolicy } from "../../lib/app-policies";
  import { translator, type Locale, type MessageKey } from "../../i18n";
  const props = defineProps<{ locale: Locale; nodeNames?: Record<string, string> }>();
  const { t, tx } = translator(() => props.locale);
  const { appPolicies, setAppPolicy } = useApp();
  type Row = ConnectionRow;
  type Snapshot = {
    session: number;
    running: boolean;
    error?: string;
    uploadTotal: number;
    downloadTotal: number;
    active: Row[];
    closed: Row[];
  };
  type AppGroup = ApplicationGroup<Row>;
  // shallowRef: the poll replaces the whole snapshot each tick, so we only need reactivity on the
  // `.value` identity. This skips recursively proxying hundreds of rows (+ nested metadata) every
  // 1.5s and the per-read Proxy getter cost while rendering — the rows are read-only, replace-only.
  const snapshot = shallowRef<Snapshot>({
    session: 0,
    running: false,
    uploadTotal: 0,
    downloadTotal: 0,
    active: [],
    closed: [],
  });
  const selection = ref<ConnectionSelection>();
  let connectionAnchor: HTMLElement | undefined;
  const selectedConnectionActive = computed(
    () => snapshot.value.running && snapshot.value.active.some(row => row.id === selection.value?.row.id)
  );
  async function openConnection(row: Row, event: MouseEvent) {
    selection.value = { session: snapshot.value.session, row };
    const target = event.currentTarget as HTMLElement;
    connectionAnchor = target.matches("button") ? target : target.querySelector<HTMLElement>("button") || undefined;
    await nextTick();
    if (!connDialog.value?.matches(":popover-open")) connDialog.value?.showPopover();
  }
  function closeConnection() {
    if (connDialog.value?.matches(":popover-open")) connDialog.value.hidePopover();
    else {
      selection.value = undefined;
      if (connectionAnchor?.isConnected) connectionAnchor.focus();
    }
  }
  function onConnToggle(event: ToggleEvent) {
    if (event.newState === "closed") {
      selection.value = undefined;
      if (connectionAnchor?.isConnected) connectionAnchor.focus();
    }
  }
  async function closeThisConnection() {
    const sel = selection.value;
    if (!sel) return;
    try {
      await invoke("close_connections", { session: snapshot.value.session, id: sel.row.id });
      if (connDialog.value?.matches(":popover-open")) connDialog.value.hidePopover();
      await refresh();
    } catch (e) {
      error.value = String(e);
    }
  }
  const selectedApp = ref("");
  const dialog = ref<HTMLElement>();
  const connDialog = ref<HTMLElement>();
  const closePopover = ref<HTMLElement>();
  const detailOpen = ref(false);
  const selectedGroup = ref<AppGroup>();
  const view = ref<"apps" | "connections">("apps");
  const connectionApp = ref<string | null>(null);
  const status = ref<"active" | "closed">("active");
  const query = ref("");
  const searchOptions = ref<SearchOptions>({ caseSensitive: false, wholeWord: false, regex: false });
  const search = computed(() => connectionSearch(query.value, searchOptions.value));
  const searchModes = [
    { key: "caseSensitive", icon: CaseSensitive, label: "connections.caseSensitive" },
    { key: "wholeWord", icon: WholeWord, label: "connections.wholeWord" },
    { key: "regex", icon: Regex, label: "connections.regex" },
  ] as const;
  const closing = ref(false);
  let closeSession = 0;
  let anchor: HTMLElement | undefined;
  function positionPopover(element: HTMLElement, target: HTMLElement) {
    const rect = target.getBoundingClientRect();
    element.style.left = `${clampIntoViewport(rect.right - element.offsetWidth, element.offsetWidth, window.innerWidth, 12)}px`;
    element.style.top = `${clampIntoViewport(rect.bottom + 8, element.offsetHeight, window.innerHeight, 12)}px`;
  }
  async function openDetail(group: AppGroup, event: MouseEvent) {
    selectedApp.value = group.app;
    selectedGroup.value = group;
    applyMsg.value = "";
    anchor = event.currentTarget as HTMLElement;
    await nextTick();
    dialog.value?.showPopover();
    if (dialog.value && anchor) positionPopover(dialog.value, anchor);
  }
  function closeDetail() {
    dialog.value?.hidePopover();
    anchor?.focus();
  }
  function hideConnectionPopover() {
    if (connDialog.value?.matches(":popover-open")) connDialog.value.hidePopover();
  }
  function changeView(value: "apps" | "connections") {
    closeDetail();
    hideConnectionPopover();
    selection.value = undefined;
    view.value = value;
    query.value = "";
    connectionApp.value = null;
  }
  function showAppConnections(app: string) {
    closeDetail();
    hideConnectionPopover();
    selection.value = undefined;
    connectionApp.value = app;
    query.value = "";
    status.value = "active";
    view.value = "connections";
  }
  function host(row: Row) {
    return connectionEndpoint(row.metadata.host || row.metadata.destinationIP, row.metadata.destinationPort);
  }
  function chain(row: Row) {
    return row.chains.map(tag => props.nodeNames?.[tag] || tag).join(" / ") || "—";
  }
  const scopedActive = computed(() => connectionsForApp(snapshot.value.active, connectionApp.value));
  const scopedClosed = computed(() => connectionsForApp(snapshot.value.closed, connectionApp.value));
  const filteredRows = computed(() => {
    const rows = status.value === "active" ? scopedActive.value : scopedClosed.value;
    // Empty query matches everything, so skip building the per-row search haystack (host + chain
    // string work on every row) when there is nothing to match against.
    if (!query.value.trim()) return rows;
    return rows.filter(row => search.value.matches(`${host(row)} ${row.metadata.processPath} ${chain(row)}`));
  });
  function requestCloseAll(event: MouseEvent) {
    closeSession = snapshot.value.session;
    closePopover.value?.showPopover();
    if (closePopover.value) positionPopover(closePopover.value, event.currentTarget as HTMLElement);
  }
  async function closeAll() {
    if (closing.value) return;
    closing.value = true;
    error.value = "";
    try {
      await invoke("close_connections", { session: closeSession, id: null });
      closePopover.value?.hidePopover();
      await refresh();
    } catch (e) {
      error.value = String(e);
    } finally {
      closing.value = false;
    }
  }
  function dismissPopovers() {
    dialog.value?.hidePopover();
    closePopover.value?.hidePopover();
    if (connDialog.value?.matches(":popover-open")) connDialog.value.hidePopover();
  }
  const filter = ref<"all" | "configured" | "proxy" | "direct" | "block">("all");
  const error = ref("");
  const loadError = ref("");
  const generating = ref(false);
  const applyMsg = ref<MessageKey | "">("");
  const applyTone = ref<"ok" | "bad">("ok");
  const policyMeta = [
    {
      value: "inherit",
      icon: Route,
      label: "connections.inheritLabel",
      desc: "connections.inheritDesc",
    },
    {
      value: "proxy",
      icon: ShieldCheck,
      label: "connections.proxyLabel",
      desc: "connections.proxyDesc",
    },
    {
      value: "direct",
      icon: ArrowUpRight,
      label: "connections.directLabel",
      desc: "connections.directDesc",
    },
    { value: "block", icon: Ban, label: "connections.blockLabel", desc: "connections.blockDesc" },
  ] as { value: AppPolicy; icon: Component; label: MessageKey; desc: MessageKey }[];
  const policyShortKey: Record<AppPolicy, MessageKey> = {
    inherit: "connections.inheritShort",
    proxy: "connections.proxyShort",
    direct: "connections.directLabel",
    block: "connections.blockShort",
  };
  // Draft: picking only mutates the draft; the "generate rule" button is what persists the draft as a rule.
  const draft = ref<Record<string, AppPolicy>>({});
  const savedOf = (app: string): AppPolicy =>
    Object.prototype.hasOwnProperty.call(appPolicies.value, app) ? appPolicies.value[app] : "inherit";
  const dirty = computed(
    () => draft.value[selectedApp.value] !== undefined && draft.value[selectedApp.value] !== savedOf(selectedApp.value)
  );
  function pick(app: string, policy: AppPolicy) {
    draft.value = { ...draft.value, [app]: policy };
  }
  const filterOptions = computed(
    () =>
      [
        { value: "all", label: t("connections.filterAll") },
        { value: "configured", label: t("connections.filterConfigured") },
        { value: "proxy", label: t("connections.proxyLabel") },
        { value: "direct", label: t("connections.directLabel") },
        { value: "block", label: t("connections.blockLabel") },
      ] as { value: typeof filter.value; label: string }[]
  );
  const policyOf = (app: string): AppPolicy => draft.value[app] ?? savedOf(app);
  const policyShort = (app: string) => t(policyShortKey[savedOf(app)]);
  function bytes(value: number) {
    const units = ["B", "KB", "MB", "GB", "TB"];
    let v = Math.max(0, value || 0),
      i = 0;
    while (v >= 1024 && i < units.length - 1) {
      v /= 1024;
      i++;
    }
    return `${v.toFixed(i ? 1 : 0)} ${units[i]}`;
  }
  const groups = computed<AppGroup[]>(() =>
    applicationGroups(snapshot.value.active, snapshot.value.closed, appPolicies.value)
  );
  const filteredGroups = computed(() => {
    const q = query.value.trim();
    return groups.value.filter(group => {
      const passesFilter =
        filter.value === "all" ||
        (filter.value === "configured" ? savedOf(group.app) !== "inherit" : savedOf(group.app) === filter.value);
      if (!passesFilter) return false;
      // Skip the search haystack build when there is no query.
      return !q || search.value.matches(`${group.app || t("connections.unknownApp")} ${group.exe}`);
    });
  });
  const activeApp = computed(() => groups.value.find(group => group.app === selectedApp.value) || selectedGroup.value);
  const totalConns = computed(() => groups.value.reduce((sum, group) => sum + group.rows.length, 0));
  const configuredCount = computed(() => Object.keys(appPolicies.value).length);
  async function save() {
    if (generating.value || !dirty.value) return;
    generating.value = true;
    applyMsg.value = "";
    try {
      const app = selectedApp.value;
      const policy = draft.value[app];
      if (!app || !policy) return;
      await setAppPolicy(app, policy);
      delete draft.value[app];
      applyTone.value = "ok";
      applyMsg.value = "connections.applied";
      await refresh();
    } catch (e) {
      applyTone.value = "bad";
      applyMsg.value = "connections.applyFailed";
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      generating.value = false;
    }
  }
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const connectionWatch = bindConnectionWatch("details");
  let popoverResize: ResizeObserver | undefined;
  async function refresh() {
    try {
      const result = await invoke<Snapshot>("list_connections");
      if (!stopped) {
        snapshot.value = result;
        selection.value = refreshConnectionSelection(selection.value, result);
        loadError.value = "";
      }
    } catch {
      if (!stopped) loadError.value = t("connections.unableToLoadConnectionsMake");
    }
  }
  async function poll() {
    if (stopped) return;
    if (!document.hidden) await refresh();
    if (!stopped) timer = setTimeout(poll, 1500);
  }
  onMounted(() => {
    popoverResize = new ResizeObserver(() => {
      if (dialog.value?.matches(":popover-open") && anchor?.isConnected) positionPopover(dialog.value, anchor);
    });
    if (dialog.value) popoverResize.observe(dialog.value);
    window.addEventListener("resize", dismissPopovers);
    if ("__TAURI_INTERNALS__" in window) {
      connectionWatch.start();
      void poll();
    } else error.value = t("connections.connectionMonitoringIsAvailableIn");
  });
  onUnmounted(() => {
    stopped = true;
    clearTimeout(timer);
    connectionWatch.stop();
    popoverResize?.disconnect();
    window.removeEventListener("resize", dismissPopovers);
  });
</script>
<style scoped lang="scss" src="./index.scss"></style>
