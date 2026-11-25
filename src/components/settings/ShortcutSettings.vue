<template>
  <section class="shortcut-settings">
    <h2>{{ t('common.keyboardShortcuts') }}</h2>
    <div class="setting-row"><span>{{ t('shortcuts.enableGlobalShortcuts') }}<HelpTip :text="t('shortcuts.worksWhenTheWindowIs')" /></span><ToggleSwitch v-model="draft.enabled" :label="t('shortcuts.enableGlobalShortcuts')" :disabled="!ready || saving" /></div>
    <div v-for="item in fields" :key="item.key" class="setting-row"><label :for="`shortcut-${item.key}`">{{ tx(item.label) }}</label><div class="shortcut-input"><input :id="`shortcut-${item.key}`" readonly :value="recording === item.key ? t('shortcuts.pressAKeyCombination') : formatShortcut(draft[item.key],isMac)" :placeholder="t('shortcuts.notSet')" :aria-label="tx(item.label)" :disabled="!ready || saving" @focus="recording=item.key" @blur="recording=null" @keydown.stop="record($event,item.key)" /><button class="icon-button" :disabled="!ready || saving || !draft[item.key]" :aria-label="t('shortcuts.clearShortcut') + ' ' + tx(item.label)" @click="draft[item.key]=''"><X :size="15" /></button></div></div>
    <p v-if="error" class="shortcut-error" role="alert">{{ tx(error) }}</p>
    <div class="shortcut-actions"><small role="status">{{ tx(status) }}</small><button :disabled="!ready || saving || !dirty" @click="save">{{ t(saving ? 'shortcuts.saving' : 'shortcuts.saveShortcuts') }}</button></div>
  </section>
</template>
<script setup lang="ts">
import HelpTip from '../ui/HelpTip.vue';
import ToggleSwitch from '../ui/ToggleSwitch.vue';
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { X } from '@lucide/vue';
import { captureShortcut, formatShortcut } from '../../lib/shortcut-keys';
import { translator, type Locale } from '../../i18n';
const props=defineProps<{locale:Locale;isMac:boolean}>();
const { t, tx } = translator(() => props.locale);
type Config={enabled:boolean;showWindow:string;toggleConnection:string;toggleRoute:string};
type Key='showWindow'|'toggleConnection'|'toggleRoute';
type Status={config:Config;registered:boolean;error?:string};
const draft=ref<Config>({enabled:false,showWindow:'CommandOrControl+Shift+A',toggleConnection:'CommandOrControl+Shift+X',toggleRoute:'CommandOrControl+Shift+M'});
const saved=ref('');const ready=ref(false);const saving=ref(false);const error=ref('');const status=ref('');const recording=ref<Key|null>(null);
const fields: {key:Key;label:string}[]=[{key:'showWindow',label:'shortcuts.showHideWindow'},{key:'toggleConnection',label:'shortcuts.connectDisconnect'},{key:'toggleRoute',label:'shortcuts.switchRuleGlobalMode'}];
watch(recording,value=>{ if(ready.value) void invoke('set_shortcut_recording',{recording:!!value}).catch(e=>error.value=String(e)); });
onUnmounted(()=>{ if(ready.value) void invoke('set_shortcut_recording',{recording:false}); });
const dirty=computed(()=>saved.value!==JSON.stringify(draft.value));
function apply(result:Status) { draft.value={...result.config};saved.value=JSON.stringify(result.config);error.value=result.error||'';status.value=result.registered?'shortcuts.shortcutsAreActive':result.config.enabled?'shortcuts.shortcutsAreNotActive':'shortcuts.shortcutsAreDisabled'; }
function record(event:KeyboardEvent,key:Key) {
  if(event.code === "Tab") return;
  event.preventDefault();
  if(event.code==='Escape') { (event.target as HTMLInputElement).blur();return; }
  if(['Backspace','Delete'].includes(event.code)&&!event.ctrlKey&&!event.altKey&&!event.metaKey) { draft.value[key]='';return; }
  const value=captureShortcut(event);
  if(value) { draft.value[key]=value;error.value='';(event.target as HTMLInputElement).blur(); }
}
onMounted(async()=>{if(!('__TAURI_INTERNALS__' in window)){error.value='shortcuts.globalShortcutsAreAvailableIn';return;}try{apply(await invoke<Status>('load_shortcuts'));ready.value=true;}catch(e){error.value=String(e);} });
async function save(){saving.value=true;error.value='';try{apply(await invoke<Status>('save_shortcuts',{config:draft.value}));}catch(e){error.value=String(e);}finally{saving.value=false;}}
</script>
<style scoped>
.shortcut-settings { margin-top:0; }
.shortcut-input { display:flex; align-items:center; gap:4px; width:260px; max-width:55%; min-width:0; padding-inline-end:4px; border:1px solid var(--border); border-radius:8px; background:var(--bg); flex:none; }
.shortcut-input:focus-within { border-color:var(--accent-text); }
.shortcut-input input { flex:1; min-width:0; width:100%; height:34px; padding:0 10px; border:0; outline:0; border-radius:8px; color:var(--text); background:transparent; font-size:13px; cursor:pointer; }
.shortcut-input input:disabled { opacity:.6; cursor:not-allowed; }
.shortcut-input button { width:26px; min-width:26px; height:26px; min-height:26px; padding:0; border:0; background:transparent; color:var(--muted); }
.shortcut-actions { display:flex; gap:16px; align-items:center; justify-content:space-between; margin-top:14px; }
.shortcut-actions small { color:var(--muted); font-size:12px; }
.shortcut-error { color:var(--danger); font-size:12px; line-height:1.6; }
</style>
