<script setup lang="ts">
  import { computed, ref } from "vue";
  import { Gauge, ArrowDown, ArrowUp, Zap, X, ShieldAlert } from "@lucide/vue";
  import SpeedGauge from "./SpeedGauge.vue";
  import { useApp } from "../composables/useApp";
  import BaseButton from "./ui/BaseButton.vue";

  const {
    t,
    tx,
    selectedNodeName,
    testSpeed,
    cancelSpeedTest,
    speedTesting,
    speedProgressMbps,
    speedPing,
    speedResult,
    speedUpResult,
    speedUpProgressMbps,
    speedError,
  } = useApp();

  const dialog = ref<HTMLDialogElement>();
  // Opening the dialog first shows the data-usage warning; only after clicking "Start test" does it enter the test view. Reopening shows the warning again.
  const armed = ref(false);

  // During the test, follow the live speed (download during the download phase, upload during the upload phase); afterward, freeze on the average download speed.
  const gaugeValue = computed(() => {
    if (speedTesting.value) return speedResult.value ? speedUpProgressMbps.value : speedProgressMbps.value;
    return speedResult.value?.mbps ?? 0;
  });
  function fmtMbps(v: number): string {
    return v >= 100 ? Math.round(v).toString() : v.toFixed(2);
  }
  const downloadText = computed(() => {
    if (speedResult.value) return fmtMbps(speedResult.value.mbps);
    if (speedTesting.value && !speedResult.value)
      return speedProgressMbps.value > 0 ? speedProgressMbps.value.toFixed(1) : "–";
    return "–";
  });
  const uploadText = computed(() => {
    if (speedUpResult.value) return fmtMbps(speedUpResult.value.mbps);
    if (speedTesting.value && speedResult.value)
      return speedUpProgressMbps.value > 0 ? speedUpProgressMbps.value.toFixed(1) : "–";
    return "–";
  });
  const pingText = computed(() => (speedPing.value != null ? `${speedPing.value}` : "–"));
  const nodeLabel = computed(() =>
    t("unlock.currentNode", { name: tx(selectedNodeName.value) || t("nodes.notSelected") })
  );
  // During the test, show a phase-based hint ("testing download/upload"); otherwise show the regular description.
  const statusLine = computed(() => {
    if (speedTesting.value) return speedResult.value ? t("speed.testingUpload") : t("speed.testingDownload");
    return t("speed.hint");
  });
  const startLabel = computed(() =>
    speedTesting.value
      ? t("speed.testing")
      : speedResult.value || speedError.value
        ? t("speed.retest")
        : t("speed.start")
  );

  // Per-node data-usage reference: the test runs over a 10s window, so actual usage ≈ speed × 10s (1.25MB/Mbps).
  const COST_ROWS = [
    { speed: "100 Mbps", cost: "125 MB" },
    { speed: "300 Mbps", cost: "375 MB" },
    { speed: "500 Mbps", cost: "625 MB" },
    { speed: "1 Gbps", cost: "1.25 GB" },
  ];

  function open() {
    armed.value = false;
    dialog.value?.showModal();
  }
  function close() {
    dialog.value?.close();
  }
  function onClose() {
    cancelSpeedTest();
  }
  function begin() {
    armed.value = true;
    void testSpeed();
  }
  defineExpose({ open });
</script>

<template>
  <dialog ref="dialog" class="speed-dialog" aria-labelledby="speed-title" @close="onClose" @cancel.prevent="close">
    <header class="speed-head">
      <span class="speed-icon"><Gauge :size="20" /></span>
      <h2 id="speed-title">{{ t("speed.title") }}</h2>
      <BaseButton class="speed-x" :aria-label="t('common.close')" @click="close">
        <X :size="18" />
      </BaseButton>
    </header>

    <p class="speed-node">{{ nodeLabel }}</p>

    <!-- Before testing: data-usage warning + per-node usage reference table -->
    <template v-if="!armed">
      <div class="speed-warn" role="note">
        <ShieldAlert :size="18" />
        <span>{{ t("speed.warn") }}</span>
      </div>
      <table class="speed-cost">
        <thead>
          <tr>
            <th>{{ t("speed.colSpeed") }}</th>
            <th>{{ t("speed.colCost") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in COST_ROWS" :key="row.speed">
            <td>{{ row.speed }}</td>
            <td>{{ row.cost }}</td>
          </tr>
        </tbody>
      </table>
      <div class="speed-actions">
        <BaseButton class="primary" @click="begin"><Gauge :size="15" />{{ t("speed.start") }}</BaseButton>
        <BaseButton @click="close">{{ t("common.close") }}</BaseButton>
      </div>
    </template>

    <!-- Testing / done: gauge + three stats -->
    <template v-else>
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
        <div class="speed-stat" :class="{ muted: !speedUpResult && !(speedTesting && speedResult) }">
          <span class="speed-stat-key"><ArrowUp :size="14" />{{ t("speed.upload") }}</span>
          <strong class="ul">{{ uploadText }}<small>Mbps</small></strong>
        </div>
      </div>

      <p v-if="speedError" class="speed-error" role="alert">{{ tx(speedError) }}</p>
      <p v-else class="speed-hint">{{ statusLine }}</p>

      <div class="speed-actions">
        <BaseButton class="primary" :disabled="speedTesting" @click="testSpeed()">
          <Gauge :size="15" />{{ startLabel }}
        </BaseButton>
        <BaseButton @click="close">{{ t("common.close") }}</BaseButton>
      </div>
    </template>
  </dialog>
</template>

<style lang="scss" scoped>
  .speed-dialog {
    width: 360px;
    max-width: calc(100vw - 32px);
    padding: 22px;
    border: 0;
    border-radius: 16px;
    background: var(--bg);
    color: var(--text);
    box-shadow: var(--shadow-hover);
  }
  .speed-dialog::backdrop {
    background: #18293e66;
  }
  .speed-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .speed-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: var(--selected);
    color: var(--accent-text);
  }
  .speed-head h2 {
    flex: 1;
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .speed-x {
    flex: none;
    width: 30px;
    height: 30px;
    min-height: 0;
    padding: 0;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
  }
  .speed-x:hover {
    background: var(--selected);
    color: var(--text);
  }
  .speed-node {
    margin: 10px 0 4px;
    color: var(--muted);
    font-size: 13px;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .speed-warn {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    margin: 12px 0 0;
    padding: 11px 13px;
    border-radius: 10px;
    background: var(--warning-bg, #fff4e5);
    color: var(--warning-text, #9a5b00);
    font-size: 13px;
    line-height: 1.55;
  }
  .speed-warn svg {
    flex: none;
    margin-top: 1px;
  }
  .speed-cost {
    width: 100%;
    margin-top: 12px;
    border-collapse: collapse;
    font-size: 13px;
  }
  .speed-cost th,
  .speed-cost td {
    padding: 8px 10px;
    text-align: left;
    border-bottom: 1px solid var(--border);
  }
  .speed-cost th {
    color: var(--muted);
    font-weight: 550;
  }
  .speed-cost td {
    font-variant-numeric: tabular-nums;
  }
  .speed-cost td:last-child,
  .speed-cost th:last-child {
    text-align: right;
    color: var(--strong);
  }
  .speed-cost tbody tr:last-child td {
    border-bottom: 0;
  }
  .speed-stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin-top: 4px;
  }
  .speed-stat {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 10px 4px;
    border-radius: 10px;
    background: var(--surface);
  }
  .speed-stat-key {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-size: 12px;
  }
  .speed-stat strong {
    font-size: 19px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .speed-stat strong.dl {
    color: var(--accent-text);
  }
  .speed-stat strong.ul {
    color: var(--accent-text);
  }
  .speed-stat strong small {
    margin-left: 3px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 500;
  }
  .speed-stat.muted strong {
    color: var(--muted);
  }
  .speed-hint,
  .speed-error {
    margin: 14px 0 0;
    font-size: 12px;
    line-height: 1.6;
    text-align: center;
  }
  .speed-hint {
    color: var(--muted);
  }
  .speed-error {
    color: var(--danger);
  }
  .speed-actions {
    display: flex;
    gap: 10px;
    margin-top: 16px;
  }
  .speed-actions button {
    flex: 1;
    min-height: 40px;
  }
  .speed-actions .primary {
    flex: 2;
  }
</style>
