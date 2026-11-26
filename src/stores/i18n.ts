import { defineStore } from "pinia";
import { ref } from "vue";
import { translator, type Locale } from "../i18n";

// i18n: current language and translation functions, shared by all components/pages.
// Replaces the duplicate implementations formerly inside useApp and in each component's translator(() => props.locale).
export const useI18nStore = defineStore(
  "i18n",
  () => {
    const locale = ref<Locale>("en");
    const { t, tx } = translator(() => locale.value);
    return { locale, t, tx };
  },
  { persist: { key: "airport.i18n", pick: ["locale"] } }
);
