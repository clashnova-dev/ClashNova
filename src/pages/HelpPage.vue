<template>
  <section class="panel desktop-help">
    <button class="help-back" @click="$emit('back')"><ArrowLeft :size="16" />{{ t('help.backToSettings') }}</button>
    <label class="help-search">{{ t('help.search') }}<input v-model="query" type="text" :placeholder="t('help.searchPlaceholder')" @keyup="query = ($event.target as HTMLInputElement).value" /></label>
    <nav class="help-topics" :aria-label="t('help.topics')"><button v-for="topic in topics" :key="topic" :aria-pressed="category === topic" @click="category = topic">{{ topic === 'all' ? t('help.all') : topic }}</button></nav>
    <p class="help-count" role="status">{{ filtered.length === 1 ? t('help.countOne') : t('help.count', { count: filtered.length }) }}</p>
    <p v-if="!filtered.length">{{ t('help.empty') }}</p>
    <details v-for="article in filtered" :key="article.id"><summary>{{ article.title }}</summary><p v-for="(paragraph, index) in article.body.split('\n')" :key="index">{{ paragraph }}</p></details>
  </section>
</template>
<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ArrowLeft } from '@lucide/vue';
import { desktopHelp } from '../content/help';
import { translator, type Locale } from '../i18n';
const props = defineProps<{ locale: Locale }>();
defineEmits<{ back: [] }>();
const query = ref('');
const category = ref('all');
const { t } = translator(() => props.locale);
const en = computed(() => props.locale === 'en');
const articles = computed(() => desktopHelp.map(item => ({ id: item.id, category: en.value ? item.categoryEn : item.category, title: en.value ? item.titleEn : item.title, body: en.value ? item.bodyEn : item.body })));
const topics = computed(() => ['all', ...new Set(articles.value.map(item => item.category))]);
const filtered = computed(() => articles.value.filter(item => (category.value === 'all' || category.value === item.category) && `${item.title} ${item.body}`.toLocaleLowerCase().includes(query.value.trim().toLocaleLowerCase())));
watch(() => props.locale, () => { category.value = 'all'; query.value = ''; });
</script>
<style scoped>
.desktop-help{min-width:0}.help-back{display:inline-flex;align-items:center;gap:8px;margin-bottom:24px}.help-search{display:grid;gap:8px;max-width:560px;font-weight:600}.help-search input{width:100%;min-width:0;appearance:none;border:1px solid var(--border);border-radius:9px;background:var(--bg);color:var(--text);font:inherit;font-weight:400;min-height:40px;padding:9px 12px}.help-search input:focus-visible{outline:2px solid var(--focus);outline-offset:2px}.help-back :deep(svg){transform:none}.help-topics{display:flex;flex-wrap:wrap;gap:8px;margin-top:18px}.help-topics button[aria-pressed=true]{border-color:var(--accent-text);color:var(--accent-text);background:var(--selected)}.help-count{color:var(--muted);font-size:13px;margin:18px 0}.desktop-help details{border-top:1px solid var(--line);padding-bottom:4px}.desktop-help summary{padding:18px 2px;font-weight:600;cursor:pointer;overflow-wrap:anywhere}.desktop-help details p{max-width:75ch;line-height:1.85;margin:0 0 18px;overflow-wrap:anywhere}.desktop-help summary:focus-visible{outline:2px solid var(--accent-text);outline-offset:2px}
</style>
