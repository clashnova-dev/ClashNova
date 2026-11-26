<script setup lang="ts">
  import { computed, onUnmounted, ref, watch } from "vue";
  import { Route, ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Pencil } from "@lucide/vue";
  import { useApp } from "../../composables/useApp";
  import SearchField from "../../components/ui/SearchField.vue";
  import SelectMenu from "../../components/ui/SelectMenu.vue";
  import ToggleSwitch from "../../components/ui/ToggleSwitch.vue";
  import BaseButton from "../../components/ui/BaseButton.vue";
  import ProfileRulesDialog from "../../components/ProfileRulesDialog.vue";
  import { connectionSearch, type SearchOptions } from "../../lib/connection-search";
  import { liveAppPolicyRules } from "../../lib/app-policies";
  import { ruleGroups, rulePage, type RuleSource, type RuleRow } from "../../lib/rule-viewer";
  import { builtinRouteRules } from "../../lib/routing-rules";
  const {
    t,
    tx,
    locale,
    rawProfile,
    buildProfileConfig,
    routeMode,
    chinaGeosite,
    adBlock,
    setRuleToggle,
    appPolicies,
    connectionNodeNames,
    selectedNodeName,
    tab,
    profileKind,
    currentExternal,
  } = useApp();
  const rulesEditor = ref<InstanceType<typeof ProfileRulesDialog> | null>(null);
  function openRulesEditor() {
    if (currentExternal.value) rulesEditor.value?.open(currentExternal.value);
  }
  const query = ref(""),
    appliedQuery = ref(""),
    source = ref<RuleSource | "all">("all"),
    page = ref(1);
  const options = ref<SearchOptions>({ caseSensitive: false, wholeWord: false, regex: false });
  let timer: ReturnType<typeof setTimeout> | undefined;
  watch(query, value => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      appliedQuery.value = value;
      page.value = 1;
    }, 120);
  });
  onUnmounted(() => clearTimeout(timer));
  watch(
    [source, options],
    () => {
      page.value = 1;
    },
    { deep: true }
  );
  const data = computed(() => {
    if (!rawProfile.value) return { groups: [], error: "" };
    try {
      const config = JSON.parse(buildProfileConfig());
      return {
        groups: ruleGroups(
          config.route?.rules,
          config.route?.final,
          liveAppPolicyRules(appPolicies.value).length,
          builtinRouteRules(routeMode.value === "rule", {
            chinaGeosite: chinaGeosite.value,
            adBlock: adBlock.value,
          }).length
        ),
        error: "",
      };
    } catch (error) {
      return { groups: [], error: String(error) };
    }
  });
  const sourceNames = computed(() => ({
    application: t("rules.application"),
    builtin: t("rules.builtin"),
    subscription: t("rules.subscription"),
    fallback: t("rules.fallback"),
  }));
  const sourceOptions = computed(() => [
    { value: "all" as const, label: t("rules.allSources") },
    ...(["application", "builtin", "subscription", "fallback"] as const).map(value => ({
      value,
      label: sourceNames.value[value],
    })),
  ]);
  function target(value: string) {
    return value === "select"
      ? tx(selectedNodeName.value) || t("connections.selector")
      : value === "reject"
        ? t("connections.blockShort")
        : connectionNodeNames.value[value] || value;
  }
  const search = computed(() => connectionSearch(appliedQuery.value, options.value));
  const result = computed(() =>
    rulePage(
      data.value.groups,
      page.value,
      100,
      source.value,
      appliedQuery.value.trim()
        ? row =>
            !search.value.error &&
            search.value.matches(
              [row.kind, row.value, row.target, target(row.target), sourceNames.value[row.source]].join(" ")
            )
        : undefined
    )
  );
  watch(data, () => {
    page.value = 1;
  });
  watch(
    () => result.value.pages,
    value => {
      if (page.value > value) page.value = value;
    }
  );
  function rowTitle(row: RuleRow) {
    return t("rules.groupOrder", { order: row.order });
  }
</script>
<template>
  <section class="rules-page" :aria-label="t('nav.rules')">
    <p v-if="data.error" class="form-error" role="alert">{{ tx(data.error) }}</p>
    <div class="rules-panel">
      <div class="rules-toolbar">
        <SearchField
          v-model="query"
          v-model:options="options"
          :locale="locale"
          :label="t('rules.search')"
          :placeholder="t('rules.search')"
          :invalid="search.error"
        />
        <SelectMenu v-model="source" class="source-select" :label="t('rules.source')" :options="sourceOptions" />
        <BaseButton
          v-if="profileKind === 'external' && currentExternal"
          class="rules-edit-btn"
          @click="openRulesEditor"
        >
          <Pencil :size="15" />{{ t("rules.edit") }}
        </BaseButton>
      </div>
      <div v-if="rawProfile && routeMode === 'rule'" class="rules-options">
        <label class="rules-option"
          ><ToggleSwitch
            :model-value="chinaGeosite"
            :label="t('rules.chinaGeosite')"
            @update:model-value="v => setRuleToggle('chinaGeosite', v)"
          /><span>{{ t("rules.chinaGeosite") }}</span></label
        >
        <label class="rules-option"
          ><ToggleSwitch
            :model-value="adBlock"
            :label="t('rules.adBlock')"
            @update:model-value="v => setRuleToggle('adBlock', v)"
          /><span>{{ t("rules.adBlock") }}</span></label
        >
      </div>
      <p v-if="search.error" class="rules-search-error" role="alert">
        {{ t("connections.invalidRegex") }}
      </p>
      <div class="rules-scroll">
        <table v-if="rawProfile && !data.error && result.rows.length">
          <colgroup>
            <col class="type-col" />
            <col />
            <col class="target-col" />
            <col class="source-col" />
          </colgroup>
          <thead>
            <tr>
              <th>{{ t("rules.type") }}</th>
              <th>{{ t("rules.content") }}</th>
              <th>{{ t("rules.target") }}</th>
              <th>{{ t("rules.source") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in result.rows" :key="row.id">
              <td :title="rowTitle(row)">{{ row.kind }}</td>
              <td :title="row.value">{{ row.value }}</td>
              <td :title="target(row.target)">{{ target(row.target) }}</td>
              <td :title="sourceNames[row.source]">{{ sourceNames[row.source] }}</td>
            </tr>
          </tbody>
        </table>
        <div v-else class="rules-empty">
          <Route :size="25" />
          <p>
            {{ t(!rawProfile ? "rules.needSubscription" : data.error ? "rules.invalidConfig" : "rules.noMatches") }}
          </p>
          <BaseButton v-if="!rawProfile" @click="tab = 'sub'">
            {{ t("nav.subscriptions") }}
          </BaseButton>
        </div>
      </div>
      <footer class="rules-footer">
        <span role="status">{{ t("rules.resultCount", { count: result.total }) }}</span>
        <nav :aria-label="t('rules.pagination')">
          <BaseButton
            :disabled="page <= 1"
            :aria-label="t('rules.firstPage')"
            :title="t('rules.firstPage')"
            @click="page = 1"
          >
            <ChevronsLeft :size="16" />
          </BaseButton>
          <BaseButton
            :disabled="page <= 1"
            :aria-label="t('rules.previousPage')"
            :title="t('rules.previousPage')"
            @click="page--"
          >
            <ChevronLeft :size="16" />
          </BaseButton>
          <span>{{ t("rules.pageOf", { page, pages: result.pages }) }}</span>
          <BaseButton
            :disabled="page >= result.pages"
            :aria-label="t('rules.nextPage')"
            :title="t('rules.nextPage')"
            @click="page++"
          >
            <ChevronRight :size="16" />
          </BaseButton>
          <BaseButton
            :disabled="page >= result.pages"
            :aria-label="t('rules.lastPage')"
            :title="t('rules.lastPage')"
            @click="page = result.pages"
          >
            <ChevronsRight :size="16" />
          </BaseButton>
        </nav>
      </footer>
    </div>
    <ProfileRulesDialog ref="rulesEditor" />
  </section>
</template>
<style scoped lang="scss" src="./index.scss"></style>
