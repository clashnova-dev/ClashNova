<script setup lang="ts">
import { computed, ref } from 'vue';
import { X } from '@lucide/vue';
import { translator, type Locale, type MessageKey } from '../i18n';
import { connectionEndpoint, type ConnectionRow } from '../lib/connection-details';
const props = defineProps<{ row: ConnectionRow; active: boolean; locale: Locale; chain: string }>();
const emit = defineEmits<{ close: [] }>();
const { t } = translator(() => props.locale);
const tab = ref<'general' | 'raw'>('general');
const host = computed(() => connectionEndpoint(props.row.metadata.host || props.row.metadata.destinationIP, props.row.metadata.destinationPort));
function bytes(value: number) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']; let v = Math.max(0, value || 0), i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v.toFixed(i ? 1 : 0)} ${units[i]}`;
}
const started = computed(() => {
  const value = new Date(props.row.start);
  return Number.isNaN(value.getTime()) ? props.row.start || '—' : value.toLocaleString(props.locale, {
    year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false,
  });
});
const sections = computed<{ title: MessageKey; fields: [MessageKey, string][] }[]>(() => [
  { title: 'connections.detailGeneral', fields: [
    ['connections.detailStatus', t(props.active ? 'connections.active' : 'connections.closed')],
    ['connections.host', host.value],
    ['connections.detailNetwork', props.row.metadata.network?.toUpperCase() || '—'],
    ['connections.detailInbound', props.row.metadata.type || '—'],
    ['connections.detailSource', connectionEndpoint(props.row.metadata.sourceIP || '', props.row.metadata.sourcePort)],
    ['connections.detailDestination', connectionEndpoint(props.row.metadata.destinationIP, props.row.metadata.destinationPort)],
    ['connections.detailStarted', started.value],
    ['connections.detailProcess', props.row.metadata.processPath || '—'],
    ['connections.detailId', props.row.id],
  ] },
  { title: 'connections.detailRouting', fields: [
    ['connections.chain', props.chain],
    ['connections.detailRule', props.row.rule || '—'],
    ['connections.detailRulePayload', props.row.rulePayload || '—'],
  ] },
  { title: 'connections.totalTraffic', fields: [
    ['connections.downloaded', bytes(props.row.download)],
    ['connections.uploaded', bytes(props.row.upload)],
    ['connections.downloadSpeed', `${bytes(props.active ? props.row.downloadSpeed : 0)}/s`],
    ['connections.uploadSpeed', `${bytes(props.active ? props.row.uploadSpeed : 0)}/s`],
  ] },
]);
const raw = computed(() => JSON.stringify(props.row, null, 2));
</script>
<template>
  <aside class="connection-inspector" :aria-label="t('connections.detailTitle')" @keydown.esc.stop.prevent="emit('close')">
    <header class="inspector-header"><strong :title="host">{{ host }}</strong><button type="button" :aria-label="t('common.close')" @click="emit('close')"><X :size="16" /></button></header>
    <div class="inspector-tabs">
      <button type="button" :aria-pressed="tab === 'general'" :class="{ on: tab === 'general' }" @click="tab = 'general'">{{ t('connections.detailGeneral') }}</button>
      <button type="button" :aria-pressed="tab === 'raw'" :class="{ on: tab === 'raw' }" @click="tab = 'raw'">{{ t('connections.detailRaw') }}</button>
    </div>
    <div class="inspector-body">
      <template v-if="tab === 'general'">
        <section v-for="section in sections" :key="section.title" class="inspector-section">
          <h4>{{ t(section.title) }}</h4>
          <dl><div v-for="[label, value] in section.fields" :key="label"><dt>{{ t(label) }}</dt><dd>{{ value }}</dd></div></dl>
        </section>
      </template>
      <pre v-else class="inspector-raw">{{ raw }}</pre>
    </div>
  </aside>
</template>
<style scoped>
.connection-inspector { flex:0 0 38%; min-width:300px; max-width:420px; min-height:0; display:flex; flex-direction:column; border-left:1px solid var(--line); background:var(--bg); }
.inspector-header { display:flex; align-items:center; gap:10px; min-height:36px; padding:6px 12px; border-bottom:1px solid var(--line); }
.inspector-header strong { min-width:0; flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:12px; font-weight:600; }
.inspector-header button { width:24px; height:24px; min-height:0; padding:0; border:0; background:transparent; color:var(--muted); }
.inspector-tabs { display:flex; flex:none; gap:14px; padding:0 12px; border-bottom:1px solid var(--line); }
.inspector-tabs button { min-height:32px; padding:5px 0; border:0; border-bottom:2px solid transparent; border-radius:0; background:transparent; font-size:12px; color:var(--muted); }
.inspector-tabs button.on { border-bottom-color:var(--accent); color:var(--accent-text); }
.connection-inspector button:focus-visible { outline:2px solid var(--accent); outline-offset:-2px; }
.inspector-body { flex:1; min-height:0; overflow:auto; user-select:text; }
.inspector-section { padding:12px; border-bottom:1px solid var(--line); }
.inspector-section h4 { margin:0 0 10px; font-size:12px; font-weight:600; }
.inspector-section dl { margin:0; display:flex; flex-direction:column; gap:9px; }
.inspector-section dl > div { display:grid; grid-template-columns:95px minmax(0,1fr); gap:10px; font-size:12px; line-height:1.5; }
.inspector-section dt { color:var(--muted); overflow-wrap:anywhere; }
.inspector-section dd { margin:0; white-space:pre-wrap; overflow-wrap:anywhere; font-variant-numeric:tabular-nums; }
.inspector-raw { margin:0; padding:12px; font-size:11px; line-height:1.6; white-space:pre; }
@container connections (max-width:820px) {
  .connection-inspector { flex:0 1 48%; min-width:0; max-width:none; width:100%; min-height:150px; border-left:0; border-top:1px solid var(--line); }
}
</style>
