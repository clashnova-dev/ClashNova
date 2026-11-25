<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, useId, watch } from 'vue';
import { CaseSensitive, WholeWord, Regex, Check, ChevronsDown, Copy, Download, Pause, Play, RefreshCw, Search, X } from '@lucide/vue';
import { useApp } from '../composables/useApp';
import SelectMenu from '../components/ui/SelectMenu.vue';
import HelpTip from '../components/ui/HelpTip.vue';
import { filterLogs, logEntries, logRange, redactLog, type LogEntry } from '../lib/log-viewer';
import { connectionSearch, type SearchOptions } from '../lib/connection-search';

const { logViewport, logs, activityLogs, renderLogLine, followLogs, scrollLogs, onLogScroll, refreshLogs, logsLoading, logsError, t, tx } = useApp();
const query = ref('');
const searchOptions = ref<SearchOptions>({ caseSensitive: false, wholeWord: false, regex: false });
const search = computed(() => connectionSearch(query.value, searchOptions.value));
const searchErrorId = useId();
const searchModes = [
  { key: 'caseSensitive', icon: CaseSensitive, label: 'connections.caseSensitive' },
  { key: 'wholeWord', icon: WholeWord, label: 'connections.wholeWord' },
  { key: 'regex', icon: Regex, label: 'connections.regex' },
] as const;
const level = ref('all');
const source = ref('all');
const paused = ref(false);
const frozen = ref<LogEntry[]>([]);
const feedback = ref('');
const sharing = ref(false);
const scrollTop = ref(0);
const viewportHeight = ref(400);
const selected = ref<LogEntry>();
const detail = ref<HTMLElement>();
let observer: ResizeObserver | undefined;
const current = computed(() => logEntries(activityLogs.value, logs.value, renderLogLine));
const entries = computed(() => paused.value ? frozen.value : current.value);
const filtered = computed(() => filterLogs(entries.value, query.value, level.value, source.value, searchOptions.value));
// Estimate the monospace measure (wide characters count twice) without measuring every DOM row.
// One shared width keeps the last column and header stable while vertically virtualizing the list.
const messageWidth = computed(() => Math.max(640, ...filtered.value.map(row => row.message.replace(/[^\x00-\x7F]/g, 'xx').length * 8 + 16)));
const range = computed(() => logRange(filtered.value.length, Math.max(0, scrollTop.value - 34), Math.max(0, viewportHeight.value - 34)));
const visible = computed(() => filtered.value.slice(range.value.start, range.value.end));
const levelOptions = computed(() => [{ value: 'all', label: t('logs.allLevels') }, ...['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE'].map(value => ({ value, label: value }))]);
const sourceOptions = computed(() => [{ value: 'all', label: t('logs.allSources') }, { value: 'client', label: t('logs.client') }, { value: 'core', label: t('logs.core') }, { value: 'tun', label: 'TUN' }]);
const sourceLabel = (row: LogEntry) => row.source === 'tun' ? 'TUN' : t(row.source === 'client' ? 'logs.client' : 'logs.core');
function onScroll() {
  const next = logViewport.value?.scrollTop || 0;
  if (next !== scrollTop.value) onLogScroll();
  scrollTop.value = next;
}
async function bottom() {
  followLogs.value = true;
  await nextTick();
  if (logViewport.value) logViewport.value.scrollTop = logViewport.value.scrollHeight;
  onScroll();
}
function pause() { if (!paused.value) frozen.value = current.value; paused.value = !paused.value; feedback.value = ''; }
async function refresh() { try { await refreshLogs(); } catch { /* The inline error preserves the current view. */ } }
function reset() { query.value = ''; level.value = 'all'; source.value = 'all'; searchOptions.value = { caseSensitive: false, wholeWord: false, regex: false }; }
async function copy(row?: LogEntry) {
  if (sharing.value) return;
  sharing.value = true;
  try { await navigator.clipboard.writeText((row ? [row] : filtered.value).map(item => item.text).join('\n')); feedback.value = 'logs.copySuccess'; }
  catch { feedback.value = 'logs.copyFailed'; }
  finally { sharing.value = false; }
}
function download() {
  let url = '';
  try {
    url = URL.createObjectURL(new Blob([filtered.value.map(row => row.text).join('\n') + '\n'], { type: 'text/plain;charset=utf-8' }));
    const link = document.createElement('a');
    link.href = url; link.download = `ClashNova-${new Date().toISOString().replace(/[:.]/g, '-')}.log`;
    document.body.append(link); link.click(); link.remove(); feedback.value = 'logs.exportSuccess';
  } catch { feedback.value = 'logs.exportFailed'; }
  finally { if (url) setTimeout(() => URL.revokeObjectURL(url), 1000); }
}
async function inspect(row: LogEntry) { selected.value = row; feedback.value = ''; await nextTick(); detail.value?.showPopover(); }
watch(filtered, async () => {
  await nextTick();
  if (followLogs.value) await scrollLogs();
  else if (logViewport.value) logViewport.value.scrollTop = Math.min(logViewport.value.scrollTop, logViewport.value.scrollHeight);
  onScroll();
});
watch([query, level, source, searchOptions], async () => { feedback.value = ''; await nextTick(); if (logViewport.value) logViewport.value.scrollTop = 0; onScroll(); }, { deep: true });
onMounted(async () => {
  observer = new ResizeObserver(([entry]) => {
    viewportHeight.value = entry.contentRect.height;
    if (followLogs.value) void bottom();
  });
  if (logViewport.value) observer.observe(logViewport.value);
  await bottom();
});
onUnmounted(() => observer?.disconnect());
</script>
<template>
  <section class="log-viewer" :aria-label="t('logs.activityLogs')">
    <div class="log-toolbar">
      <div class="log-search" :class="{ invalid: search.error }"><Search :size="16" /><input v-model="query" :aria-label="t('logs.search')" :placeholder="t('logs.search')" :aria-invalid="search.error" :aria-describedby="search.error ? searchErrorId : undefined" spellcheck="false" /><button v-if="query" type="button" :aria-label="t('logs.clearSearch')" @click="query = ''"><X :size="14" /></button><div class="log-search-modes"><button v-for="mode in searchModes" :key="mode.key" type="button" :class="{ selected: searchOptions[mode.key] }" :aria-label="t(mode.label)" :title="t(mode.label)" :aria-pressed="searchOptions[mode.key]" @click="searchOptions[mode.key] = !searchOptions[mode.key]"><component :is="mode.icon" :size="16" /></button></div></div>
      <div class="log-filters"><SelectMenu v-model="level" :label="t('logs.level')" :options="levelOptions" /><SelectMenu v-model="source" :label="t('logs.source')" :options="sourceOptions" /></div>
      <div class="log-actions">
        <button type="button" :class="{ selected: paused }" :aria-label="t(paused ? 'logs.resume' : 'logs.pause')" :title="t(paused ? 'logs.resume' : 'logs.pause')" :aria-pressed="paused" @click="pause"><component :is="paused ? Play : Pause" :size="16" /></button>
        <button type="button" :disabled="logsLoading" :aria-label="t('logs.refresh')" :title="t('logs.refresh')" @click="refresh"><RefreshCw :size="16" :class="{ spin: logsLoading }" /></button>
        <span class="log-divider" />
        <button type="button" :disabled="!filtered.length || sharing" :aria-label="t('logs.copy')" :title="t('logs.copy')" @click="copy()"><Copy :size="16" /></button>
        <button type="button" :disabled="!filtered.length" :aria-label="t('logs.export')" :title="t('logs.export')" @click="download"><Download :size="16" /></button>
      </div>
    </div>
    <div v-if="logsError" class="log-read-error" role="alert"><span>{{ t('logs.readFailed') }} · {{ redactLog(tx(logsError)) }}</span><button type="button" :disabled="logsLoading" @click="refresh">{{ t('logs.retry') }}</button></div>
    <div v-if="search.error" :id="searchErrorId" class="log-read-error" role="alert"><span>{{ t('connections.invalidRegex') }}</span><button type="button" @click="searchOptions.regex = false"><X :size="14" />{{ t('connections.regex') }}</button></div>
    <div ref="logViewport" class="log-body" :aria-label="t('logs.activityLogs')" tabindex="0" @scroll.passive="onScroll">
      <div v-if="filtered.length" class="log-table" :style="{ '--log-message-width': `${messageWidth}px` }">
        <div class="log-columns" aria-hidden="true"><span>{{ t('logs.time') }}</span><span>{{ t('logs.level') }}</span><span>{{ t('logs.message') }}</span><span>{{ t('logs.source') }}</span></div>
        <div :style="{ height: `${range.before}px` }" aria-hidden="true" />
        <button v-for="row in visible" :key="row.id" type="button" class="log-row" :class="row.level.toLowerCase()" :title="row.text" @click="inspect(row)">
          <time :datetime="row.time.replace(' ', 'T')">{{ row.time || '—' }}</time><span class="log-level">{{ row.level }}</span><span class="log-message">{{ row.message }}</span><span class="log-source">{{ sourceLabel(row) }}</span>
        </button>
        <div :style="{ height: `${range.after}px` }" aria-hidden="true" />
      </div>
      <div v-else class="log-empty"><Search v-if="entries.length" :size="25" /><span>{{ t(entries.length ? 'logs.filteredEmpty' : 'logs.empty') }}</span><button v-if="entries.length" type="button" @click="reset">{{ t('logs.resetFilters') }}</button></div>
    </div>
    <footer class="log-footer">
      <div class="log-state"><span class="log-state-dot" :class="{ paused, failed: !!logsError }" /><span>{{ t(paused ? 'logs.paused' : logsError ? 'logs.readFailed' : 'logs.live') }}</span><span class="log-count">{{ t('logs.shown', { count: filtered.length, total: entries.length }) }}</span><HelpTip :text="t('logs.windowHint')" /></div>
      <span v-if="feedback" class="log-feedback" role="status">{{ tx(feedback) }}</span>
      <div class="log-follow"><button type="button" :aria-pressed="followLogs" :class="{ selected: followLogs }" @click="followLogs ? followLogs = false : bottom()"><Check v-if="followLogs" :size="14" />{{ t('logs.followLatest') }}</button><button type="button" :aria-label="t('logs.scrollBottom')" :title="t('logs.scrollBottom')" @click="bottom"><ChevronsDown :size="16" /></button></div>
    </footer>
    <div ref="detail" popover="auto" class="log-detail" role="dialog" :aria-label="t('logs.detail')">
      <template v-if="selected"><div class="log-detail-head"><strong>{{ t('logs.detail') }}</strong><button type="button" :aria-label="t('common.close')" @click="detail?.hidePopover()"><X :size="17" /></button></div><div class="log-detail-meta"><time>{{ selected.time || '—' }}</time><span>{{ selected.level }}</span><span>{{ sourceLabel(selected) }}</span></div><pre>{{ selected.message }}</pre><div class="log-detail-foot"><small role="status">{{ feedback ? tx(feedback) : t('logs.exportHint') }}</small><button type="button" :disabled="sharing" @click="copy(selected)"><Copy :size="15" />{{ t('logs.copyLine') }}</button></div></template>
    </div>
  </section>
</template>
<style scoped>
.log-viewer{display:flex;flex-direction:column;flex:1 1 0;min-height:0;min-width:0;border:1px solid var(--line);border-radius:14px;background:var(--bg);overflow:hidden}
.log-toolbar{display:flex;align-items:center;flex-wrap:wrap;gap:10px;padding:12px;border-bottom:1px solid var(--line)}
.log-search{display:flex;align-items:center;gap:8px;flex:1 1 300px;max-width:460px;min-width:220px;height:34px;padding:0 7px 0 10px;border:1px solid var(--border);border-radius:8px;color:var(--muted);background:var(--bg)}
.log-search:focus-within{border-color:var(--accent-text)}
.log-search.invalid{border-color:var(--danger)}
.log-search-modes{display:flex;align-items:center;gap:1px;flex:none;padding-inline-start:4px;border-inline-start:1px solid var(--line)}
.log-search .log-search-modes button{width:25px;height:26px;min-height:26px;padding:0;border-radius:4px}
.log-search-modes button.selected{background:var(--selected);color:var(--accent-text)}
.log-search input{width:100%;min-width:0;border:0;outline:0;background:transparent;color:var(--text);font:inherit;font-size:13px;padding:0;caret-color:var(--accent-text)}
.log-search input::placeholder{color:var(--muted)}
.log-search>svg{flex:none}
.log-filters{display:flex;gap:8px;margin-inline-start:auto}
.log-filters :deep(.select-menu){min-width:110px}
.log-filters :deep(.select-trigger){height:34px;min-height:34px;border-radius:8px;font-size:13px}
.log-actions{display:flex;align-items:center;gap:3px;margin-inline-start:auto}
.log-actions button,.log-search button,.log-follow button,.log-detail-head button{height:32px;min-height:32px;padding:0 8px;border:0;background:transparent;color:var(--muted);border-radius:6px;gap:5px}
.log-actions button:hover:not(:disabled),.log-follow button:hover:not(:disabled){background:var(--hover);color:var(--text)}
.log-actions button.selected,.log-follow button.selected{color:var(--accent-text);background:var(--selected)}
.log-divider{height:17px;width:1px;background:var(--line);margin:0 4px}
.log-table{width:100%;min-width:calc(var(--log-message-width) + 400px)}
.log-columns,.log-row{display:grid;grid-template-columns:190px 64px minmax(var(--log-message-width),1fr) 88px;gap:10px;align-items:center;padding-inline:14px}
.log-columns{position:sticky;top:0;z-index:1;height:34px;min-height:34px;background:var(--surface);border-bottom:1px solid var(--line);color:var(--muted);font-size:12px}
.log-body{flex:1;min-height:0;overflow:auto;overflow-anchor:none;overscroll-behavior:contain;scrollbar-width:thin;scrollbar-color:var(--scrollbar) transparent;outline-offset:-2px}
.log-row{height:28px;min-height:28px;width:100%;padding-block:0;border:0;border-radius:0;background:transparent;color:var(--text);font:13px/28px ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;text-align:start;font-weight:400;justify-content:normal;cursor:pointer}
.log-row:hover{background:var(--hover);color:var(--text)}
.log-row:focus-visible{outline:2px solid var(--focus);outline-offset:-2px}
.log-row time{color:var(--muted);font-size:12px;font-variant-numeric:tabular-nums;white-space:nowrap}
.log-level{font-size:11px;font-weight:650}
.log-source{font-family:inherit;font-size:12px;color:var(--muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.log-message{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;-webkit-user-select:text;user-select:text}
.log-row.error .log-level{color:var(--danger)}
.log-row.warn .log-level{color:var(--warn)}
.log-row.debug .log-level,.log-row.trace .log-level{color:var(--muted)}
.log-empty{display:flex;min-height:100%;flex-direction:column;align-items:center;justify-content:center;gap:12px;color:var(--muted);font-size:14px;padding:25px}
.log-empty button{height:32px;min-height:32px;font-size:13px}
.log-footer{display:flex;align-items:center;flex-wrap:wrap;gap:8px;padding:6px 12px;min-height:43px;border-top:1px solid var(--line);font-size:12px;color:var(--muted)}
.log-state,.log-follow{display:flex;align-items:center;gap:7px}
.log-state-dot{width:6px;height:6px;border-radius:50%;background:var(--success)}
.log-state-dot.paused{background:var(--warn)}
.log-state-dot.failed{background:var(--danger)}
.log-count{margin-inline-start:5px;font-variant-numeric:tabular-nums}
.log-follow{margin-inline-start:auto;gap:3px}
.log-follow button{font-size:12px}
.log-feedback{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.log-read-error{display:flex;align-items:center;gap:10px;background:var(--danger-bg);color:var(--danger);padding:8px 12px;font-size:12px}
.log-read-error span{flex:1;min-width:0;overflow-wrap:anywhere}
.log-read-error button{height:28px;min-height:28px;padding:0 10px;font-size:12px}
.log-detail{position:fixed;inset:0;margin:auto;width:min(680px,calc(100vw - 48px));max-height:calc(100vh - 70px);overflow:auto;padding:18px;border:1px solid var(--border);border-radius:14px;background:var(--bg);color:var(--text);box-shadow:var(--shadow-hover)}
.log-detail::backdrop{background:transparent}
.log-detail-head,.log-detail-foot,.log-detail-meta{display:flex;align-items:center;justify-content:space-between;gap:10px}
.log-detail-meta{justify-content:flex-start;margin-top:8px;font-size:12px;color:var(--muted);font-variant-numeric:tabular-nums}
.log-detail pre{max-height:45vh;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;margin:16px 0;font:13px/1.7 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;-webkit-user-select:text;user-select:text}
.log-detail-foot{padding-top:12px;border-top:1px solid var(--line)}
.log-detail-foot small{color:var(--muted);font-size:12px}
.log-detail-foot button{height:32px;min-height:32px;padding:0 10px;font-size:12px;gap:6px;flex:none}
@media(max-width:950px){.log-search{max-width:none;flex-basis:260px}.log-filters{margin-inline-start:auto}.log-toolbar{gap:8px}.log-filters :deep(.select-menu){min-width:100px}}
</style>
