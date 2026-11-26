<script setup lang="ts">
  import { computed } from "vue";
  import { X } from "@lucide/vue";
  import BaseButton from "./ui/BaseButton.vue";
  import { translator, type Locale, type MessageKey } from "../i18n";
  import { connectionEndpoint, type ConnectionRow } from "../lib/connection-details";
  const props = defineProps<{ row: ConnectionRow; active: boolean; locale: Locale; chain: string }>();
  const emit = defineEmits<{ close: []; closeConnection: [] }>();
  const { t } = translator(() => props.locale);
  const host = computed(() =>
    connectionEndpoint(props.row.metadata.host || props.row.metadata.destinationIP, props.row.metadata.destinationPort)
  );
  function bytes(value: number) {
    const units = ["B", "KB", "MB", "GB", "TB"];
    let v = Math.max(0, value || 0),
      i = 0;
    while (v >= 1024 && i < units.length - 1) {
      v /= 1024;
      i++;
    }
    return `${v.toFixed(i ? 1 : 0)} ${units[i]}`;
  }
  const started = computed(() => {
    const value = new Date(props.row.start);
    return Number.isNaN(value.getTime())
      ? props.row.start || "—"
      : value.toLocaleString(props.locale, {
          year: "numeric",
          month: "2-digit",
          day: "2-digit",
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
          hour12: false,
        });
  });
  // Kind: inbound type + transport protocol, e.g. Tun(tcp).
  const kind = computed(() => {
    const type = props.row.metadata.type;
    const net = props.row.metadata.network;
    if (type && net) return `${type}(${net})`;
    return type || net?.toUpperCase() || "—";
  });
  const general = computed<[MessageKey, string][]>(() => [
    ["connections.detailStatus", props.active ? t("connections.active") : t("connections.closed")],
    ["connections.host", host.value],
    ["connections.detailType", kind.value],
    ["connections.detailInbound", props.row.metadata.type || "—"],
    ["connections.detailNetwork", props.row.metadata.network || "—"],
    ["connections.detailId", props.row.id || "—"],
    ["connections.detailProcess", props.row.metadata.processPath || "—"],
    ["connections.detailStarted", started.value],
    ["connections.detailSource", connectionEndpoint(props.row.metadata.sourceIP || "", props.row.metadata.sourcePort)],
    [
      "connections.detailDestination",
      connectionEndpoint(props.row.metadata.destinationIP, props.row.metadata.destinationPort),
    ],
    ["connections.downloaded", bytes(props.row.download)],
    ["connections.uploaded", bytes(props.row.upload)],
    ["connections.downloadSpeed", `${bytes(props.active ? props.row.downloadSpeed : 0)}/s`],
    ["connections.uploadSpeed", `${bytes(props.active ? props.row.uploadSpeed : 0)}/s`],
  ]);
  const routing = computed<[MessageKey, string][]>(() => [
    ["connections.chain", props.chain || "—"],
    ["connections.detailRule", props.row.rule || "—"],
    ["connections.detailRulePayload", props.row.rulePayload || "—"],
  ]);
  const raw = computed(() => JSON.stringify(props.row, null, 2));
</script>
<template>
  <div class="conn-detail" :aria-label="t('connections.detailTitle')">
    <header class="conn-detail-head">
      <strong :title="host">{{ host }}</strong>
      <BaseButton :aria-label="t('common.close')" @click="emit('close')">
        <X :size="16" />
      </BaseButton>
    </header>
    <h2 class="conn-detail-section">{{ t("connections.detailGeneral") }}</h2>
    <dl class="conn-detail-fields">
      <template v-for="[label, value] in general" :key="label"
        ><dt>{{ t(label) }}</dt>
        <dd :title="value">{{ value }}</dd></template
      >
    </dl>
    <h2 class="conn-detail-section">{{ t("connections.detailRouting") }}</h2>
    <dl class="conn-detail-fields">
      <template v-for="[label, value] in routing" :key="label"
        ><dt>{{ t(label) }}</dt>
        <dd :title="value">{{ value }}</dd></template
      >
    </dl>
    <details class="conn-detail-raw">
      <summary>{{ t("connections.detailRaw") }}</summary>
      <pre>{{ raw }}</pre>
    </details>
    <footer class="conn-detail-foot">
      <BaseButton class="conn-detail-close" :disabled="!active" @click="emit('closeConnection')">
        {{ t("connections.closeConnection") }}
      </BaseButton>
    </footer>
  </div>
</template>
<style lang="scss" scoped>
  .conn-detail {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .conn-detail-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 10px;
  }
  .conn-detail-head strong {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 600;
  }
  .conn-detail-head button {
    flex: none;
    width: 26px;
    height: 26px;
    min-height: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--muted);
    border-radius: 6px;
  }
  .conn-detail-head button:hover {
    color: var(--text);
    background: var(--hover);
  }
  .conn-detail-fields {
    margin: 0;
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 10px 14px;
    align-items: baseline;
    overflow: auto;
    user-select: text;
    padding: 2px 0;
  }
  .conn-detail-fields dt {
    color: var(--muted);
    white-space: nowrap;
    font-size: 13px;
    line-height: 1.5;
  }
  .conn-detail-fields dd {
    margin: 0;
    overflow-wrap: anywhere;
    font-variant-numeric: tabular-nums;
    font-size: 13px;
    line-height: 1.5;
  }
  .conn-detail-section {
    margin: 12px 0 8px;
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
    line-height: 1.4;
  }
  .conn-detail-raw {
    margin-top: 12px;
    color: var(--muted);
    font-size: 12px;
  }
  .conn-detail-raw summary {
    cursor: pointer;
  }
  .conn-detail-raw pre {
    margin: 8px 0 0;
    max-height: 160px;
    overflow: auto;
    padding: 8px;
    border-radius: 6px;
    background: var(--seg-bg);
    color: var(--text);
    font-size: 12px;
    line-height: 1.45;
    white-space: pre-wrap;
  }
  .conn-detail-foot {
    display: flex;
    justify-content: center;
    margin-top: 16px;
  }
  .conn-detail-close {
    min-height: 34px;
    padding: 7px 30px;
    border: 0;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 13px;
    font-weight: 500;
  }
  .conn-detail-close:hover:not(:disabled) {
    background: var(--primary-hover);
  }
  .conn-detail-close:disabled {
    opacity: 1;
    background: var(--seg-bg);
    color: var(--muted);
  }
  .conn-detail button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
</style>
