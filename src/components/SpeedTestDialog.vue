<script setup lang="ts">
import { computed, ref } from "vue";
import { Gauge, ArrowDown, ArrowUp, Zap, X } from "@lucide/vue";
import SpeedGauge from "./SpeedGauge.vue";
import { useApp } from "../composables/useApp";

const { t, tx, selectedNodeName, testSpeed, cancelSpeedTest, speedTesting, speedProgressMbps, speedPing, speedResult, speedError } = useApp();

const dialog = ref<HTMLDialogElement>();
// 测速中跟随实时速率，结束后定格平均速率。
const gaugeValue = computed(() => (speedTesting.value ? speedProgressMbps.value : speedResult.value?.mbps ?? 0));
const downloadText = computed(() => {
  if (speedResult.value) return speedResult.value.mbps >= 100 ? Math.round(speedResult.value.mbps).toString() : speedResult.value.mbps.toFixed(2);
  if (speedTesting.value) return speedProgressMbps.value > 0 ? speedProgressMbps.value.toFixed(1) : "–";
  return "–";
});
const pingText = computed(() => (speedPing.value != null ? `${speedPing.value}` : "–"));
const nodeLabel = computed(() => t("unlock.currentNode", { name: tx(selectedNodeName.value) || t("nodes.notSelected") }));
const startLabel = computed(() => (speedTesting.value ? t("speed.testing") : speedResult.value || speedError.value ? t("speed.retest") : t("speed.start")));

function open() {
  dialog.value?.showModal();
}
function close() {
  dialog.value?.close();
}
function onClose() {
  cancelSpeedTest();
}
defineExpose({ open });
</script>

<template>
  <dialog ref="dialog" class="speed-dialog" aria-labelledby="speed-title" @close="onClose" @cancel.prevent="close">
    <header class="speed-head">
      <span class="speed-icon"><Gauge :size="20" /></span>
      <h2 id="speed-title">{{ t("speed.title") }}</h2>
      <button type="button" class="speed-x" :aria-label="t('common.close')" @click="close"><X :size="18" /></button>
    </header>

    <p class="speed-node">{{ nodeLabel }}</p>

    <SpeedGauge :value="gaugeValue" :active="speedTesting" />

    <div class="speed-stats">
      <div class="speed-stat">
        <span class="speed-stat-key"><Zap :size="14" />{{ t("speed.ping") }}</span>
        <strong>{{ pingText }}<small>ms</small></strong>
      </div>
      <div class="speed-stat">
        <span class="speed-stat-key"><ArrowDown :size="14" />{{ t("speed.download") }}</span>
        <strong class="dl">{{ downloadText }}<small>Mbps</small></strong>
      </div>
      <div class="speed-stat muted">
        <span class="speed-stat-key"><ArrowUp :size="14" />{{ t("speed.upload") }}</span>
        <strong>–<small>Mbps</small></strong>
      </div>
    </div>

    <p v-if="speedError" class="speed-error" role="alert">{{ tx(speedError) }}</p>
    <p v-else class="speed-hint">{{ t("speed.hint") }}</p>

    <div class="speed-actions">
      <button type="button" class="primary" :disabled="speedTesting" @click="testSpeed()">
        <Gauge :size="15" />{{ startLabel }}
      </button>
      <button type="button" @click="close">{{ t("common.close") }}</button>
    </div>
  </dialog>
</template>

<style scoped>
.speed-dialog { width: 360px; max-width: calc(100vw - 32px); padding: 22px; border: 0; border-radius: 16px; background: var(--bg); color: var(--text); box-shadow: var(--shadow-hover); }
.speed-dialog::backdrop { background: #18293e66; }
.speed-head { display: flex; align-items: center; gap: 10px; }
.speed-icon { display: grid; place-items: center; flex: none; width: 36px; height: 36px; border-radius: 10px; background: var(--selected); color: var(--accent-text); }
.speed-head h2 { flex: 1; margin: 0; font-size: 16px; font-weight: 600; }
.speed-x { flex: none; width: 30px; height: 30px; min-height: 0; padding: 0; border: 0; border-radius: 8px; background: transparent; color: var(--muted); }
.speed-x:hover { background: var(--selected); color: var(--text); }
.speed-node { margin: 10px 0 4px; color: var(--muted); font-size: 13px; text-align: center; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.speed-stats { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin-top: 4px; }
.speed-stat { display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 10px 4px; border-radius: 10px; background: var(--surface); }
.speed-stat-key { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; }
.speed-stat strong { font-size: 19px; font-weight: 650; font-variant-numeric: tabular-nums; }
.speed-stat strong.dl { color: var(--accent-text); }
.speed-stat strong small { margin-left: 3px; color: var(--muted); font-size: 11px; font-weight: 500; }
.speed-stat.muted strong { color: var(--muted); }
.speed-hint, .speed-error { margin: 14px 0 0; font-size: 12px; line-height: 1.6; text-align: center; }
.speed-hint { color: var(--muted); }
.speed-error { color: var(--danger); }
.speed-actions { display: flex; gap: 10px; margin-top: 16px; }
.speed-actions button { flex: 1; min-height: 40px; }
.speed-actions .primary { flex: 2; }
</style>
