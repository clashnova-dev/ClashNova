<script setup lang="ts">
import {computed,onUnmounted,ref,watch} from 'vue';
import {Route,ChevronLeft,ChevronRight,ChevronsLeft,ChevronsRight} from '@lucide/vue';
import {useApp} from '../composables/useApp';
import SearchField from '../components/ui/SearchField.vue';
import SelectMenu from '../components/ui/SelectMenu.vue';
import {connectionSearch,type SearchOptions} from '../lib/connection-search';
import {appPolicyRules} from '../lib/app-policies';
import {ruleGroups,rulePage,type RuleSource,type RuleRow} from '../lib/rule-viewer';
const {t,tx,locale,rawProfile,buildProfileConfig,routeMode,appPolicies,connectionNodeNames,selectedNodeName,tab}=useApp();
const query=ref(''),appliedQuery=ref(''),source=ref<RuleSource|'all'>('all'),page=ref(1);
const options=ref<SearchOptions>({caseSensitive:false,wholeWord:false,regex:false});
let timer:ReturnType<typeof setTimeout>|undefined;
watch(query,value=>{clearTimeout(timer);timer=setTimeout(()=>{appliedQuery.value=value;page.value=1;},120);});
onUnmounted(()=>clearTimeout(timer));
watch([source,options],()=>{page.value=1;},{deep:true});
const data=computed(()=>{
 if(!rawProfile.value)return {groups:[],error:''};
 try{
  const config=JSON.parse(buildProfileConfig());
  return {groups:ruleGroups(config.route?.rules,config.route?.final,appPolicyRules(appPolicies.value).length,routeMode.value==='rule'?3:0),error:''};
 }catch(error){return {groups:[],error:String(error)};}
});
const sourceNames=computed(()=>({
 application:t('rules.application'),builtin:t('rules.builtin'),subscription:t('rules.subscription'),fallback:t('rules.fallback'),
}));
const sourceOptions=computed(()=>[{value:'all' as const,label:t('rules.allSources')},...(['application','builtin','subscription','fallback'] as const).map(value=>({value,label:sourceNames.value[value]}))]);
function target(value:string){return value==='select'?tx(selectedNodeName.value)||t('connections.selector'):value==='reject'?t('connections.blockShort'):connectionNodeNames.value[value]||value;}
const search=computed(()=>connectionSearch(appliedQuery.value,options.value));
const result=computed(()=>rulePage(data.value.groups,page.value,100,source.value,appliedQuery.value.trim()?row=>!search.value.error&&search.value.matches([row.kind,row.value,row.target,target(row.target),sourceNames.value[row.source]].join(' ')):undefined));
const total=computed(()=>data.value.groups.reduce((count,group)=>count+group.values.length,0));
watch(data,()=>{page.value=1;});
watch(()=>result.value.pages,value=>{if(page.value>value)page.value=value;});
function rowTitle(row:RuleRow){return t('rules.groupOrder',{order:row.order});}
</script>
<template>
 <section class="rules-page" :aria-label="t('nav.rules')">
  <div class="rules-summary"><span class="mode-label"><Route :size="16" />{{ t(routeMode==='rule'?'rules.ruleMode':'rules.globalMode') }}</span><span>{{ t('rules.summary',{groups:data.groups.length,entries:total}) }}</span></div>
  <p class="rules-note">{{ t('rules.previewHint') }}</p>
  <p v-if="data.error" class="form-error" role="alert">{{ tx(data.error) }}</p>
  <div class="rules-panel">
   <div class="rules-toolbar">
    <SearchField v-model="query" v-model:options="options" :locale="locale" :label="t('rules.search')" :placeholder="t('rules.search')" :invalid="search.error" />
    <SelectMenu v-model="source" class="source-select" :label="t('rules.source')" :options="sourceOptions" />
   </div>
   <p v-if="search.error" class="rules-search-error" role="alert">{{ t('connections.invalidRegex') }}</p>
   <div class="rules-scroll">
    <table v-if="rawProfile && !data.error && result.rows.length">
     <colgroup><col class="type-col" /><col /><col class="target-col" /><col class="source-col" /></colgroup>
     <thead><tr><th>{{ t('rules.type') }}</th><th>{{ t('rules.content') }}</th><th>{{ t('rules.target') }}</th><th>{{ t('rules.source') }}</th></tr></thead>
     <tbody><tr v-for="row in result.rows" :key="row.id">
      <td :title="rowTitle(row)">{{ row.kind }}</td><td :title="row.value">{{ row.value }}</td><td :title="target(row.target)">{{ target(row.target) }}</td><td :title="sourceNames[row.source]">{{ sourceNames[row.source] }}</td>
     </tr></tbody>
    </table>
    <div v-else class="rules-empty"><Route :size="25" /><p>{{ t(!rawProfile?'rules.needSubscription':data.error?'rules.invalidConfig':'rules.noMatches') }}</p><button v-if="!rawProfile" type="button" @click="tab='sub'">{{ t('nav.subscriptions') }}</button></div>
   </div>
   <footer class="rules-footer">
    <span role="status">{{ t('rules.resultCount',{count:result.total}) }}</span>
    <nav :aria-label="t('rules.pagination')">
     <button type="button" :disabled="page<=1" :aria-label="t('rules.firstPage')" :title="t('rules.firstPage')" @click="page=1"><ChevronsLeft :size="16" /></button>
     <button type="button" :disabled="page<=1" :aria-label="t('rules.previousPage')" :title="t('rules.previousPage')" @click="page--"><ChevronLeft :size="16" /></button>
     <span>{{ t('rules.pageOf',{page,pages:result.pages}) }}</span>
     <button type="button" :disabled="page>=result.pages" :aria-label="t('rules.nextPage')" :title="t('rules.nextPage')" @click="page++"><ChevronRight :size="16" /></button>
     <button type="button" :disabled="page>=result.pages" :aria-label="t('rules.lastPage')" :title="t('rules.lastPage')" @click="page=result.pages"><ChevronsRight :size="16" /></button>
    </nav>
   </footer>
  </div>
 </section>
</template>
<style scoped>
.rules-page{display:flex;flex-direction:column;flex:1;min-width:0;min-height:0;gap:10px}
.rules-summary{display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;color:var(--muted);font-size:12px;padding:0 3px}
.mode-label{display:inline-flex;align-items:center;gap:8px;color:var(--strong);font-size:13px;font-weight:600}
.rules-note{margin:0 3px;color:var(--muted);font-size:12px;line-height:1.6}
.rules-panel{display:flex;flex-direction:column;flex:1;min-height:0;min-width:0;border:1px solid var(--line);border-radius:14px;overflow:hidden;background:var(--bg)}
.rules-toolbar{display:flex;align-items:center;flex-wrap:wrap;gap:10px;padding:12px;border-bottom:1px solid var(--line);flex:none}
.source-select{margin-left:auto;width:160px;min-width:140px}
.source-select :deep(.select-trigger){min-height:32px;height:32px;font-size:12px}
.rules-search-error{margin:0;padding:8px 12px;color:var(--danger);font-size:12px}
.rules-scroll{flex:1;min-height:0;min-width:0;overflow:auto;scrollbar-width:thin;scrollbar-color:var(--scrollbar) transparent}
table{width:100%;min-width:570px;table-layout:fixed;border-collapse:separate;border-spacing:0;font-size:13px}
.type-col{width:145px}.target-col{width:130px}.source-col{width:110px}
th{position:sticky;top:0;z-index:1;background:var(--surface);color:var(--muted);font-size:12px;font-weight:500}
th,td{padding:10px 14px;text-align:left;border-bottom:1px solid var(--line);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
tbody tr:hover{background:var(--hover)}
.rules-empty{min-height:180px;height:100%;display:flex;align-items:center;justify-content:center;flex-direction:column;gap:8px;padding:24px;color:var(--muted);font-size:13px;text-align:center}
.rules-empty p{margin:0;max-width:50ch}.rules-empty button{min-height:32px;font-size:12px}
.rules-footer{display:flex;align-items:center;justify-content:space-between;gap:12px;flex-wrap:wrap;border-top:1px solid var(--line);padding:9px 12px;color:var(--muted);font-size:12px;flex:none}
.rules-footer nav{display:flex;align-items:center;gap:5px;font-variant-numeric:tabular-nums}
.rules-footer button{width:28px;min-height:28px;padding:0;border-color:transparent;background:transparent}
.rules-footer nav span{padding:0 6px}
@media(max-width:900px){.source-select{width:140px}.rules-toolbar :deep(.search-field){flex:1 1 280px}.rules-summary{font-size:11px}}
</style>
