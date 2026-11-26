<template>
  <section class="panel desktop-help">
    <BaseButton class="help-back" @click="$emit('back')"
      ><ArrowLeft :size="16" />{{ t("help.backToSettings") }}</BaseButton
    >
    <label class="help-search"
      >{{ t("help.search")
      }}<input
        v-model="query"
        type="text"
        :placeholder="t('help.searchPlaceholder')"
        @keyup="query = ($event.target as HTMLInputElement).value"
    /></label>
    <nav class="help-topics" :aria-label="t('help.topics')">
      <BaseButton v-for="topic in topics" :key="topic" :aria-pressed="category === topic" @click="category = topic">
        {{ topic === "all" ? t("help.all") : topic }}
      </BaseButton>
    </nav>
    <p class="help-count" role="status">
      {{ filtered.length === 1 ? t("help.countOne") : t("help.count", { count: filtered.length }) }}
    </p>
    <p v-if="!filtered.length">{{ t("help.empty") }}</p>
    <details v-for="article in filtered" :key="article.id">
      <summary>{{ article.title }}</summary>
      <p v-for="(paragraph, index) in article.body.split('\n')" :key="index">{{ paragraph }}</p>
    </details>
  </section>
</template>
<script setup lang="ts">
  import { computed, ref, watch } from "vue";
  import { ArrowLeft } from "@lucide/vue";
  import BaseButton from "../../components/ui/BaseButton.vue";
  import { desktopHelp } from "./help-content";
  import { translator, type Locale } from "../../i18n";
  const props = defineProps<{ locale: Locale }>();
  defineEmits<{ back: [] }>();
  const query = ref("");
  const category = ref("all");
  const { t } = translator(() => props.locale);
  const en = computed(() => props.locale === "en");
  const articles = computed(() =>
    desktopHelp.map(item => ({
      id: item.id,
      category: en.value ? item.categoryEn : item.category,
      title: en.value ? item.titleEn : item.title,
      body: en.value ? item.bodyEn : item.body,
    }))
  );
  const topics = computed(() => ["all", ...new Set(articles.value.map(item => item.category))]);
  const filtered = computed(() =>
    articles.value.filter(
      item =>
        (category.value === "all" || category.value === item.category) &&
        `${item.title} ${item.body}`.toLocaleLowerCase().includes(query.value.trim().toLocaleLowerCase())
    )
  );
  watch(
    () => props.locale,
    () => {
      category.value = "all";
      query.value = "";
    }
  );
</script>
<style scoped lang="scss" src="./index.scss"></style>
