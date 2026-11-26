<script setup lang="ts">
  import { computed, nextTick, onMounted, onUnmounted, ref, useId, watch } from "vue";
  import {
    CaseSensitive,
    WholeWord,
    Regex,
    Check,
    ChevronsDown,
    Copy,
    Download,
    Pause,
    Play,
    RefreshCw,
    Search,
    X,
  } from "@lucide/vue";
  import { useApp } from "../../composables/useApp";
  import SelectMenu from "../../components/ui/SelectMenu.vue";
  import HelpTip from "../../components/ui/HelpTip.vue";
  import BaseButton from "../../components/ui/BaseButton.vue";
  import { filterLogs, logEntries, logRange, redactLog, type LogEntry } from "../../lib/log-viewer";
  import { connectionSearch, type SearchOptions } from "../../lib/connection-search";

  const {
    logViewport,
    logs,
    activityLogs,
    renderLogLine,
    followLogs,
    scrollLogs,
    onLogScroll,
    refreshLogs,
    logsLoading,
    logsError,
    t,
    tx,
  } = useApp();
  const query = ref("");
  const searchOptions = ref<SearchOptions>({ caseSensitive: false, wholeWord: false, regex: false });
  const search = computed(() => connectionSearch(query.value, searchOptions.value));
  const searchErrorId = useId();
  const searchModes = [
    { key: "caseSensitive", icon: CaseSensitive, label: "connections.caseSensitive" },
    { key: "wholeWord", icon: WholeWord, label: "connections.wholeWord" },
    { key: "regex", icon: Regex, label: "connections.regex" },
  ] as const;
  const level = ref("all");
  const source = ref("all");
  const paused = ref(false);
  const frozen = ref<LogEntry[]>([]);
  const feedback = ref("");
  const sharing = ref(false);
  const scrollTop = ref(0);
  const viewportHeight = ref(400);
  const selected = ref<LogEntry>();
  const detail = ref<HTMLElement>();
  let observer: ResizeObserver | undefined;
  const current = computed(() => logEntries(activityLogs.value, logs.value, renderLogLine));
  const entries = computed(() => (paused.value ? frozen.value : current.value));
  const filtered = computed(() =>
    filterLogs(entries.value, query.value, level.value, source.value, searchOptions.value)
  );
  // Estimate the monospace measure (wide characters count twice) without measuring every DOM row.
  // One shared width keeps the last column and header stable while vertically virtualizing the list.
  const messageWidth = computed(() =>
    Math.max(640, ...filtered.value.map(row => row.message.replace(/[^\x00-\x7F]/g, "xx").length * 8 + 16))
  );
  const range = computed(() =>
    logRange(filtered.value.length, Math.max(0, scrollTop.value - 34), Math.max(0, viewportHeight.value - 34))
  );
  const visible = computed(() => filtered.value.slice(range.value.start, range.value.end));
  const levelOptions = computed(() => [
    { value: "all", label: t("logs.allLevels") },
    ...["ERROR", "WARN", "INFO", "DEBUG", "TRACE"].map(value => ({ value, label: value })),
  ]);
  const sourceOptions = computed(() => [
    { value: "all", label: t("logs.allSources") },
    { value: "client", label: t("logs.client") },
    { value: "core", label: t("logs.core") },
    { value: "tun", label: "TUN" },
  ]);
  const sourceLabel = (row: LogEntry) =>
    row.source === "tun" ? "TUN" : t(row.source === "client" ? "logs.client" : "logs.core");
  function onScroll() {
    const next = logViewport.value?.scrollTop || 0;
    if (next !== scrollTop.value) onLogScroll();
    scrollTop.value = next;
  }
  async function bottom() {
    followLogs.value = true;
    await nextTick();
    if (logViewport.value) logViewport.value.scrollTop = logViewport.value.scrollHeight;
    onScroll();
  }
  function pause() {
    if (!paused.value) frozen.value = current.value;
    paused.value = !paused.value;
    feedback.value = "";
  }
  async function refresh() {
    try {
      await refreshLogs();
    } catch {
      /* The inline error preserves the current view. */
    }
  }
  function reset() {
    query.value = "";
    level.value = "all";
    source.value = "all";
    searchOptions.value = { caseSensitive: false, wholeWord: false, regex: false };
  }
  async function copy(row?: LogEntry) {
    if (sharing.value) return;
    sharing.value = true;
    try {
      await navigator.clipboard.writeText((row ? [row] : filtered.value).map(item => item.text).join("\n"));
      feedback.value = "logs.copySuccess";
    } catch {
      feedback.value = "logs.copyFailed";
    } finally {
      sharing.value = false;
    }
  }
  function download() {
    let url = "";
    try {
      url = URL.createObjectURL(
        new Blob([filtered.value.map(row => row.text).join("\n") + "\n"], {
          type: "text/plain;charset=utf-8",
        })
      );
      const link = document.createElement("a");
      link.href = url;
      link.download = `ClashNova-${new Date().toISOString().replace(/[:.]/g, "-")}.log`;
      document.body.append(link);
      link.click();
      link.remove();
      feedback.value = "logs.exportSuccess";
    } catch {
      feedback.value = "logs.exportFailed";
    } finally {
      if (url) setTimeout(() => URL.revokeObjectURL(url), 1000);
    }
  }
  async function inspect(row: LogEntry) {
    selected.value = row;
    feedback.value = "";
    await nextTick();
    detail.value?.showPopover();
  }
  watch(filtered, async () => {
    await nextTick();
    if (followLogs.value) await scrollLogs();
    else if (logViewport.value)
      logViewport.value.scrollTop = Math.min(logViewport.value.scrollTop, logViewport.value.scrollHeight);
    onScroll();
  });
  watch(
    [query, level, source, searchOptions],
    async () => {
      feedback.value = "";
      await nextTick();
      if (logViewport.value) logViewport.value.scrollTop = 0;
      onScroll();
    },
    { deep: true }
  );
  onMounted(async () => {
    observer = new ResizeObserver(([entry]) => {
      viewportHeight.value = entry.contentRect.height;
      if (followLogs.value) void bottom();
    });
    if (logViewport.value) observer.observe(logViewport.value);
    await bottom();
  });
  onUnmounted(() => observer?.disconnect());
</script>
<template>
  <section class="log-viewer" :aria-label="t('logs.activityLogs')">
    <div class="log-toolbar">
      <div class="log-search" :class="{ invalid: search.error }">
        <Search :size="16" /><input
          v-model="query"
          :aria-label="t('logs.search')"
          :placeholder="t('logs.search')"
          :aria-invalid="search.error"
          :aria-describedby="search.error ? searchErrorId : undefined"
          spellcheck="false"
        /><BaseButton v-if="query" :aria-label="t('logs.clearSearch')" @click="query = ''">
          <X :size="14" />
        </BaseButton>
        <div class="log-search-modes">
          <BaseButton
            v-for="mode in searchModes"
            :key="mode.key"
            :class="{ selected: searchOptions[mode.key] }"
            :aria-label="t(mode.label)"
            :title="t(mode.label)"
            :aria-pressed="searchOptions[mode.key]"
            @click="searchOptions[mode.key] = !searchOptions[mode.key]"
          >
            <component :is="mode.icon" :size="16" />
          </BaseButton>
        </div>
      </div>
      <div class="log-filters">
        <SelectMenu v-model="level" :label="t('logs.level')" :options="levelOptions" /><SelectMenu
          v-model="source"
          :label="t('logs.source')"
          :options="sourceOptions"
        />
      </div>
      <div class="log-actions">
        <BaseButton
          :class="{ selected: paused }"
          :aria-label="t(paused ? 'logs.resume' : 'logs.pause')"
          :title="t(paused ? 'logs.resume' : 'logs.pause')"
          :aria-pressed="paused"
          @click="pause"
        >
          <component :is="paused ? Play : Pause" :size="16" />
        </BaseButton>
        <BaseButton :disabled="logsLoading" :aria-label="t('logs.refresh')" :title="t('logs.refresh')" @click="refresh">
          <RefreshCw :size="16" :class="{ spin: logsLoading }" />
        </BaseButton>
        <span class="log-divider" />
        <BaseButton
          :disabled="!filtered.length || sharing"
          :aria-label="t('logs.copy')"
          :title="t('logs.copy')"
          @click="copy()"
        >
          <Copy :size="16" />
        </BaseButton>
        <BaseButton
          :disabled="!filtered.length"
          :aria-label="t('logs.export')"
          :title="t('logs.export')"
          @click="download"
        >
          <Download :size="16" />
        </BaseButton>
      </div>
    </div>
    <div v-if="logsError" class="log-read-error" role="alert">
      <span>{{ t("logs.readFailed") }} · {{ redactLog(tx(logsError)) }}</span
      ><BaseButton :disabled="logsLoading" @click="refresh">{{ t("logs.retry") }}</BaseButton>
    </div>
    <div v-if="search.error" :id="searchErrorId" class="log-read-error" role="alert">
      <span>{{ t("connections.invalidRegex") }}</span><BaseButton @click="searchOptions.regex = false">
        <X :size="14" />{{ t("connections.regex") }}
      </BaseButton>
    </div>
    <div
      ref="logViewport"
      class="log-body"
      :aria-label="t('logs.activityLogs')"
      tabindex="0"
      @scroll.passive="onScroll"
    >
      <div v-if="filtered.length" class="log-table" :style="{ '--log-message-width': `${messageWidth}px` }">
        <div class="log-columns" aria-hidden="true">
          <span>{{ t("logs.time") }}</span><span>{{ t("logs.level") }}</span
          ><span>{{ t("logs.message") }}</span
          ><span>{{ t("logs.source") }}</span>
        </div>
        <div :style="{ height: `${range.before}px` }" aria-hidden="true" />
        <BaseButton
          v-for="row in visible"
          :key="row.id"
          class="log-row"
          :class="row.level.toLowerCase()"
          :title="row.text"
          @click="inspect(row)"
        >
          <time :datetime="row.time.replace(' ', 'T')">{{ row.time || "—" }}</time><span class="log-level">{{
            row.level
          }}</span
          ><span class="log-message">{{ row.message }}</span
          ><span class="log-source">{{ sourceLabel(row) }}</span>
        </BaseButton>
        <div :style="{ height: `${range.after}px` }" aria-hidden="true" />
      </div>
      <div v-else class="log-empty">
        <Search v-if="entries.length" :size="25" /><span>{{
          t(entries.length ? "logs.filteredEmpty" : "logs.empty")
        }}</span
        ><BaseButton v-if="entries.length" @click="reset">
          {{ t("logs.resetFilters") }}
        </BaseButton>
      </div>
    </div>
    <footer class="log-footer">
      <div class="log-state">
        <span class="log-state-dot" :class="{ paused, failed: !!logsError }" /><span>{{
          t(paused ? "logs.paused" : logsError ? "logs.readFailed" : "logs.live")
        }}</span
        ><span class="log-count">{{ t("logs.shown", { count: filtered.length, total: entries.length }) }}</span
        ><HelpTip :text="t('logs.windowHint')" />
      </div>
      <span v-if="feedback" class="log-feedback" role="status">{{ tx(feedback) }}</span>
      <div class="log-follow">
        <BaseButton
          :aria-pressed="followLogs"
          :class="{ selected: followLogs }"
          @click="followLogs ? (followLogs = false) : bottom()"
        >
          <Check v-if="followLogs" :size="14" />{{ t("logs.followLatest") }}</BaseButton
        ><BaseButton :aria-label="t('logs.scrollBottom')" :title="t('logs.scrollBottom')" @click="bottom">
          <ChevronsDown :size="16" />
        </BaseButton>
      </div>
    </footer>
    <div ref="detail" popover="auto" class="log-detail" role="dialog" :aria-label="t('logs.detail')">
      <template v-if="selected"
        ><div class="log-detail-head">
          <strong>{{ t("logs.detail") }}</strong><BaseButton
            :aria-label="t('common.close')"
            @click="detail?.hidePopover()"
          >
            <X :size="17" />
          </BaseButton>
        </div>
        <div class="log-detail-meta">
          <time>{{ selected.time || "—" }}</time><span>{{ selected.level }}</span
          ><span>{{ sourceLabel(selected) }}</span>
        </div>
        <pre>{{ selected.message }}</pre>
        <div class="log-detail-foot">
          <small role="status">{{ feedback ? tx(feedback) : t("logs.exportHint") }}</small><BaseButton
            :disabled="sharing"
            @click="copy(selected)"
          >
            <Copy :size="15" />{{ t("logs.copyLine") }}
          </BaseButton>
        </div></template
      >
    </div>
  </section>
</template>
<style scoped lang="scss" src="./index.scss"></style>
