<script setup lang="ts">
  import { computed, ref, watch } from "vue";
  import { useApp } from "../../composables/useApp";
  import SelectMenu from "../../components/ui/SelectMenu.vue";
  import BaseButton from "../../components/ui/BaseButton.vue";
  import {
    ArrowDown,
    ArrowDownWideNarrow,
    Check,
    GripVertical,
    Link,
    Plus,
    Search,
    Trash2,
    Wifi,
    X,
    Zap,
  } from "@lucide/vue";
  const {
    t,
    tx,
    tab,
    nodes,
    sortedNodes,
    nodeSort,
    selectedNode,
    selectNode,
    busy,
    connected,
    AUTOMATIC_NODE_ID,
    automaticNodeName,
    regionCode,
    latencies,
    latencyTone,
    latencyLabel,
    testLatencies,
    testing,
    probeCancelling,
    probeProgress,
    probeTotal,
    probeSummary,
    probeErrors,
    coldLatencies,
    warmedLatencies,
    chain,
    chainActive,
    commitChain,
    chainGroups,
    saveChainGroup,
    deleteChainGroup,
    testChain,
    clearChainTest,
    chainTesting,
    chainDelay,
    chainError,
  } = useApp();
  const query = ref("");
  const onlineCount = computed(() => nodes.value.filter(node => node.agentOnline).length);
  const keyword = computed(() => query.value.trim().toLowerCase());
  // Auto-select sits in the grid like any node, so it follows the search box too.
  const showAutomatic = computed(
    () => !keyword.value || [t("nodes.automatic"), "auto"].some(text => text.toLowerCase().includes(keyword.value))
  );
  const visibleNodes = computed(() =>
    keyword.value
      ? sortedNodes.value.filter(node =>
          [node.name, tx(node.region), node.protocol].join(" ").toLowerCase().includes(keyword.value)
        )
      : sortedNodes.value
  );

  // Proxy chain: a fixed-height overlay edits a local draft, applied on demand.
  const chainOpen = ref(false);
  const draft = ref<string[]>([...chain.value]);
  watch(chain, value => {
    draft.value = [...value];
  });
  // Editing the chain invalidates any end-to-end measurement of the previous order.
  watch(draft, () => clearChainTest(), { deep: true });
  const chainDelayTone = computed(() =>
    chainDelay.value == null ? "idle" : chainDelay.value <= 100 ? "good" : chainDelay.value <= 300 ? "fair" : "bad"
  );
  // Only surface the latency pill once a node has a result (or is mid-test / failed / offline);
  // an untested node shows nothing rather than a "not tested" placeholder.
  const showLatency = (id: string) => latencyLabel(id) !== "nodes.notTested";
  const nodeName = (id: string) => nodes.value.find(node => node.id === id)?.name || id;
  const nodeProto = (id: string) => nodes.value.find(node => node.id === id)?.protocol || "";
  const addOptions = computed(() => [
    { value: "", label: t("chain.add") },
    ...nodes.value
      .filter(node => !draft.value.includes(node.id))
      .map(node => ({
        value: node.id,
        label: node.name,
        meta: latencies.value[node.id] != null && latencies.value[node.id] >= 0 ? `${latencies.value[node.id]} ms` : "",
      })),
  ]);
  const flowText = computed(
    () => `${t("chain.device")} → ${draft.value.map(nodeName).join(" → ")} → ${t("chain.target")}`
  );
  const dragIndex = ref<number | null>(null);
  const overIndex = ref<number | null>(null);

  // Saved chain presets: the editor edits a draft; selecting a group loads it.
  const editingId = ref("");
  const groupName = ref("");
  const groupSelectOptions = computed(() => [
    { value: "", label: t("chain.newChain") },
    ...chainGroups.value.map(g => ({ value: g.id, label: g.name })),
  ]);
  function loadGroup(id: string) {
    const g = chainGroups.value.find(x => x.id === id);
    if (!g) return;
    editingId.value = g.id;
    groupName.value = g.name;
    draft.value = [...g.nodes];
  }
  function newGroup() {
    editingId.value = "";
    groupName.value = "";
    draft.value = [];
  }
  function onGroupSelect(id: string) {
    if (id) loadGroup(id);
    else newGroup();
  }
  function saveGroup() {
    const name = groupName.value.trim();
    if (!name || draft.value.length < 2) return;
    editingId.value = saveChainGroup(name, draft.value, editingId.value || undefined);
  }
  function removeGroup() {
    if (!editingId.value) return;
    deleteChainGroup(editingId.value);
    const first = chainGroups.value[0];
    if (first) loadGroup(first.id);
    else newGroup();
  }
  function openChain() {
    // Prefer the group matching the applied chain, else the first group, else an ad-hoc chain.
    const match = chainGroups.value.find(
      g => g.nodes.length === chain.value.length && g.nodes.every((n, i) => n === chain.value[i])
    );
    if (match) loadGroup(match.id);
    else if (chainGroups.value.length && !chain.value.length) loadGroup(chainGroups.value[0].id);
    else {
      editingId.value = "";
      groupName.value = "";
      draft.value = [...chain.value];
    }
    chainOpen.value = true;
  }
  function closeChain() {
    chainOpen.value = false;
  }
  function addToChain(id: string) {
    if (id) draft.value = [...draft.value, id];
  }
  // Pointer-based drag reorder via the grip handle (more reliable than native DnD).
  function startDrag(index: number, event: PointerEvent) {
    event.preventDefault();
    dragIndex.value = index;
    overIndex.value = index;
    const onMove = (e: PointerEvent) => {
      const rows = Array.from(document.querySelectorAll<HTMLElement>(".chain-row"));
      for (let i = 0; i < rows.length; i++) {
        const r = rows[i].getBoundingClientRect();
        if (e.clientY >= r.top && e.clientY <= r.bottom) {
          overIndex.value = i;
          return;
        }
      }
    };
    const onUp = () => {
      const from = dragIndex.value,
        to = overIndex.value;
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
  function removeHop(index: number) {
    draft.value = draft.value.filter((_, i) => i !== index);
  }
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
      <label class="search-box"
        ><Search :size="16" /><input
          v-model="query"
          type="text"
          :aria-label="t('nodes.searchNodes')"
          :placeholder="t('nodes.searchByNameOrRegion')"
          @keyup="query = ($event.target as HTMLInputElement).value"
      /></label>
      <SelectMenu
        v-model="nodeSort"
        :icon="ArrowDownWideNarrow"
        :label="t('nodes.nodeOrder')"
        :options="[
          { value: 'default', label: t('nodes.originalOrder') },
          { value: 'latency', label: t('nodes.lowestLatency') },
        ]"
      />
      <BaseButton variant="primary" class="test-btn" :disabled="probeCancelling" @click="testLatencies()">
        <Wifi :size="16" />{{ testing ? t("nodes.stop") : t("nodes.testAll") }}
      </BaseButton>
      <BaseButton
        class="chain-btn"
        :class="{ active: chainActive }"
        :title="t('chain.title')"
        :aria-label="t('chain.title')"
        @click="openChain"
      >
        <Link :size="18" />
      </BaseButton>
    </div>
    <template v-if="nodes.length">
      <div class="nodes-meta">
        <span
          >{{ t("nodes.all") }} <strong>{{ nodes.length }}</strong> · {{ t("nodes.available") }} {{ onlineCount }}</span
        >
        <span v-if="probeSummary" class="probe-status" role="status"
          ><span>{{ tx(probeSummary) }}</span
          ><progress v-if="testing" :value="probeProgress" :max="probeTotal" :aria-label="t('nodes.nodeTestProgress')"
        /></span>
      </div>
      <div
        v-if="showAutomatic || visibleNodes.length"
        class="node-grid"
        role="radiogroup"
        :aria-label="t('nodes.availableNodes')"
      >
        <BaseButton
          v-if="showAutomatic"
          class="node-card"
          role="radio"
          :aria-checked="selectedNode === AUTOMATIC_NODE_ID"
          :class="{ selected: selectedNode === AUTOMATIC_NODE_ID }"
          :disabled="busy || !onlineCount"
          :title="t('nodes.checksEvery3MinutesAnd')"
          @click="selectNode(AUTOMATIC_NODE_ID)"
        >
          <span class="region-chip" aria-hidden="true"><Zap :size="16" /></span>
          <span class="node-card-text"
            ><strong>{{ t("nodes.automatic") }}</strong
            ><small
              >{{
                selectedNode === AUTOMATIC_NODE_ID && connected
                  ? automaticNodeName || t("nodes.selectingANode")
                  : t("nodes.autoSwitch")
              }}{{ selectedNode === AUTOMATIC_NODE_ID ? " · " + t("common.inUse") : "" }}</small
            ></span
          >
          <span class="pill" :class="latencyTone(AUTOMATIC_NODE_ID)">{{ tx(latencyLabel(AUTOMATIC_NODE_ID)) }}</span>
          <Check v-if="selectedNode === AUTOMATIC_NODE_ID" class="node-check" :size="18" :stroke-width="2.4" />
        </BaseButton>
        <BaseButton
          v-for="node in visibleNodes"
          :key="node.id"
          class="node-card"
          role="radio"
          :aria-checked="selectedNode === node.id"
          :class="{ selected: selectedNode === node.id }"
          :disabled="!node.agentOnline"
          @click="selectNode(node.id)"
        >
          <span class="region-chip" aria-hidden="true">{{ regionCode(node.region, node.name) || "··" }}</span>
          <span class="node-card-text"
            ><strong :title="node.name">{{ node.name }}</strong
            ><small>{{ node.protocol }}{{ selectedNode === node.id ? " · " + t("common.inUse") : "" }}</small></span
          >
          <span
            v-if="showLatency(node.id)"
            class="pill"
            :class="latencyTone(node.id)"
            :title="
              tx(probeErrors[node.id]) ||
              (coldLatencies[node.id]
                ? t(
                    warmedLatencies[node.id]
                      ? 'nodes.requestTimeAfterConnectionSetup'
                      : 'nodes.firstConnectionMsSecondTest',
                    { delay: coldLatencies[node.id] }
                  )
                : t('nodes.requestLatencyThroughTheNode'))
            "
            >{{ tx(latencyLabel(node.id)) }}</span
          >
          <Check v-if="selectedNode === node.id" class="node-check" :size="18" :stroke-width="2.4" />
        </BaseButton>
      </div>
      <div v-else class="empty-note">{{ t("nodes.noMatchingNodes") }}</div>
    </template>
    <div v-else class="card empty-card">
      <p>{{ t("nodes.thisSubscriptionHasNoAvailable") }}</p>
      <BaseButton variant="primary" @click="tab = 'sub'">{{ t("nodes.manageSubscriptions") }}</BaseButton>
    </div>

    <div v-if="chainOpen" class="chain-overlay" @click.self="closeChain">
      <div class="chain-dialog" role="dialog" aria-modal="true" aria-label="Proxy chain">
        <div class="dialog-heading">
          <h2>{{ t("chain.title") }}</h2>
          <BaseButton variant="icon" :aria-label="t('common.close')" @click="closeChain">
            <X :size="20" />
          </BaseButton>
        </div>
        <div class="chain-groups">
          <SelectMenu
            :model-value="editingId"
            :icon="Link"
            :label="t('chain.groups')"
            :options="groupSelectOptions"
            @update:model-value="onGroupSelect"
          />
          <input
            class="chain-name"
            v-model="groupName"
            :placeholder="t('chain.namePlaceholder')"
            maxlength="40"
            :aria-label="t('chain.namePlaceholder')"
          />
          <BaseButton
            class="chain-gbtn danger"
            :disabled="!editingId"
            :title="t('chain.deleteGroup')"
            :aria-label="t('chain.deleteGroup')"
            @click="removeGroup"
          >
            <Trash2 :size="16" />
          </BaseButton>
        </div>
        <p class="chain-flow">
          <span>{{ draft.length >= 2 ? flowText : t("chain.hint") }}</span>
          <span v-if="chainTesting" class="pill idle">{{ t("chain.testing") }}</span>
          <span v-else-if="chainError" class="pill bad">{{ t("chain.testFailed") }}</span>
          <span v-else-if="chainDelay != null" class="pill" :class="chainDelayTone">{{ chainDelay }} ms</span>
        </p>
        <p v-if="chainError && !chainTesting" class="chain-err" role="alert">
          {{ tx(chainError) }}
        </p>
        <div class="chain-add">
          <SelectMenu
            :model-value="''"
            :icon="Plus"
            :label="t('chain.add')"
            :options="addOptions"
            @update:model-value="addToChain"
          />
          <BaseButton class="chain-test" :disabled="draft.length < 2" @click="testChain(draft)">
            <Wifi :size="16" />{{ chainTesting ? t("chain.testing") : t("chain.test") }}
          </BaseButton>
        </div>
        <div class="chain-body">
          <ol v-if="draft.length" class="chain-list">
            <template v-for="(id, index) in draft" :key="id">
              <li
                class="chain-row"
                :class="{
                  dragging: dragIndex === index,
                  over: overIndex === index && dragIndex !== index,
                }"
              >
                <span class="chain-grip" aria-hidden="true" @pointerdown="startDrag(index, $event)"
                  ><GripVertical :size="17"
                /></span>
                <span class="chain-role" :class="roleClass(index)">{{ roleLabel(index) }}</span>
                <span class="chain-node" :title="nodeName(id)">{{ nodeName(id) }}</span>
                <span v-if="nodeProto(id)" class="chain-proto">{{ nodeProto(id) }}</span>
                <span v-if="showLatency(id)" class="pill" :class="latencyTone(id)">{{ tx(latencyLabel(id)) }}</span>
                <BaseButton
                  class="chain-rm"
                  draggable="false"
                  :aria-label="t('chain.remove')"
                  :title="t('chain.remove')"
                  @click="removeHop(index)"
                >
                  <Trash2 :size="18" />
                </BaseButton>
              </li>
              <div v-if="index < draft.length - 1" class="chain-arrow" aria-hidden="true">
                <ArrowDown :size="16" />
              </div>
            </template>
          </ol>
          <p v-else class="chain-empty">{{ t("chain.emptyHint") }}</p>
        </div>
        <div class="dialog-actions">
          <BaseButton :disabled="!draft.length" @click="draft = []">
            {{ t("chain.clear") }}
          </BaseButton>
          <BaseButton class="chain-save" :disabled="!groupName.trim() || draft.length < 2" @click="saveGroup">
            {{ t("chain.saveGroup") }}
          </BaseButton>
          <BaseButton variant="primary" :disabled="busy || (draft.length < 2 && !chainActive)" @click="applyChain">
            {{ draft.length >= 2 ? t("chain.apply") : t("chain.applyOff") }}
          </BaseButton>
        </div>
      </div>
    </div>
  </div>
</template>
<style scoped lang="scss" src="./index.scss"></style>
