<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useApp } from "../composables/useApp";
import SelectMenu from "../components/ui/SelectMenu.vue";
import { ArrowDown, ArrowDownWideNarrow, Check, GripVertical, Link, Plus, Search, Trash2, Wifi, X, Zap } from "@lucide/vue";
const { t, tx, tab, nodes, sortedNodes, nodeSort, selectedNode, selectNode, busy, connected, AUTOMATIC_NODE_ID, automaticNodeName, regionCode, latencies, latencyTone, latencyLabel, testLatencies, testing, probeCancelling, probeProgress, probeTotal, probeSummary, probeErrors, coldLatencies, warmedLatencies, chain, chainActive, commitChain, chainGroups, saveChainGroup, deleteChainGroup, testChain, clearChainTest, chainTesting, chainDelay, chainError } = useApp();
const query = ref("");
const onlineCount = computed(() => nodes.value.filter(node => node.agentOnline).length);
const keyword = computed(() => query.value.trim().toLowerCase());
// Auto-select sits in the grid like any node, so it follows the search box too.
const showAutomatic = computed(() => !keyword.value || [t("nodes.automatic"), "auto"].some(text => text.toLowerCase().includes(keyword.value)));
const visibleNodes = computed(() => keyword.value ? sortedNodes.value.filter(node => [node.name, tx(node.region), node.protocol].join(" ").toLowerCase().includes(keyword.value)) : sortedNodes.value);

// Proxy chain (链式代理): a fixed-height overlay edits a local draft, applied on demand.
const chainOpen = ref(false);
const draft = ref<string[]>([...chain.value]);
watch(chain, value => { draft.value = [...value]; });
// Editing the chain invalidates any end-to-end measurement of the previous order.
watch(draft, () => clearChainTest(), { deep: true });
const chainDelayTone = computed(() => chainDelay.value == null ? "idle" : chainDelay.value <= 100 ? "good" : chainDelay.value <= 300 ? "fair" : "bad");
// Only surface the latency pill once a node has a result (or is mid-test / failed / offline);
// an untested node shows nothing rather than a "not tested" placeholder.
const showLatency = (id: string) => latencyLabel(id) !== "nodes.notTested";
const nodeName = (id: string) => nodes.value.find(node => node.id === id)?.name || id;
const nodeProto = (id: string) => nodes.value.find(node => node.id === id)?.protocol || "";
const addOptions = computed(() => [{ value: "", label: t("chain.add") }, ...nodes.value.filter(node => !draft.value.includes(node.id)).map(node => ({ value: node.id, label: node.name, meta: latencies.value[node.id] != null && latencies.value[node.id] >= 0 ? `${latencies.value[node.id]} ms` : "" }))]);
const flowText = computed(() => `${t("chain.device")} → ${draft.value.map(nodeName).join(" → ")} → ${t("chain.target")}`);
const dragIndex = ref<number | null>(null);
const overIndex = ref<number | null>(null);

// Saved chain presets (链路组): the editor edits a draft; selecting a group loads it.
const editingId = ref("");
const groupName = ref("");
const groupSelectOptions = computed(() => [{ value: "", label: t("chain.newChain") }, ...chainGroups.value.map(g => ({ value: g.id, label: g.name }))]);
function loadGroup(id: string) {
  const g = chainGroups.value.find(x => x.id === id);
  if (!g) return;
  editingId.value = g.id; groupName.value = g.name; draft.value = [...g.nodes];
}
function newGroup() { editingId.value = ""; groupName.value = ""; draft.value = []; }
function onGroupSelect(id: string) { if (id) loadGroup(id); else newGroup(); }
function saveGroup() {
  const name = groupName.value.trim();
  if (!name || draft.value.length < 2) return;
  editingId.value = saveChainGroup(name, draft.value, editingId.value || undefined);
}
function removeGroup() {
  if (!editingId.value) return;
  deleteChainGroup(editingId.value);
  const first = chainGroups.value[0];
  if (first) loadGroup(first.id); else newGroup();
}
function openChain() {
  // Prefer the group matching the applied chain, else the first group, else an ad-hoc chain.
  const match = chainGroups.value.find(g => g.nodes.length === chain.value.length && g.nodes.every((n, i) => n === chain.value[i]));
  if (match) loadGroup(match.id);
  else if (chainGroups.value.length && !chain.value.length) loadGroup(chainGroups.value[0].id);
  else { editingId.value = ""; groupName.value = ""; draft.value = [...chain.value]; }
  chainOpen.value = true;
}
function closeChain() { chainOpen.value = false; }
function addToChain(id: string) { if (id) draft.value = [...draft.value, id]; }
// Pointer-based drag reorder via the grip handle (more reliable than native DnD).
function startDrag(index: number, event: PointerEvent) {
  event.preventDefault();
  dragIndex.value = index;
  overIndex.value = index;
  const onMove = (e: PointerEvent) => {
    const rows = Array.from(document.querySelectorAll<HTMLElement>(".chain-row"));
    for (let i = 0; i < rows.length; i++) {
      const r = rows[i].getBoundingClientRect();
      if (e.clientY >= r.top && e.clientY <= r.bottom) { overIndex.value = i; return; }
    }
  };
  const onUp = () => {
    const from = dragIndex.value, to = overIndex.value;
    if (from !== null && to !== null && from !== to) {
      const copy = [...draft.value];
      const [moved] = copy.splice(from, 1);
      copy.splice(to, 0, moved);
      draft.value = copy;
    }
    dragIndex.value = null;
    overIndex.value = null;
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}
function removeHop(index: number) { draft.value = draft.value.filter((_, i) => i !== index); }
function roleLabel(index: number) {
  if (draft.value.length < 2) return t("chain.node");
  if (index === 0) return t("chain.entry");
  if (index === draft.value.length - 1) return t("chain.exit");
  return t("chain.relay");
}
function roleClass(index: number) {
  if (draft.value.length >= 2 && index === 0) return "is-entry";
  if (draft.value.length >= 2 && index === draft.value.length - 1) return "is-exit";
  return "is-relay";
}
async function applyChain() {
  const name = groupName.value.trim();
  if (name && draft.value.length) editingId.value = saveChainGroup(name, draft.value, editingId.value || undefined);
  await commitChain([...draft.value]);
  closeChain();
}
</script>
<template>
  <div class="nodes-page">
    <div class="nodes-tools">
      <label class="search-box"><Search :size="16" /><input v-model="query" type="text" :aria-label="t('nodes.searchNodes')" :placeholder="t('nodes.searchByNameOrRegion')" @keyup="query = ($event.target as HTMLInputElement).value" /></label>
      <SelectMenu v-model="nodeSort" :icon="ArrowDownWideNarrow" :label="t('nodes.nodeOrder')" :options="[{ value: 'default', label: t('nodes.originalOrder') }, { value: 'latency', label: t('nodes.lowestLatency') }]" />
      <button class="primary test-btn" :disabled="probeCancelling" @click="testLatencies()"><Wifi :size="16" />{{ testing ? t("nodes.stop") : t("nodes.testAll") }}</button>
      <button class="chain-btn" :class="{ active: chainActive }" :title="t('chain.title')" :aria-label="t('chain.title')" @click="openChain"><Link :size="18" /></button>
    </div>
    <template v-if="nodes.length">
      <div class="nodes-meta">
        <span>{{ t("nodes.all") }} <strong>{{ nodes.length }}</strong> · {{ t("nodes.available") }} {{ onlineCount }}</span>
        <span v-if="probeSummary" class="probe-status" role="status"><span>{{ tx(probeSummary) }}</span><progress v-if="testing" :value="probeProgress" :max="probeTotal" :aria-label="t('nodes.nodeTestProgress')" /></span>
      </div>
      <div v-if="showAutomatic || visibleNodes.length" class="node-grid" role="radiogroup" :aria-label="t('nodes.availableNodes')">
        <button v-if="showAutomatic" class="node-card" role="radio" :aria-checked="selectedNode === AUTOMATIC_NODE_ID" :class="{ selected: selectedNode === AUTOMATIC_NODE_ID }" :disabled="busy || !onlineCount" :title="t('nodes.checksEvery3MinutesAnd')" @click="selectNode(AUTOMATIC_NODE_ID)">
          <span class="region-chip" aria-hidden="true"><Zap :size="16" /></span>
          <span class="node-card-text"><strong>{{ t("nodes.automatic") }}</strong><small>{{ selectedNode === AUTOMATIC_NODE_ID && connected ? automaticNodeName || t("nodes.selectingANode") : t("nodes.autoSwitch") }}{{ selectedNode === AUTOMATIC_NODE_ID ? " · " + t("common.inUse") : "" }}</small></span>
          <span class="pill" :class="latencyTone(AUTOMATIC_NODE_ID)">{{ tx(latencyLabel(AUTOMATIC_NODE_ID)) }}</span>
          <Check v-if="selectedNode === AUTOMATIC_NODE_ID" class="node-check" :size="18" :stroke-width="2.4" />
        </button>
        <button v-for="node in visibleNodes" :key="node.id" class="node-card" role="radio" :aria-checked="selectedNode === node.id" :class="{ selected: selectedNode === node.id }" :disabled="!node.agentOnline" @click="selectNode(node.id)">
          <span class="region-chip" aria-hidden="true">{{ regionCode(node.region, node.name) || "··" }}</span>
          <span class="node-card-text"><strong :title="node.name">{{ node.name }}</strong><small>{{ node.protocol }}{{ selectedNode === node.id ? " · " + t("common.inUse") : "" }}</small></span>
          <span v-if="showLatency(node.id)" class="pill" :class="latencyTone(node.id)" :title="tx(probeErrors[node.id]) || (coldLatencies[node.id] ? t(warmedLatencies[node.id] ? 'nodes.requestTimeAfterConnectionSetup' : 'nodes.firstConnectionMsSecondTest', { delay: coldLatencies[node.id] }) : t('nodes.requestLatencyThroughTheNode'))">{{ tx(latencyLabel(node.id)) }}</span>
          <Check v-if="selectedNode === node.id" class="node-check" :size="18" :stroke-width="2.4" />
        </button>
      </div>
      <div v-else class="empty-note">{{ t("nodes.noMatchingNodes") }}</div>
    </template>
    <div v-else class="card empty-card"><p>{{ t("nodes.thisSubscriptionHasNoAvailable") }}</p><button class="primary" @click="tab = 'sub'">{{ t("nodes.manageSubscriptions") }}</button></div>

    <div v-if="chainOpen" class="chain-overlay" @click.self="closeChain">
      <div class="chain-dialog" role="dialog" aria-modal="true" aria-label="Proxy chain">
        <div class="dialog-heading"><h2>{{ t("chain.title") }}</h2><button class="icon-button" type="button" :aria-label="t('common.close')" @click="closeChain"><X :size="20" /></button></div>
        <div class="chain-groups">
          <SelectMenu :model-value="editingId" :icon="Link" :label="t('chain.groups')" :options="groupSelectOptions" @update:model-value="onGroupSelect" />
          <input class="chain-name" v-model="groupName" :placeholder="t('chain.namePlaceholder')" maxlength="40" :aria-label="t('chain.namePlaceholder')" />
          <button class="chain-gbtn danger" type="button" :disabled="!editingId" :title="t('chain.deleteGroup')" :aria-label="t('chain.deleteGroup')" @click="removeGroup"><Trash2 :size="16" /></button>
        </div>
        <p class="chain-flow">
          <span>{{ draft.length >= 2 ? flowText : t("chain.hint") }}</span>
          <span v-if="chainTesting" class="pill idle">{{ t("chain.testing") }}</span>
          <span v-else-if="chainError" class="pill bad">{{ t("chain.testFailed") }}</span>
          <span v-else-if="chainDelay != null" class="pill" :class="chainDelayTone">{{ chainDelay }} ms</span>
        </p>
        <p v-if="chainError && !chainTesting" class="chain-err" role="alert">{{ tx(chainError) }}</p>
        <div class="chain-add">
          <SelectMenu :model-value="''" :icon="Plus" :label="t('chain.add')" :options="addOptions" @update:model-value="addToChain" />
          <button class="chain-test" type="button" :disabled="draft.length < 2" @click="testChain(draft)"><Wifi :size="16" />{{ chainTesting ? t("chain.testing") : t("chain.test") }}</button>
        </div>
        <div class="chain-body">
          <ol v-if="draft.length" class="chain-list">
            <template v-for="(id, index) in draft" :key="id">
              <li class="chain-row" :class="{ dragging: dragIndex === index, over: overIndex === index && dragIndex !== index }">
                <span class="chain-grip" aria-hidden="true" @pointerdown="startDrag(index, $event)"><GripVertical :size="17" /></span>
                <span class="chain-role" :class="roleClass(index)">{{ roleLabel(index) }}</span>
                <span class="chain-node" :title="nodeName(id)">{{ nodeName(id) }}</span>
                <span v-if="nodeProto(id)" class="chain-proto">{{ nodeProto(id) }}</span>
                <span v-if="showLatency(id)" class="pill" :class="latencyTone(id)">{{ tx(latencyLabel(id)) }}</span>
                <button class="chain-rm" type="button" draggable="false" :aria-label="t('chain.remove')" :title="t('chain.remove')" @click="removeHop(index)"><Trash2 :size="18" /></button>
              </li>
              <div v-if="index < draft.length - 1" class="chain-arrow" aria-hidden="true"><ArrowDown :size="16" /></div>
            </template>
          </ol>
          <p v-else class="chain-empty">{{ t("chain.emptyHint") }}</p>
        </div>
        <div class="dialog-actions">
          <button type="button" :disabled="!draft.length" @click="draft = []">{{ t("chain.clear") }}</button>
          <button type="button" class="chain-save" :disabled="!groupName.trim() || draft.length < 2" @click="saveGroup">{{ t("chain.saveGroup") }}</button>
          <button class="primary" type="button" :disabled="busy || (draft.length < 2 && !chainActive)" @click="applyChain">{{ draft.length >= 2 ? t("chain.apply") : t("chain.applyOff") }}</button>
        </div>
      </div>
    </div>
  </div>
</template>
<style scoped>
.search-box { flex:1 1 130px; }
/* Fixed width so the label swap (检测全部 ↔ 停止) never resizes the button. */
.test-btn { min-width:120px; justify-content:center; }
.chain-btn { flex:none; display:inline-flex; align-items:center; justify-content:center; width:44px; min-height:40px; border:1px solid var(--border); border-radius:10px; background:var(--bg); color:var(--accent); }
.chain-btn:hover { border-color:var(--selected-border); }
.chain-btn.active { border-color:var(--accent); background:var(--selected); }
.chain-btn svg { flex:none; }

.chain-overlay { position:fixed; inset:0; z-index:200; display:flex; align-items:center; justify-content:center; padding:16px; background:#18293e66; }
.chain-dialog { background:var(--bg); width:640px; max-width:100%; height:min(82vh, 680px); display:flex; flex-direction:column; gap:14px; padding:22px; border-radius:16px; color:var(--text); box-shadow:0 24px 80px #172d5140; }
.chain-groups { display:flex; align-items:stretch; gap:8px; }
.chain-groups :deep(.select-menu) { flex:0 1 170px; min-width:120px; }
.chain-name { flex:1; min-width:0; height:40px; padding:0 12px; border:1px solid var(--border); border-radius:10px; background:var(--bg); color:var(--text); font-size:14px; }
.chain-name:focus { outline:none; border-color:var(--selected-border); }
.chain-gbtn { flex:none; display:inline-flex; align-items:center; justify-content:center; width:40px; height:40px; border:1px solid var(--border); border-radius:10px; background:var(--bg); color:var(--strong); cursor:pointer; }
.chain-gbtn:hover:not(:disabled) { border-color:var(--selected-border); }
.chain-gbtn.danger { color:var(--danger); }
.chain-gbtn:disabled { opacity:.4; cursor:default; }
.chain-gbtn svg { flex:none; }
.chain-save { border:1px solid var(--border); background:var(--bg); color:var(--strong); }
.chain-dialog .dialog-actions { gap:10px; }
.chain-dialog .dialog-actions button:first-child { margin-right:auto; }
.chain-flow { margin:0; display:flex; align-items:center; flex-wrap:wrap; gap:8px; color:var(--muted); font-size:13px; line-height:1.6; }
.chain-flow > span:first-child { word-break:break-word; }
.chain-flow .pill { flex:none; }
.chain-err { margin:-6px 0 0; font-size:12px; line-height:1.5; color:var(--danger); }
.chain-add { display:flex; align-items:stretch; gap:10px; }
.chain-add :deep(.select-menu) { flex:1; min-width:0; }
.chain-test { flex:none; display:inline-flex; align-items:center; gap:7px; padding:0 16px; min-height:40px; border:1px solid var(--border); border-radius:10px; background:var(--bg); color:var(--strong); font-size:14px; white-space:nowrap; cursor:pointer; }
.chain-test:hover:not(:disabled) { border-color:var(--selected-border); }
.chain-test:disabled { opacity:.5; cursor:default; }
.chain-test svg { flex:none; color:var(--accent); }
.chain-body { flex:1; min-height:0; overflow-y:auto; margin:0 -4px; padding:2px 4px; }
.chain-list { margin:0; padding:0; list-style:none; display:flex; flex-direction:column; }
.chain-row { display:flex; align-items:center; gap:9px; padding:8px 8px 8px 6px; border:1px solid var(--line); border-radius:12px; background:var(--surface); }
.chain-row.dragging { opacity:.4; }
.chain-row.over { border-color:var(--accent); box-shadow:0 0 0 1px var(--accent); }
.chain-grip { flex:none; display:inline-flex; color:var(--muted); cursor:grab; touch-action:none; }
.chain-grip:active { cursor:grabbing; }
.chain-role { flex:none; padding:4px 12px; border-radius:999px; font-size:12.5px; font-weight:600; }
.chain-role.is-entry { background:var(--success-bg); color:var(--success); }
.chain-role.is-exit { background:var(--warn-bg); color:var(--warn); }
.chain-role.is-relay { background:var(--selected); color:var(--accent-text); }
.chain-node { flex:1; min-width:40px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:14px; font-weight:500; }
.chain-proto { flex:none; padding:3px 9px; border-radius:7px; background:var(--hover); color:var(--muted); font-size:12px; }
.chain-rm { flex:none; display:inline-flex; align-items:center; justify-content:center; width:32px; height:32px; border:none; border-radius:9px; background:none; color:var(--danger); cursor:pointer; }
.chain-rm:hover { background:var(--danger-bg); }
/* SVGs shrink inside flex rows here (see .account svg / .ov-node svg) — pin them. */
.chain-row svg, .chain-arrow svg { flex:none; }
.chain-arrow { display:flex; justify-content:center; align-items:center; color:var(--muted); height:20px; }
.chain-empty { height:100%; margin:0; display:flex; align-items:center; justify-content:center; text-align:center; color:var(--muted); font-size:13px; }
</style>
