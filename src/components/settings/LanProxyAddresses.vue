<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Copy, RefreshCw } from '@lucide/vue';
import { useApp } from '../../composables/useApp';
import { validateProxyPort } from '../../lib/network-settings';
const props = defineProps<{ port: string; pending: boolean }>();
const { t, tx } = useApp();
type Address = { interface: string; ip: string };
const addresses = ref<Address[]>([]);
const loading = ref(false);
const notice = ref('');
const feedback = ref('');
let alive = true;
const port = computed(() => { try { return validateProxyPort(props.port); } catch { return null; } });
const endpoint = (ip: string) => port.value ? `${ip}:${port.value}` : ip;
async function refresh() {
  if (loading.value) return;
  feedback.value = '';
  if (!('__TAURI_INTERNALS__' in window)) { addresses.value = []; notice.value = 'settings.lanDesktopOnly'; return; }
  loading.value = true;
  try {
    const result = await invoke<Address[]>('get_lan_addresses');
    if (alive) { addresses.value = result; notice.value = result.length ? '' : 'settings.lanMissing'; }
  } catch { if (alive) { addresses.value = []; notice.value = 'settings.lanFailed'; } }
  finally { if (alive) loading.value = false; }
}
async function copy(ip: string) {
  try { await navigator.clipboard.writeText(endpoint(ip)); feedback.value = 'settings.lanCopied'; }
  catch { feedback.value = 'error.copyFailed'; }
}
onMounted(() => { void refresh(); window.addEventListener('focus', refresh); window.addEventListener('online', refresh); });
onUnmounted(() => { alive = false; window.removeEventListener('focus', refresh); window.removeEventListener('online', refresh); });
</script>
<template>
  <div class="setting-row lan-address-row">
    <span>{{ t('settings.lanAddress') }}<small class="setting-description">{{ t(pending ? 'settings.lanPending' : 'settings.lanHint') }}</small></span>
    <div class="lan-addresses" :class="{ empty: !addresses.length }">
      <div v-for="address in addresses" :key="`${address.interface}:${address.ip}`" class="lan-address">
        <div><code>{{ endpoint(address.ip) }}</code><small>{{ address.interface }}</small></div>
        <button type="button" :disabled="loading || !port" :aria-label="t('settings.copyLanAddress', { address: endpoint(address.ip) })" :title="t('settings.copyLanAddress', { address: endpoint(address.ip) })" @click="copy(address.ip)"><Copy :size="14" /></button>
      </div>
      <button type="button" class="lan-refresh" :disabled="loading" :aria-label="t('settings.refreshLan')" :title="t('settings.refreshLan')" @click="refresh"><RefreshCw :size="14" :class="{ spin: loading }" /></button>
      <div v-if="loading || notice || feedback" class="lan-address-status"><small v-if="loading" role="status">{{ t('settings.lanLoading') }}</small><small v-else-if="notice" role="status">{{ tx(notice) }}</small><small v-else-if="feedback" role="status">{{ tx(feedback) }}</small></div>
    </div>
  </div>
</template>
<style scoped>
.lan-address-row{align-items:flex-start}
.lan-address-row>span{flex:1;min-width:0;font-weight:500}
.lan-address-row .setting-description{max-width:380px;font-size:12px;font-weight:400;line-height:1.6;color:var(--muted)}
.lan-addresses{display:grid;grid-template-columns:minmax(0,1fr) 26px;gap:6px 8px;min-width:200px;max-width:55%;flex:none}
.lan-address{grid-column:1;display:flex;justify-content:flex-end;align-items:center;gap:8px;padding:6px 8px 6px 12px;border:1px solid var(--line);border-radius:8px;background:var(--surface)}
.lan-address>div{flex:1;min-width:0}
.lan-address code{display:block;color:var(--strong);font:13px/1.6 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;white-space:nowrap;-webkit-user-select:text;user-select:text}
.lan-address small{display:block;max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--muted);font-size:11px}
.lan-addresses button{width:26px;height:26px;min-height:26px;padding:0;border:0;border-radius:5px;background:transparent;color:var(--muted);flex:none}
.lan-addresses button:hover:not(:disabled){color:var(--accent-text);background:var(--selected)}
.lan-refresh{grid-column:2;grid-row:1;align-self:center}
.lan-address-status{grid-column:1 / -1;display:flex;align-items:center;justify-content:flex-end;gap:8px;color:var(--muted)}
.lan-addresses.empty .lan-address-status{grid-column:1;grid-row:1;align-self:center}
.lan-address-status small{font-size:12px;line-height:1.6;overflow-wrap:anywhere}
</style>
