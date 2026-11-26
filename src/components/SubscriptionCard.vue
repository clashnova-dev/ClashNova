<template>
  <article
    class="subscription-card"
    :class="{ selected }"
    :aria-label="name + (selected ? t('subscriptions.currentSuffix') : '')"
  >
    <header>
      <span class="sub-icon" aria-hidden="true"
        ><Send v-if="kind === 'airport'" :size="18" /><Link v-else :size="18"
      /></span>
      <div class="sub-title">
        <h3 :title="name">{{ name }}</h3>
        <small :title="source">{{ subtitle }}</small>
      </div>
      <span v-if="selected" class="pill tint">{{ t("common.inUse") }}</span>
    </header>
    <div class="sub-usage">
      <div>
        <span>{{ t("subscriptions.usedTraffic") }}</span><strong>{{ usageLabel }}</strong>
      </div>
      <div
        class="subscription-meter"
        role="progressbar"
        :aria-label="t('common.trafficUsage')"
        :aria-valuenow="percent ?? 0"
        aria-valuemin="0"
        aria-valuemax="100"
      >
        <span v-if="percent !== undefined" :style="{ width: percent + '%' }" :class="{ exhausted: percent >= 100 }" />
      </div>
    </div>
    <dl class="sub-meta">
      <div>
        <dt>{{ t("subscriptions.expiresAt") }}</dt>
        <dd :class="{ expired }">{{ expiryLabel }}</dd>
      </div>
      <div>
        <dt>{{ t("subscriptions.updatedAt") }}</dt>
        <dd>{{ updatedAt ? date(updatedAt, true) : "—" }}</dd>
      </div>
    </dl>
    <footer><slot /></footer>
  </article>
</template>
<script setup lang="ts">
  import { computed } from "vue";
  import { Link, Send } from "@lucide/vue";
  import { translator, type Locale } from "../i18n";
  import { usedTraffic, trafficPercent, type SubscriptionUsage } from "../lib/subscription-info";
  const props = defineProps<{
    name: string;
    selected: boolean;
    source: string;
    updatedAt?: string;
    usage?: SubscriptionUsage;
    locale: Locale;
    kind?: "airport" | "external";
    label?: string;
  }>();
  const { t, tx } = translator(() => props.locale);
  const subtitle = computed(
    () => [props.label, props.source].filter(Boolean).join(" · ") || t("subscriptions.localFile")
  );
  const used = computed(() => usedTraffic(props.usage));
  function bytes(value: number) {
    const units = ["B", "KB", "MB", "GB", "TB"];
    let index = 0;
    while (value >= 1024 && index < units.length - 1) {
      value /= 1024;
      index++;
    }
    return `${value.toFixed(index ? 1 : 0)} ${units[index]}`;
  }
  const usageLabel = computed(
    () =>
      `${used.value === undefined ? "—" : bytes(used.value)} / ${Number.isSafeInteger(props.usage?.total) && props.usage!.total! > 0 ? bytes(props.usage!.total!) : t("common.notProvided")}`
  );
  const percent = computed(() => trafficPercent(props.usage));
  const expiry = computed(() =>
    Number.isSafeInteger(props.usage?.expire) && props.usage!.expire! > 0 && props.usage!.expire! <= 8640000000000
      ? props.usage!.expire! * 1000
      : undefined
  );
  const expired = computed(() => expiry.value !== undefined && expiry.value <= Date.now());
  function date(value: string | number, time = false) {
    const d = new Date(value);
    return Number.isNaN(d.getTime())
      ? "—"
      : d.toLocaleString(props.locale, {
          year: "numeric",
          month: "2-digit",
          day: "2-digit",
          ...(time ? { hour: "2-digit", minute: "2-digit" } : {}),
        });
  }
  const expiryLabel = computed(() =>
    expiry.value === undefined
      ? t("common.notProvided")
      : date(expiry.value) + (expired.value ? ` · ${t("subscriptions.expired")}` : "")
  );
</script>
<style lang="scss" scoped>
  .subscription-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    min-height: 190px;
    height: 100%;
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: 16px;
    background: var(--bg);
    box-shadow: var(--shadow-card);
  }
  .subscription-card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .sub-icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: var(--pill-bg);
    color: var(--pill-text);
  }
  .selected .sub-icon {
    background: var(--selected);
    color: var(--accent-text);
  }
  .sub-title {
    flex: 1;
    min-width: 0;
  }
  h3 {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 16px;
    line-height: 1.35;
    font-weight: 650;
    color: var(--text);
  }
  .sub-title small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    line-height: 16px;
    color: var(--muted);
  }
  .sub-usage > div:first-child {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 4px 10px;
  }
  .sub-usage span {
    font-size: 14px;
    color: var(--muted);
  }
  .sub-usage strong {
    font-size: 15px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .subscription-meter {
    height: 6px;
    margin-top: 6px;
    overflow: hidden;
    border-radius: 8px;
    background: var(--line);
  }
  .subscription-meter > span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
  }
  .subscription-meter > .exhausted {
    background: var(--danger);
  }
  .sub-meta {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 4px 10px;
    margin: 0;
    font-size: 14px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 2px 0 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .expired {
    color: var(--danger);
  }
  footer {
    margin-top: auto;
  }
  footer :deep(button) {
    min-height: 36px;
    padding: 4px 12px;
    border-radius: 9px;
    font-size: 14px;
  }
  footer :deep(.icon-button) {
    width: 36px;
    height: 36px;
    padding: 0;
  }
</style>
