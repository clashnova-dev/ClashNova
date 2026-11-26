import { defineStore, storeToRefs } from "pinia";
import { ref, computed } from "vue";
import { type Text } from "../i18n";
import { type ChainGroup } from "../types";
import { useNodesStore } from "./nodes";
import { useConnectionStore } from "./connection";

// chain: proxy chain (entry → exit node sequence), chain presets, and chain-test state.
// Types moved to ../types/chain; re-exported here for compatibility with older references that import the type from the store.
export type { ChainGroup };

export const useChainStore = defineStore("chain", () => {
  const { nodes } = storeToRefs(useNodesStore());
  const chain = ref<string[]>([]);
  const chainGroups = ref<ChainGroup[]>([]);
  const chainTesting = ref(false);
  const chainDelay = ref<number | null>(null);
  const chainError = ref<Text>("");

  const chainActive = computed(() => chain.value.length >= 2);
  const chainNames = computed(() => chain.value.map(id => nodes.value.find(n => n.id === id)?.name || id));
  const chainSummary = computed(() => chainNames.value.join(" → "));

  // Create or update a chain-group preset; returns the group id.
  function saveChainGroup(name: string, groupNodes: string[], id?: string): string {
    const { persistPreferences } = useConnectionStore();
    const gid =
      id && chainGroups.value.some(g => g.id === id)
        ? id
        : `g${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
    const existing = chainGroups.value.find(g => g.id === gid);
    if (existing) {
      existing.name = name;
      existing.nodes = [...groupNodes];
    } else chainGroups.value.push({ id: gid, name, nodes: [...groupNodes] });
    chainGroups.value = [...chainGroups.value];
    persistPreferences();
    return gid;
  }
  function deleteChainGroup(id: string) {
    const { persistPreferences } = useConnectionStore();
    chainGroups.value = chainGroups.value.filter(g => g.id !== id);
    persistPreferences();
  }

  return {
    chain,
    chainGroups,
    chainTesting,
    chainDelay,
    chainError,
    chainActive,
    chainNames,
    chainSummary,
    saveChainGroup,
    deleteChainGroup,
  };
});
