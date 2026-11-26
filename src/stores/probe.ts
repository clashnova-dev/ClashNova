import { defineStore, storeToRefs } from "pinia";
import { ref, watch, computed } from "vue";
import { msg, type Text } from "../i18n";
import { isNative } from "../lib/platform";
import { parseSingBoxProfile } from "../lib/profiles";
import { sortByLatency } from "../lib/node-order";
import { AUTOMATIC_NODE_ID, outboundTag } from "../lib/automatic-selection";
import { useCoreStore } from "./core";
import { useNodesStore } from "./nodes";
import { useSubscriptionsStore } from "./subscriptions";
import { useChainStore } from "./chain";
import { useLogsStore } from "./logs";
import { useConnectionStore } from "./connection";
import { type Bridge, type Tone, type SpeedSample, type AutomaticSelection, type ProbeState } from "../types";

// probe: latency probing, speed testing, chain testing, and automatic selection, all run through the isolated test core (mutually exclusive with batch testing).
export const useProbeStore = defineStore(
  "probe",
  () => {
    const core = useCoreStore();
    const { phase, connected, message } = storeToRefs(core);
    const nodesStore = useNodesStore();
    const { nodes, latencies, coldLatencies, warmedLatencies, selectedNode, nodeSort } = storeToRefs(nodesStore);
    const subs = useSubscriptionsStore();
    const { rawProfile, profileKind } = storeToRefs(subs);
    const chainStore = useChainStore();
    const { chainTesting, chainDelay, chainError } = storeToRefs(chainStore);
    const { addActivity } = useLogsStore();

    const testing = ref(false);
    const probeSummary = ref<Text>("");
    const speedTesting = ref(false);
    const speedProgressMbps = ref(0);
    const speedPing = ref<number | null>(null);
    const speedResult = ref<SpeedSample | null>(null);
    const speedUpProgressMbps = ref(0);
    const speedUpResult = ref<SpeedSample | null>(null);
    const speedError = ref<Text>("");
    const probeStates = ref<Record<string, ProbeState>>({});
    const probeErrors = ref<Record<string, Text>>({});
    const probeProgress = ref(0);
    const probeTotal = ref(0);
    const probeConcurrency = ref(8);
    const probeCancelling = ref(false);

    let activeProbeSession: string | null = null;
    let speedRun = 0;
    let chainRun = 0;
    let probeRun = 0;
    let automaticTimer: ReturnType<typeof setTimeout> | undefined;
    let automaticGeneration = 0;

    const automaticSelection = ref<AutomaticSelection | null>(null);

    // Keep rows in place while testing: the order uses the pre-run latencies and is re-sorted once when the run finishes, rather than jumping as each result arrives.
    const orderDelays = ref<Record<string, number>>({});
    watch(
      [latencies, testing],
      () => {
        if (!testing.value) orderDelays.value = { ...latencies.value };
      },
      { deep: true, immediate: true }
    );
    const sortedNodes = computed(() =>
      nodeSort.value === "latency" ? sortByLatency(nodes.value, orderDelays.value) : nodes.value
    );

    async function refreshAutomaticSelection() {
      const run = automaticGeneration;
      if (!isNative || phase.value !== "CONNECTED" || selectedNode.value !== AUTOMATIC_NODE_ID) return;
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const result = await invoke<{
          tag: string;
          delay?: number | null;
        } | null>("get_auto_selection");
        if (run === automaticGeneration) automaticSelection.value = result;
      } catch {
        if (run === automaticGeneration) automaticSelection.value = null;
      } finally {
        if (run === automaticGeneration) automaticTimer = setTimeout(refreshAutomaticSelection, 5000);
      }
    }
    watch([phase, selectedNode, rawProfile], () => {
      automaticGeneration++;
      clearTimeout(automaticTimer);
      automaticSelection.value = null;
      void refreshAutomaticSelection();
    });

    function delayTone(delay: number): Tone {
      return delay <= 100 ? "good" : delay <= 300 ? "fair" : "bad";
    }
    function latencyTone(id: string): Tone {
      if (id === AUTOMATIC_NODE_ID)
        return connected.value && automaticSelection.value?.delay ? delayTone(automaticSelection.value.delay) : "idle";
      const state = probeStates.value[id];
      if (state === "failed") return "bad";
      const value = latencies.value[id];
      return value === undefined || state === "queued" || state === "testing" ? "idle" : delayTone(value);
    }
    function latencyLabel(id: string) {
      if (id === AUTOMATIC_NODE_ID)
        return !connected.value
          ? "nodes.testedAfterConnect"
          : automaticSelection.value?.delay
            ? `${automaticSelection.value.delay} ms`
            : "nodes.testing";
      if (
        id === AUTOMATIC_NODE_ID
          ? !nodes.value.some(node => node.agentOnline)
          : !nodes.value.find(node => node.id === id)?.agentOnline
      )
        return "nodes.offline";
      const state = probeStates.value[id];
      if (state === "queued") return "nodes.queued";
      if (state === "testing") return "nodes.testing";
      if (state === "failed") return "nodes.timedOut";
      const value = latencies.value[id];
      return value === undefined ? "nodes.notTested" : `${value} ms`;
    }

    function cancelProbes() {
      const { bridge } = useConnectionStore();
      probeRun++;
      if (testing.value) probeCancelling.value = true;
      const session = activeProbeSession;
      if (session)
        void bridge()
          .then(client => client.endProbe(session))
          .catch(() => {});
    }

    async function testLatencies(only?: string[]) {
      const { bridge } = useConnectionStore();
      if (testing.value) {
        cancelProbes();
        probeSummary.value = "nodes.stoppingTest";
        return;
      }
      if (!isNative || !rawProfile.value) {
        message.value = "nodes.testNeedsDesktop";
        return;
      }
      const run = ++probeRun;
      const candidates = nodes.value.filter(node => node.agentOnline && (!only || only.includes(node.id)));
      if (!candidates.length) {
        probeSummary.value = "nodes.nothingToTest";
        return;
      }
      const external = profileKind.value === "external";
      const tagFor = (id: string) => (external ? id : `node-${id.replaceAll("-", "").slice(0, 8)}`);
      testing.value = true;
      probeCancelling.value = false;
      probeProgress.value = 0;
      probeTotal.value = candidates.length;
      probeStates.value = Object.fromEntries(candidates.map(node => [node.id, "queued"]));
      latencies.value = {};
      probeErrors.value = {};
      coldLatencies.value = {};
      warmedLatencies.value = {};
      probeSummary.value = "nodes.preparingTest";
      const started = performance.now();
      let session: string | null = null;
      let client: Bridge | null = null;
      try {
        const config = external ? JSON.stringify(parseSingBoxProfile(rawProfile.value).config) : rawProfile.value;
        client = await bridge();
        session = await client.beginProbe(
          config,
          candidates.map(node => tagFor(node.id))
        );
        activeProbeSession = session;
        if (run !== probeRun) return;
        let cursor = 0;
        // Up to eight concurrent. A freed slot immediately picks the next queued node, so one slow node doesn't hold up the whole batch or block interaction.
        const worker = async () => {
          while (run === probeRun && cursor < candidates.length) {
            const node = candidates[cursor++];
            probeStates.value[node.id] = "testing";
            try {
              const result = await client!.probeDelay(session!, tagFor(node.id));
              if (run !== probeRun) return;
              latencies.value[node.id] = result.delay;
              coldLatencies.value[node.id] = result.coldDelay;
              warmedLatencies.value[node.id] = result.warmed;
              probeStates.value[node.id] = "success";
            } catch (error) {
              if (run !== probeRun) return;
              probeStates.value[node.id] = "failed";
              probeErrors.value[node.id] = String(error);
            }
            probeProgress.value++;
            probeSummary.value = msg("nodes.testProgress", {
              done: probeProgress.value,
              total: candidates.length,
              count: Object.keys(latencies.value).length,
            });
          }
        };
        await Promise.all(
          Array.from(
            {
              length: Math.min(Math.max(1, Math.min(64, probeConcurrency.value)), candidates.length),
            },
            worker
          )
        );
        if (run === probeRun) {
          const seconds = ((performance.now() - started) / 1000).toFixed(1);
          const summary = msg("nodes.testSummary", {
            count: Object.keys(latencies.value).length,
            total: candidates.length,
            seconds,
          });
          probeSummary.value = summary;
          addActivity(msg("log.nodeTest", { summary }));
        }
      } catch (error) {
        if (run === probeRun) probeSummary.value = msg("nodes.testFailed", { error: String(error) });
      } finally {
        if (session && client) await client.endProbe(session).catch(() => {});
        activeProbeSession = null;
        testing.value = false;
        probeCancelling.value = false;
        for (const [id, status] of Object.entries(probeStates.value)) {
          if (status === "queued" || status === "testing") delete probeStates.value[id];
        }
      }
    }

    // End-to-end chain measurement: after applying the chain to the probe config, time one request through the exit outbound — the core routes it as entry → … → exit → target.
    async function testChain(ids: string[]) {
      const { bridge } = useConnectionStore();
      if (chainTesting.value) {
        chainRun++;
        const session = activeProbeSession;
        if (session)
          void bridge()
            .then(client => client.endProbe(session))
            .catch(() => {});
        chainTesting.value = false;
        return;
      }
      if (ids.length < 2) return;
      if (testing.value) {
        message.value = "nodes.testBusy";
        return;
      }
      if (!isNative || !rawProfile.value) {
        message.value = "nodes.testNeedsDesktop";
        return;
      }
      const external = profileKind.value === "external";
      const tagFor = (id: string) => (external ? id : `node-${id.replaceAll("-", "").slice(0, 8)}`);
      const run = ++chainRun;
      chainTesting.value = true;
      chainDelay.value = null;
      chainError.value = "";
      let session: string | null = null;
      let client: Bridge | null = null;
      try {
        const cfg: any = external ? parseSingBoxProfile(rawProfile.value).config : JSON.parse(rawProfile.value);
        const tags = ids.map(tagFor);
        const byTag = new Map<string, any>((cfg.outbounds ?? []).map((o: any) => [o.tag, o]));
        if (!tags.every(tag => byTag.has(tag))) throw new Error("native.probeNodeNotFound");
        for (let i = 1; i < tags.length; i++) byTag.get(tags[i]).detour = tags[i - 1];
        const exitTag = tags[tags.length - 1];
        client = await bridge();
        session = await client.beginProbe(JSON.stringify(cfg), [exitTag]);
        activeProbeSession = session;
        if (run !== chainRun) return;
        const result = await client.probeDelay(session, exitTag);
        if (run !== chainRun) return;
        chainDelay.value = result.delay;
      } catch (error) {
        if (run === chainRun) chainError.value = String(error);
      } finally {
        if (session && client) await client.endProbe(session).catch(() => {});
        if (activeProbeSession === session) activeProbeSession = null;
        if (run === chainRun) chainTesting.value = false;
      }
    }
    // Past end-to-end results no longer describe the edited chain; clear them when the draft changes.
    function clearChainTest() {
      chainDelay.value = null;
      chainError.value = "";
    }

    // Speed test: for the given (default: currently selected) node, first measure latency once as the ping, then measure the actual download rate through the isolated test core.
    async function testSpeed(nodeId?: string) {
      const { bridge } = useConnectionStore();
      if (speedTesting.value) return;
      const external = profileKind.value === "external";
      const id =
        nodeId ||
        (selectedNode.value === AUTOMATIC_NODE_ID
          ? nodes.value.find(node => outboundTag(node.id, external) === automaticSelection.value?.tag)?.id
          : selectedNode.value);
      if (testing.value) {
        speedError.value = "nodes.testBusy";
        return;
      }
      if (!isNative || !rawProfile.value || !id) {
        speedError.value = "nodes.testNeedsDesktop";
        return;
      }
      const tag = outboundTag(id, external);
      const run = ++speedRun;
      speedTesting.value = true;
      speedResult.value = null;
      speedError.value = "";
      speedProgressMbps.value = 0;
      speedPing.value = null;
      speedUpResult.value = null;
      speedUpProgressMbps.value = 0;
      let session: string | null = null;
      let client: Bridge | null = null;
      let stop: (() => void) | undefined;
      let stopUp: (() => void) | undefined;
      try {
        const config = external ? JSON.stringify(parseSingBoxProfile(rawProfile.value).config) : rawProfile.value;
        client = await bridge();
        stop = await client.onSpeedProgress(p => {
          if (run === speedRun && speedTesting.value) speedProgressMbps.value = p.mbps;
        });
        stopUp = await client.onSpeedUpProgress(p => {
          if (run === speedRun && speedTesting.value) speedUpProgressMbps.value = p.mbps;
        });
        session = await client.beginProbe(config, [tag]);
        activeProbeSession = session;
        if (run !== speedRun) return;
        // Measure latency first as the ping; failure is not fatal (the download result is authoritative), and the download will surface the real error.
        try {
          speedPing.value = (await client.probeDelay(session, tag)).delay;
        } catch {
          speedPing.value = null;
        }
        if (run !== speedRun) return;
        const result = await client.probeSpeed(session, tag);
        if (run !== speedRun) return;
        speedResult.value = result;
        speedProgressMbps.value = result.mbps;
        // After the download, measure upload next; an upload failure doesn't clear the download result already obtained, it just marks upload as unavailable.
        try {
          const up = await client.probeSpeedUp(session, tag);
          if (run !== speedRun) return;
          speedUpResult.value = up;
          speedUpProgressMbps.value = up.mbps;
        } catch {
          if (run === speedRun) speedUpResult.value = null;
        }
      } catch (error) {
        if (run === speedRun) speedError.value = String(error);
      } finally {
        stop?.();
        stopUp?.();
        if (session && client) await client.endProbe(session).catch(() => {});
        if (activeProbeSession === session) activeProbeSession = null;
        if (run === speedRun) speedTesting.value = false;
      }
    }
    // Close dialog / cancel: invalidate the current round and end the test core; the backend download loop exits as soon as it detects cancelled.
    function cancelSpeedTest() {
      const { bridge } = useConnectionStore();
      speedRun++;
      const session = activeProbeSession;
      if (session)
        void bridge()
          .then(client => client.endProbe(session))
          .catch(() => {});
      speedTesting.value = false;
      speedProgressMbps.value = 0;
      speedUpProgressMbps.value = 0;
    }

    function teardown() {
      automaticGeneration++;
      clearTimeout(automaticTimer);
      cancelProbes();
    }

    return {
      testing,
      probeSummary,
      speedTesting,
      speedProgressMbps,
      speedPing,
      speedResult,
      speedUpProgressMbps,
      speedUpResult,
      speedError,
      probeStates,
      probeErrors,
      probeProgress,
      probeTotal,
      probeConcurrency,
      probeCancelling,
      automaticSelection,
      sortedNodes,
      refreshAutomaticSelection,
      delayTone,
      latencyTone,
      latencyLabel,
      cancelProbes,
      testLatencies,
      testChain,
      clearChainTest,
      testSpeed,
      cancelSpeedTest,
      teardown,
    };
  },
  { persist: { key: "airport.probe", pick: ["probeConcurrency"] } }
);
