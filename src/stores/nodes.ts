import { defineStore } from "pinia";
import { ref } from "vue";
import { type Node } from "../types";

// nodes: core data for the node list, latencies, and selection.
export const useNodesStore = defineStore(
  "nodes",
  () => {
    const nodes = ref<Node[]>([]);
    const latencies = ref<Record<string, number>>({});
    const coldLatencies = ref<Record<string, number>>({});
    const warmedLatencies = ref<Record<string, boolean>>({});
    const nodeSort = ref<"default" | "latency">("default");
    const selectedNode = ref("");
    return { nodes, latencies, coldLatencies, warmedLatencies, nodeSort, selectedNode };
  },
  { persist: { key: "airport.nodes", pick: ["nodeSort"] } }
);
