<template>
  <section class="ip-card" :aria-label="t('exit.exitInfo')">
    <header><h2>{{ t('exit.exitInfo') }}</h2><span class="ip-route">{{ t(connected ? 'exit.proxyExit' : 'exit.directExit') }}</span><button class="icon-button" :disabled="!data?.ip" :aria-label="t(visible ? 'exit.hideIp' : 'exit.showIp')" :title="t(visible ? 'exit.hideIp' : 'exit.showIp')" @click="visible = !visible"><EyeOff v-if="visible" :size="15" /><Eye v-else :size="15" /></button><button class="icon-button" :aria-label="t('exit.refreshExitIp')" :title="t('exit.refreshExitIp')" :disabled="loading" @click="refresh"><RefreshCw :size="15" :class="{ spinning:loading }" /></button></header>
    <strong class="ip-address" :title="data ? `${visible && data.ip ? data.ip + ' · ' : ''}${data.source} · ${new Date(data.checkedAt).toLocaleString(locale)}` : undefined">{{ data?.ip ? visible ? data.ip : '••••••••' : '—' }}</strong>
    <dl class="ip-details"><template v-for="[label,value] in details" :key="label"><dt>{{ tx(label) }}</dt><dd :title="value || t('common.unknown')">{{ value || t('common.unknown') }}</dd></template></dl>
    <p v-if="error" class="ip-message error" role="status">{{ tx(error) }}</p><p v-else-if="data && !data.geo" class="ip-message">{{ t('exit.exitIpIsAvailableLocation') }}</p>
    <footer><label class="ip-auto"><button class="toggle-switch" :class="{ enabled:autoRefresh }" role="switch" :aria-checked="autoRefresh" :aria-label="t('exit.autoRefreshIp')" @click="autoRefresh = !autoRefresh"><span /></button><span>{{ t('exit.autoRefresh') }}</span></label><small>300s</small></footer>
  </section>
</template>
<script setup lang="ts">
import { computed,onMounted,onUnmounted,ref,watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { RefreshCw,Eye,EyeOff } from '@lucide/vue';
import { translator, type Locale } from '../i18n';
const props=defineProps<{locale:Locale;connected:boolean;networkKey:string}>();
const { t, tx } = translator(() => props.locale);
type Info={ip:string;country:string;viaProxy:boolean;checkedAt:string;source:string;geo?:{asn?:number;isp:string;organization:string;region:string;city:string;timezone:string;latitude?:number;longitude?:number}|null};
const data=ref<Info|null>(null),loading=ref(false),error=ref('');
function preference(key:string,fallback:boolean) { try{const value=localStorage.getItem(key);return value===null?fallback:value==='true';}catch{return fallback;} }
const visible=ref(preference('airport.ip.visible',false)),autoRefresh=ref(preference('airport.ip.autoRefresh',true));
const country=computed(()=>{if(!data.value?.country)return '';try{return new Intl.DisplayNames([props.locale],{type:'region'}).of(data.value.country)||data.value.country;}catch{return data.value.country;}});
const location=computed(()=>[...new Set([country.value,data.value?.geo?.region,data.value?.geo?.city].filter(Boolean))].join(' · '));
const provider=computed(()=>[data.value?.geo?.isp,data.value?.geo?.asn?`AS${data.value.geo.asn}`:''].filter(Boolean).join(' · '));
const details=computed(()=>[['exit.location',location.value],['exit.provider',provider.value],['exit.timezone',data.value?.geo?.timezone]] as [string,string|undefined][]);
let generation=0,stopped=false,timer:ReturnType<typeof setTimeout>|undefined;
function schedule(){clearTimeout(timer);if(autoRefresh.value&&!stopped)timer=setTimeout(refresh,300000);}
watch([visible,autoRefresh],()=>{try{localStorage.setItem('airport.ip.visible',String(visible.value));localStorage.setItem('airport.ip.autoRefresh',String(autoRefresh.value));}catch{}schedule();});
async function refresh(){
 clearTimeout(timer);const run=++generation;loading.value=true;error.value='';
 if(!('__TAURI_INTERNALS__' in window)){error.value='exit.exitIpChecksAreAvailable';loading.value=false;return;}
 try{const value=await invoke<Info>('get_exit_info');if(run===generation&&!stopped&&value?.viaProxy===props.connected)data.value=value;}
 catch(e){if(run===generation&&!stopped){data.value=null;error.value=String(e);}}
 finally{if(run===generation&&!stopped){loading.value=false;schedule();}}
}
watch(()=>props.networkKey,()=>{data.value=null;void refresh();});
onMounted(refresh);onUnmounted(()=>{stopped=true;generation++;clearTimeout(timer);});
</script>
<style scoped>
.ip-card { min-width:0; min-height:0; height:100%; overflow:hidden; display:flex; flex-direction:column; gap:4px; padding:4px 16px; border:1px solid var(--line); border-radius:16px; background:var(--bg); box-shadow:var(--shadow-card); }
.ip-card header { display:flex; align-items:center; gap:4px; }
.ip-card h2 { flex:none; margin:0; font-size:15px; font-weight:600; white-space:nowrap; }
.ip-route { flex:1; min-width:0; margin-left:4px; font-size:13px; color:var(--muted); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.ip-card header .icon-button { flex:none; width:26px; height:26px; min-height:26px; padding:0; border-color:transparent; border-radius:8px; background:transparent; color:var(--muted); }
.ip-address { display:block; min-width:0; max-width:100%; font-size:16px; line-height:20px; font-weight:650; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-variant-numeric:tabular-nums; }
.ip-details { display:grid; grid-template-columns:auto minmax(0,1fr); gap:2px 12px; margin:0; font-size:14px; line-height:18px; }
.ip-details dt { color:var(--muted); white-space:nowrap; }
.ip-details dd { display:-webkit-box; -webkit-box-orient:vertical; -webkit-line-clamp:2; margin:0; overflow:hidden; overflow-wrap:anywhere; }
.ip-card footer { display:flex; align-items:center; justify-content:space-between; gap:8px; margin-top:auto; color:var(--muted); font-size:13px; }
.ip-auto { display:flex; align-items:center; gap:8px; min-width:0; white-space:nowrap; }
.ip-auto > span { min-width:0; overflow:hidden; text-overflow:ellipsis; }
.ip-card footer small { flex:none; font-variant-numeric:tabular-nums; }
.ip-auto .toggle-switch { transform:scale(.7); transform-origin:left center; margin-right:-10px; }
.ip-message { margin:0; color:var(--muted); font-size:13px; }
.ip-message.error { color:var(--danger); }
.spinning { animation:rotate 1s linear infinite; }
@keyframes rotate { to { transform:rotate(360deg); } }
@media(prefers-reduced-motion:reduce) { .spinning { animation:none; } }
</style>
