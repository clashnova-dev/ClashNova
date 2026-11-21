<template>
  <section class="pnm-page">
    <div class="pnm-overview">
      <div class="pnm-topbar">
        <div class="pnm-segment" :aria-label="t('connections.processTitle')">
          <button type="button" :aria-pressed="view === 'apps'" :class="{ on: view === 'apps' }" @click="changeView('apps')"><AppWindow :size="16" />{{ t('connections.apps') }}</button>
          <button type="button" :aria-pressed="view === 'connections'" :class="{ on: view === 'connections' }" @click="changeView('connections')"><Network :size="16" />{{ t('connections.count') }}</button>
        </div>
        <dl class="pnm-stats">
          <div><dt>{{ t('connections.statApps') }}</dt><dd>{{ groups.length }}</dd></div>
          <div><dt>{{ t('connections.statConns') }}</dt><dd>{{ totalConns }}</dd></div>
          <div><dt>{{ t('connections.statRules') }}</dt><dd>{{ configuredCount }}</dd></div>
        </dl>
        <SelectMenu v-if="view === 'apps'" v-model="filter" class="pnm-filter" :label="t('connections.filterAll')" :options="filterOptions" />
        <button v-else-if="connectionApp === null" type="button" class="pnm-close-all" :disabled="closing || !snapshot.active.length" @click="requestCloseAll"><X :size="15" />{{ t('connections.closeAll') }}</button>
      </div>
    </div>
    <p v-if="error || loadError || snapshot.error" class="pnm-error" role="alert">{{ tx(error || loadError || snapshot.error) }}</p>
    <p v-if="search.error" class="pnm-error" role="alert">{{ t('connections.invalidRegex') }}</p>
    <div class="pnm-list">
      <div class="pnm-list-head">
        <div class="pnm-search" :class="{ invalid: search.error }"><Search :size="16" /><input v-model="query" :aria-label="t('connections.filterConnections')" :aria-invalid="search.error" :placeholder="t('connections.searchHostProcessOrChain')" /><button v-if="query" type="button" :aria-label="t('common.close')" @click="query = ''"><X :size="14" /></button><div class="pnm-search-modes"><button v-for="mode in searchModes" :key="mode.key" type="button" :class="{ on: searchOptions[mode.key] }" :aria-label="t(mode.label)" :title="t(mode.label)" :aria-pressed="searchOptions[mode.key]" @click="searchOptions[mode.key] = !searchOptions[mode.key]"><component :is="mode.icon" :size="17" /></button></div></div>
        <button v-if="view === 'connections' && connectionApp !== null" type="button" class="pnm-app-scope" :aria-label="`${t('connections.filterAll')} · ${connectionApp || t('connections.unknownApp')}`" @click="connectionApp = null"><AppWindow :size="14" /><span :title="connectionApp || t('connections.unknownApp')">{{ connectionApp || t('connections.unknownApp') }}</span><X :size="13" /></button>
        <div v-if="view === 'connections'" class="pnm-segment pnm-status">
          <button type="button" :aria-pressed="status === 'active'" :class="{ on: status === 'active' }" @click="status = 'active'">{{ t('connections.active') }} <span>{{ scopedActive.length }}</span></button>
          <button type="button" :aria-pressed="status === 'closed'" :class="{ on: status === 'closed' }" @click="status = 'closed'">{{ t('connections.closed') }} <span>{{ scopedClosed.length }}</span></button>
        </div>
        <span v-else class="pnm-list-label">{{ t('connections.appList') }} <span>{{ filteredGroups.length }}</span></span>
      </div>
      <div class="pnm-table-scroll">
        <table v-if="view === 'apps'" class="pnm-table pnm-app-table">
          <colgroup><col style="width:36%" /><col style="width:12%" /><col style="width:19%" /><col style="width:19%" /><col style="width:14%" /></colgroup>
          <thead><tr><th>{{ t('connections.apps') }}</th><th class="number">{{ t('connections.count') }}</th><th class="number">{{ t('connections.downloadSpeed') }}</th><th class="number">{{ t('connections.uploadSpeed') }}</th><th class="policy-cell">{{ t('connections.policy') }}</th></tr></thead>
          <tbody><tr v-for="group in filteredGroups" :key="group.app" class="pnm-app-row" @click="showAppConnections(group.app)">
            <td><button type="button" class="pnm-app-identity" :aria-label="`${group.app || t('connections.unknownApp')} · ${t('connections.count')}`" @click.stop="showAppConnections(group.app)"><AppIcon :path="group.rows[0]?.metadata.processPath" /><span class="pnm-app-text"><strong :title="group.app || t('connections.unknownApp')">{{ group.app || t('connections.unknownApp') }}</strong><small v-if="group.exe && group.exe !== group.app" :title="group.exe">{{ group.exe }}</small></span></button></td>
            <td class="number">{{ group.rows.length }}</td><td class="number">{{ bytes(group.downloadSpeed) }}/s</td><td class="number">{{ bytes(group.uploadSpeed) }}/s</td>
            <td class="policy-cell"><button type="button" class="pnm-policy-trigger" :aria-label="`${group.app || t('connections.unknownApp')} · ${t('connections.netPolicy')}`" aria-haspopup="dialog" :aria-expanded="detailOpen && selectedApp === group.app" @click.stop="openDetail(group, $event)">{{ policyShort(group.app) }}<ChevronDown :size="14" /></button></td>
          </tr></tbody>
        </table>
        <table v-else class="pnm-table pnm-connection-table">
          <colgroup><col style="width:25%" /><col style="width:13%" /><col style="width:13%" /><col style="width:14%" /><col style="width:14%" /><col style="width:21%" /></colgroup>
          <thead><tr><th>{{ t('connections.host') }}</th><th class="number">{{ t('connections.downloaded') }}</th><th class="number">{{ t('connections.uploaded') }}</th><th class="number">{{ t('connections.downloadSpeed') }}</th><th class="number">{{ t('connections.uploadSpeed') }}</th><th>{{ t('connections.chain') }}</th></tr></thead>
          <tbody><tr v-for="row in filteredRows" :key="row.id"><td :title="host(row)">{{ host(row) }}</td><td class="number">{{ bytes(row.download) }}</td><td class="number">{{ bytes(row.upload) }}</td><td class="number">{{ bytes(status === 'closed' ? 0 : row.downloadSpeed) }}/s</td><td class="number">{{ bytes(status === 'closed' ? 0 : row.uploadSpeed) }}/s</td><td :title="chain(row)">{{ chain(row) }}</td></tr></tbody>
        </table>
        <div v-if="view === 'apps' ? !filteredGroups.length : !filteredRows.length" class="pnm-empty"><Network :size="24" /><span>{{ t(query ? 'connections.noMatchingConnections' : snapshot.running ? status === 'closed' && view === 'connections' ? 'connections.noClosedConnections' : 'connections.noActiveConnections' : 'connections.connectToSeeNetworkRequests') }}</span></div>
      </div>
    </div>
    <div ref="dialog" popover="auto" role="dialog" :aria-label="t('connections.appDetails')" class="pnm-popover" @toggle="detailOpen = $event.newState === 'open'">
        <template v-if="activeApp">
          <div class="pnm-dialog-head">
            <div class="pnm-detail-head"><AppIcon :path="activeApp.rows[0]?.metadata.processPath" /><div><h3>{{ activeApp.app || t('connections.unknownApp') }}</h3><span class="pnm-app-exe">{{ activeApp.exe }}</span></div></div>
            <button type="button" class="pnm-dialog-close" :aria-label="t('common.close')" @click="closeDetail()"><X :size="19" /></button>
          </div>
          <dl class="pnm-detail-stats">
            <div><dt>{{ t('connections.activeConns') }}</dt><dd>{{ activeApp.rows.length }}</dd></div>
            <div><dt>{{ t('connections.totalTraffic') }}</dt><dd>{{ bytes(activeApp.download + activeApp.upload) }}</dd></div>
          </dl>
          <h4>{{ t('connections.netPolicy') }}</h4>
          <p v-if="!activeApp.app" class="pnm-policy-notice"><Info :size="14" /><span>{{ t('connections.unknownPolicyHint') }}</span></p>
          <div class="pnm-policies">
            <button v-for="opt in policyMeta" :key="opt.value" type="button" class="pnm-policy" :disabled="generating || !activeApp.app" :aria-pressed="policyOf(activeApp.app) === opt.value" :class="{ on: policyOf(activeApp.app) === opt.value }" @click="pick(activeApp.app, opt.value)">
              <component :is="opt.icon" class="pnm-policy-icon" :size="18" :stroke-width="1.8" />
              <span class="pnm-policy-text"><span class="pnm-policy-label">{{ t(opt.label) }}</span><span class="pnm-policy-desc">{{ t(opt.desc) }}</span></span>
              <Check v-if="policyOf(activeApp.app) === opt.value" class="pnm-policy-check" :size="16" :stroke-width="2" /><span v-else class="pnm-policy-check" />
            </button>
          </div>
          <div class="pnm-policy-footer">
            <span v-if="activeApp.app" class="pnm-policy-timing">{{ t('connections.nextConnect') }}</span>
            <button v-if="activeApp.app" type="button" class="pnm-generate" :disabled="generating || !dirty" @click="save">{{ generating ? t('connections.generating') : t('connections.savePolicy') }}</button>
            <button v-else type="button" class="pnm-dismiss" @click="closeDetail()">{{ t('common.close') }}</button>
          </div>
          <p v-if="applyMsg" class="pnm-apply" :class="applyTone">{{ t(applyMsg) }}</p>
        </template>
    </div>
    <div ref="closePopover" popover="auto" role="dialog" :aria-label="t('connections.closeAllConnections')" class="pnm-popover pnm-confirm">
      <strong>{{ t('connections.closeAllConnections') }}</strong><p>{{ t('connections.activeDownloadsAndRequestsWill') }}</p>
      <div><button type="button" @click="closePopover?.hidePopover()">{{ t('common.cancel') }}</button><button type="button" :disabled="closing" @click="closeAll">{{ t('connections.closeAll') }}</button></div>
    </div>
  </section>
</template>
<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, type Component } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { AppWindow, ArrowUpRight, Ban, Check, Info, CaseSensitive, WholeWord, Regex, ChevronDown, Search, Network, Route, ShieldCheck, X } from '@lucide/vue';
import AppIcon from '../components/AppIcon.vue';
import { connectionSearch, type SearchOptions } from '../lib/connection-search';
import SelectMenu from '../components/ui/SelectMenu.vue';
import { useApp } from '../composables/useApp';
import { appNameFromPath, connectionsForApp, processExecutablePath, type AppPolicy } from '../lib/app-policies';
import { translator, type Locale, type MessageKey } from '../i18n';
const props = defineProps<{ locale: Locale; nodeNames?: Record<string, string> }>();
const { t, tx } = translator(() => props.locale);
const { appPolicies, setAppPolicy } = useApp();
type Row = { id: string; metadata: { host: string; destinationIP: string; destinationPort: string; processPath: string }; upload: number; download: number; uploadSpeed: number; downloadSpeed: number; start: string; chains: string[] };
type Snapshot = { session: number; running: boolean; error?: string; uploadTotal: number; downloadTotal: number; active: Row[]; closed: Row[] };
type AppGroup = { app: string; exe: string; rows: Row[]; download: number; upload: number; downloadSpeed: number; uploadSpeed: number };
const snapshot = ref<Snapshot>({ session: 0, running: false, uploadTotal: 0, downloadTotal: 0, active: [], closed: [] });
const selectedApp = ref('');
const dialog = ref<HTMLElement>();
const closePopover = ref<HTMLElement>();
const detailOpen = ref(false);
const selectedGroup = ref<AppGroup>();
const view = ref<'apps' | 'connections'>('apps');
const connectionApp = ref<string | null>(null);
const status = ref<'active' | 'closed'>('active');
const query = ref('');
const searchOptions = ref<SearchOptions>({ caseSensitive: false, wholeWord: false, regex: false });
const search = computed(() => connectionSearch(query.value, searchOptions.value));
const searchModes = [
  { key: 'caseSensitive', icon: CaseSensitive, label: 'connections.caseSensitive' },
  { key: 'wholeWord', icon: WholeWord, label: 'connections.wholeWord' },
  { key: 'regex', icon: Regex, label: 'connections.regex' },
] as const;
const closing = ref(false);
let closeSession = 0;
let anchor: HTMLElement | undefined;
function positionPopover(element: HTMLElement, target: HTMLElement) {
  const rect = target.getBoundingClientRect();
  element.style.left = `${Math.max(12, Math.min(rect.right - element.offsetWidth, window.innerWidth - element.offsetWidth - 12))}px`;
  element.style.top = `${Math.max(12, Math.min(rect.bottom + 8, window.innerHeight - element.offsetHeight - 12))}px`;
}
async function openDetail(group: AppGroup, event: MouseEvent) {
  selectedApp.value = group.app; selectedGroup.value = group; applyMsg.value = '';
  anchor = event.currentTarget as HTMLElement;
  await nextTick();
  dialog.value?.showPopover();
  if (dialog.value && anchor) positionPopover(dialog.value, anchor);
}
function closeDetail() { dialog.value?.hidePopover(); anchor?.focus(); }
function changeView(value: 'apps' | 'connections') { closeDetail(); view.value = value; query.value = ''; connectionApp.value = null; }
function showAppConnections(app: string) {
  closeDetail(); connectionApp.value = app; query.value = ''; status.value = 'active'; view.value = 'connections';
}
function host(row: Row) { const value = row.metadata.host || row.metadata.destinationIP; return `${value.includes(':') ? `[${value}]` : value}:${row.metadata.destinationPort}`; }
function chain(row: Row) { return row.chains.map(tag => props.nodeNames?.[tag] || tag).join(' / ') || '—'; }
const scopedActive = computed(() => connectionsForApp(snapshot.value.active, connectionApp.value));
const scopedClosed = computed(() => connectionsForApp(snapshot.value.closed, connectionApp.value));
const filteredRows = computed(() => (status.value === 'active' ? scopedActive.value : scopedClosed.value).filter(row => search.value.matches(`${host(row)} ${row.metadata.processPath} ${chain(row)}`)));
function requestCloseAll(event: MouseEvent) {
  closeSession = snapshot.value.session;
  closePopover.value?.showPopover();
  if (closePopover.value) positionPopover(closePopover.value, event.currentTarget as HTMLElement);
}
async function closeAll() {
  if (closing.value) return;
  closing.value = true; error.value = '';
  try { await invoke('close_connections', { session: closeSession, id: null }); closePopover.value?.hidePopover(); await refresh(); }
  catch (e) { error.value = String(e); }
  finally { closing.value = false; }
}
function dismissPopovers() { dialog.value?.hidePopover(); closePopover.value?.hidePopover(); }
const filter = ref<'all' | 'configured' | 'proxy' | 'direct' | 'block'>('all');
const error = ref('');
const loadError = ref('');
const generating = ref(false);
const applyMsg = ref<MessageKey | ''>('');
const applyTone = ref<'ok' | 'bad'>('ok');
const policyMeta = [
  { value: 'inherit', icon: Route, label: 'connections.inheritLabel', desc: 'connections.inheritDesc' },
  { value: 'proxy', icon: ShieldCheck, label: 'connections.proxyLabel', desc: 'connections.proxyDesc' },
  { value: 'direct', icon: ArrowUpRight, label: 'connections.directLabel', desc: 'connections.directDesc' },
  { value: 'block', icon: Ban, label: 'connections.blockLabel', desc: 'connections.blockDesc' },
] as { value: AppPolicy; icon: Component; label: MessageKey; desc: MessageKey }[];
const policyShortKey: Record<AppPolicy, MessageKey> = { inherit: 'connections.inheritShort', proxy: 'connections.proxyShort', direct: 'connections.directLabel', block: 'connections.blockShort' };
// 草稿:选择只改草稿;「生成规则实现方案」按钮才把草稿保存为规则。
const draft = ref<Record<string, AppPolicy>>({});
const savedOf = (app: string): AppPolicy => appPolicies.value[app] ?? 'inherit';
const dirty = computed(() => draft.value[selectedApp.value] !== undefined && draft.value[selectedApp.value] !== savedOf(selectedApp.value));
function pick(app: string, policy: AppPolicy) { draft.value = { ...draft.value, [app]: policy }; }
const filterOptions = computed(() => ([
  { value: 'all', label: t('connections.filterAll') },
  { value: 'configured', label: t('connections.filterConfigured') },
  { value: 'proxy', label: t('connections.proxyLabel') },
  { value: 'direct', label: t('connections.directLabel') },
  { value: 'block', label: t('connections.blockLabel') },
] as { value: typeof filter.value; label: string }[]));
const policyOf = (app: string): AppPolicy => draft.value[app] ?? savedOf(app);
const policyShort = (app: string) => t(policyShortKey[policyOf(app)]);
const exeOf = (path: string) => path.split(/[\\/]/).pop() || '';
function bytes(value: number) { const units = ['B','KB','MB','GB','TB']; let v = Math.max(0,value || 0), i = 0; while(v >= 1024 && i < units.length-1) { v /= 1024; i++; } return `${v.toFixed(i ? 1 : 0)} ${units[i]}`; }
const groups = computed<AppGroup[]>(() => {
  const map = new Map<string, AppGroup>();
  for (const row of snapshot.value.active) {
    const app = appNameFromPath(row.metadata.processPath);
    let group = map.get(app);
    if (!group) { group = { app, exe: exeOf(processExecutablePath(row.metadata.processPath)), rows: [], download: 0, upload: 0, downloadSpeed: 0, uploadSpeed: 0 }; map.set(app, group); }
    group.rows.push(row);
    group.download += row.download; group.upload += row.upload; group.downloadSpeed += row.downloadSpeed; group.uploadSpeed += row.uploadSpeed;
  }
  return [...map.values()].sort((a, b) => (b.downloadSpeed - a.downloadSpeed) || (b.download - a.download) || a.app.localeCompare(b.app));
});
const filteredGroups = computed(() => groups.value.filter(group => (filter.value === 'all' || (filter.value === 'configured' ? policyOf(group.app) !== 'inherit' : policyOf(group.app) === filter.value)) && search.value.matches(`${group.app || t('connections.unknownApp')} ${group.exe}`)));
const activeApp = computed(() => groups.value.find(group => group.app === selectedApp.value) || selectedGroup.value);
const totalConns = computed(() => groups.value.reduce((sum, group) => sum + group.rows.length, 0));
const configuredCount = computed(() => Object.keys(appPolicies.value).length);
async function save() {
  if (generating.value || !dirty.value) return;
  generating.value = true;
  applyMsg.value = '';
  try { const app = selectedApp.value; const policy = draft.value[app]; if (!app || !policy) return; await setAppPolicy(app, policy); delete draft.value[app]; applyTone.value = 'ok'; applyMsg.value = 'connections.applied'; }
  catch (e) { applyTone.value = 'bad'; applyMsg.value = 'connections.applyFailed'; error.value = e instanceof Error ? e.message : String(e); }
  finally { generating.value = false; }
}
let stopped = false;
let timer: ReturnType<typeof setTimeout> | undefined;
async function refresh() {
  try { const result = await invoke<Snapshot>('list_connections'); if(!stopped) { snapshot.value = result; loadError.value = ''; } }
  catch { if(!stopped) loadError.value = t('connections.unableToLoadConnectionsMake'); }
}
async function poll() { if(stopped) return; if(!document.hidden) await refresh(); if(!stopped) timer = setTimeout(poll,1500); }
onMounted(() => { window.addEventListener('resize', dismissPopovers); if('__TAURI_INTERNALS__' in window) void poll(); else error.value = t('connections.connectionMonitoringIsAvailableIn'); });
onUnmounted(() => { stopped = true; clearTimeout(timer); window.removeEventListener('resize', dismissPopovers); });
</script>
<style scoped>
.pnm-page { flex:1; min-width:0; min-height:0; display:flex; flex-direction:column; gap:8px; overflow:hidden; }
.pnm-overview { background:var(--bg); border:1px solid var(--line); border-radius:12px; padding:8px 12px; }
.pnm-topbar { display:flex; align-items:center; gap:16px; }
.pnm-segment { display:flex; flex:none; align-items:stretch; height:32px; gap:3px; background:var(--seg-bg); border-radius:8px; padding:3px; }
.pnm-segment button { min-height:0; height:26px; padding:0 11px; border:0; background:transparent; border-radius:6px; font-size:13px; line-height:20px; white-space:nowrap; }
.pnm-segment button.on { background:var(--seg-active); color:var(--accent-text); font-weight:600; }
.pnm-filter { width:150px; min-width:120px; margin-left:auto; }
.pnm-filter :deep(.select-trigger) { min-height:32px; height:32px; font-size:13px; }
.pnm-stats { display:flex; align-items:center; gap:16px; margin:0; min-width:0; }
.pnm-stats > div { display:flex; align-items:baseline; gap:6px; white-space:nowrap; }
.pnm-stats dt { font-size:12px; color:var(--muted); }
.pnm-stats dd { margin:0; font-size:14px; font-weight:600; font-variant-numeric:tabular-nums; }
.pnm-error { margin:0; font-size:13px; color:var(--danger); }
.pnm-list { flex:1; min-height:0; display:flex; flex-direction:column; border:1px solid var(--line); border-radius:12px; overflow:hidden; background:var(--bg); }
.pnm-list-head { display:flex; align-items:center; gap:12px; padding:8px 10px; border-bottom:1px solid var(--line); }
.pnm-list-head > .pnm-status,.pnm-list-head > .pnm-list-label { margin-left:auto; }
.pnm-list-label { flex:none; font-size:13px; font-weight:600; }
.pnm-list-label span { color:var(--muted); font-weight:400; margin-left:6px; }
.pnm-search { display:flex; align-items:center; gap:8px; min-width:210px; flex:0 1 420px; width:420px; max-width:100%; height:32px; padding:0 5px 0 10px; border:1px solid var(--border); border-radius:8px; color:var(--muted); }
.pnm-search:focus-within { border-color:var(--accent); }
.pnm-search input { flex:1; width:0; min-width:0; padding:0; border:0; background:transparent; color:var(--text); font-size:13px; }
.pnm-search button { padding:0; min-height:24px; width:24px; border:0; background:transparent; }
.pnm-search-modes { display:flex; flex:none; gap:1px; }
.pnm-search-modes button { color:var(--muted); border-radius:4px; }
.pnm-search-modes button.on { color:var(--accent-text); background:var(--selected); }
.pnm-search.invalid { border-color:var(--danger); }
.pnm-search input::placeholder { color:var(--muted); }
.pnm-table-scroll { flex:1; min-height:0; overflow:auto; }
.pnm-table { width:100%; table-layout:fixed; border-collapse:collapse; font-size:13px; }
.pnm-table th { position:sticky; top:0; z-index:1; background:var(--surface); color:var(--muted); font-size:12px; font-weight:500; }
.pnm-table th,.pnm-table td { text-align:left; height:46px; padding:8px 12px; border-bottom:1px solid var(--line); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.pnm-table th { height:36px; }
.pnm-table tbody tr:hover { background:var(--hover); }
.pnm-table .number { text-align:right; font-variant-numeric:tabular-nums; }
.pnm-table .policy-cell { text-align:right; }
.pnm-connection-table { min-width:720px; }
.pnm-app-row { cursor:pointer; }
.pnm-app-identity { display:flex; align-items:center; justify-content:flex-start; gap:10px; min-width:0; width:100%; min-height:28px; padding:0; border:0; border-radius:4px; background:transparent; text-align:left; color:var(--text); }
.pnm-app-identity:hover:not(:disabled) { background:transparent; color:var(--accent-text); }
.pnm-app-scope { min-width:0; max-width:160px; min-height:28px; padding:4px 7px; font-size:12px; background:var(--selected); color:var(--accent-text); border:0; }
.pnm-app-scope > span { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.pnm-app-scope svg { flex:none; }
.pnm-app-badge { display:grid; place-items:center; flex:none; width:28px; height:28px; border-radius:7px; background:var(--surface); color:var(--accent-text); font-size:13px; font-weight:600; }
.pnm-app-text { display:flex; flex-direction:column; min-width:0; }
.pnm-app-text strong,.pnm-app-text small { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.pnm-app-text strong { font-weight:500; }
.pnm-app-text small,.pnm-app-exe { color:var(--muted); font-size:11px; }
.pnm-policy-trigger { min-height:28px; padding:3px 6px; border:0; background:transparent; font-size:12px; }
.pnm-close-all { margin-left:auto; min-height:32px; padding:4px 10px; font-size:12px; white-space:nowrap; }
.pnm-popover { position:fixed; inset:auto; margin:0; width:min(340px,calc(100vw - 24px)); max-height:calc(100vh - 24px); overflow:auto; padding:16px; border:0; border-radius:12px; background:var(--bg); color:var(--text); box-shadow:var(--shadow-hover); }
.pnm-popover::backdrop { background:transparent; }
.pnm-confirm p { color:var(--muted); font-size:13px; }
.pnm-confirm > div { display:flex; justify-content:flex-end; gap:8px; }
.pnm-detail-head { min-width:0; }
.pnm-detail-head > div { min-width:0; }
.pnm-detail-head h3,.pnm-app-exe { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; display:block; }
.pnm-empty { padding:64px 16px; display:flex; flex-direction:column; align-items:center; gap:12px; color:var(--muted); font-size:13px; }
.pnm-dialog-head { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:12px; }
.pnm-dialog-close { flex:none; width:26px; height:26px; min-height:0; padding:0; border:0; background:transparent; color:var(--muted); border-radius:6px; }
.pnm-dialog-close:hover { color:var(--text); background:var(--hover); }
.pnm-detail-head { display:flex; align-items:center; gap:9px; }
.pnm-detail-head h3 { margin:0; font-size:15px; font-weight:600; }
.pnm-detail-stats { display:flex; align-items:center; gap:20px; margin:0 0 14px; padding-bottom:12px; border-bottom:1px solid var(--line); }
.pnm-detail-stats > div { display:flex; align-items:baseline; gap:6px; }
.pnm-detail-stats dt { color:var(--muted); font-size:11px; }
.pnm-detail-stats dd { margin:0; font-size:12px; font-weight:600; font-variant-numeric:tabular-nums; }
.pnm-popover h4 { margin:0 0 6px; font-size:12px; color:var(--muted); font-weight:500; }
.pnm-policy-notice { display:flex; align-items:flex-start; gap:6px; margin:0 0 8px; color:var(--muted); font-size:11px; line-height:1.5; }
.pnm-policy-notice svg { flex:none; margin-top:1px; }
.pnm-policies { display:flex; flex-direction:column; gap:2px; }
.pnm-policy { display:flex; align-items:center; gap:10px; width:100%; min-height:52px; padding:8px; border:0; border-radius:7px; background:transparent; text-align:left; }
.pnm-policy:hover:not(:disabled) { background:var(--hover); }
.pnm-policy.on { background:var(--selected); }
.pnm-policy:disabled { opacity:1; cursor:not-allowed; }
.pnm-policy-icon { flex:none; color:var(--muted); }
.pnm-policy.on .pnm-policy-icon,.pnm-policy.on .pnm-policy-label { color:var(--accent-text); }
.pnm-policy-text { display:flex; flex-direction:column; gap:2px; min-width:0; flex:1; }
.pnm-policy-label { font-size:13px; font-weight:500; color:var(--text); }
.pnm-policy-desc { font-size:11px; color:var(--muted); line-height:1.4; }
.pnm-policy-check { flex:none; width:16px; color:var(--accent-text); }
.pnm-policy-footer { display:flex; align-items:center; justify-content:flex-end; gap:10px; margin-top:12px; padding-top:12px; border-top:1px solid var(--line); }
.pnm-policy-timing { flex:1; color:var(--muted); font-size:11px; }
.pnm-generate,.pnm-dismiss { flex:none; min-height:30px; padding:5px 10px; border-radius:7px; font-size:12px; line-height:18px; }
.pnm-generate { background:var(--accent); color:#fff; border:0; font-weight:500; }
.pnm-generate:hover:not(:disabled) { background:var(--primary-hover); color:#fff; }
.pnm-generate:disabled { opacity:1; background:var(--seg-bg); color:var(--muted); }
.pnm-apply { margin:10px 0 0; font-size:12px; line-height:1.5; }
.pnm-apply.ok { color:var(--success); }
.pnm-apply.bad { color:var(--danger); }

@container connections (max-width:680px) {
  .pnm-search { flex-basis:240px; }
  .pnm-app-scope { max-width:120px; }
  .pnm-topbar { flex-wrap:wrap; gap:8px 12px; }
  .pnm-stats { order:3; flex-basis:100%; gap:18px; }
}
.pnm-page { container-type:inline-size; container-name:connections; }
</style>
