import { defineStore, storeToRefs } from "pinia";
import { ref, computed, watch, nextTick } from "vue";
import { type SubscriptionDownload, parseSubscriptionUsage } from "../lib/subscription-info";
import { type ExternalProfile, type ActiveSubscription } from "../types";
import { msg, type Text } from "../i18n";
import { confirmation } from "../lib/confirmation";
import { parseSingBoxProfile } from "../lib/profiles";
import { AUTOMATIC_NODE_ID } from "../lib/automatic-selection";
import { parseDeepLink, redeemedSubscription } from "../lib/deep-link";
import { useCoreStore } from "./core";
import { useI18nStore } from "./i18n";
import { useAccountStore } from "./account";
import { useNodesStore } from "./nodes";
import { useChainStore } from "./chain";
import { useProbeStore } from "./probe";
import { useLogsStore } from "./logs";
import { useWindowStore } from "./window";
import { useUiStore } from "./ui";
import { useConnectionStore } from "./connection";

// Types moved to ../types/subscription; re-exported here for compatibility with older references that import the type from the store.
export type { ExternalProfile };

// subscriptions: core subscription/profile data plus CRUD, import, switching, dialogs, etc.
export const useSubscriptionsStore = defineStore(
  "subscriptions",
  () => {
    const externalProfiles = ref<ExternalProfile[]>([]);
    const activeExternalId = ref("");
    const newProfileName = ref("");
    const newProfileUrl = ref("");
    const selectedFile = ref<File | null>(null);
    const subUrl = ref("");
    const rawProfile = ref("");
    const profileKind = ref<"airport" | "external">("airport");
    const profileError = ref<Text>("");
    const importError = ref<Text>("");
    const importRequest = ref<{ code: string } | { url: string; name: string } | null>(null);
    const profileDialog = ref<HTMLDialogElement | null>(null);
    const importDialog = ref<HTMLDialogElement | null>(null);
    const profileNameInput = ref<HTMLInputElement | null>(null);
    const activeSub = ref<ActiveSubscription | null>(null);
    const currentExternal = computed(() => externalProfiles.value.find(item => item.id === activeExternalId.value));
    // On a clash:// import, show the subscription host so the user can clearly see what they're confirming.
    const importTarget = computed(() => {
      const request = importRequest.value;
      if (!request || !("url" in request)) return null;
      let host = request.url;
      try {
        host = new URL(request.url).host;
      } catch {
        /* fall back to the raw address */
      }
      return { name: request.name, host };
    });
    const profileLabel = computed(() =>
      profileKind.value === "external"
        ? currentExternal.value?.name || "subscriptions.generic"
        : activeSub.value?.planName || "overview.noActiveSubscription"
    );

    async function normalizeExternalRaw(raw: string): Promise<string> {
      const { bridge } = useConnectionStore();
      try {
        parseSingBoxProfile(raw);
        return raw;
      } catch {
        const converted = await (await bridge()).convertSubscription(raw);
        parseSingBoxProfile(converted);
        return converted;
      }
    }

    async function persistExternal(profiles = externalProfiles.value) {
      const { bridge } = useConnectionStore();
      await (await bridge()).saveSubscriptions(JSON.stringify(profiles));
    }

    async function activateExternal(profile: ExternalProfile) {
      const connection = useConnectionStore();
      const { bridge, applySnapshot, buildProfileConfig, cachedNode, cachedChain, cachedChainGroups } = connection;
      const core = useCoreStore();
      const { connected, message } = storeToRefs(core);
      const { localMode } = storeToRefs(useAccountStore());
      const { nodes, selectedNode, latencies } = storeToRefs(useNodesStore());
      const { chain, chainGroups } = storeToRefs(useChainStore());
      const probe = useProbeStore();
      const { probeStates, probeSummary } = storeToRefs(probe);
      const { cancelProbes } = probe;
      const { addActivity } = useLogsStore();
      const parsed = parseSingBoxProfile(profile.raw);
      const client = await bridge();
      if (connected.value) applySnapshot(await client.disconnect());
      localMode.value = true;
      profileKind.value = "external";
      activeExternalId.value = profile.id;
      rawProfile.value = profile.raw;
      subUrl.value = profile.url;
      activeSub.value = null;
      nodes.value = parsed.nodes;
      selectedNode.value = cachedNode(parsed.nodes);
      chain.value = cachedChain(parsed.nodes);
      chainGroups.value = cachedChainGroups();
      latencies.value = {};
      probeStates.value = {};
      probeSummary.value = "";
      cancelProbes();
      await client.saveProfile(buildProfileConfig());
      message.value = msg("subscriptions.nowUsing", { name: profile.name });
      addActivity(msg("log.subscriptionSwitched", { name: profile.name }));
    }

    async function selectProfile(profile: ExternalProfile) {
      const { busy, message } = storeToRefs(useCoreStore());
      const { connected } = storeToRefs(useCoreStore());
      if (busy.value) return;
      busy.value = true;
      try {
        if (
          connected.value &&
          !(await confirmation.request({
            title: "confirmation.switchTitle",
            body: msg("confirmation.switchBody", { name: profile.name }),
            confirmLabel: "subscriptions.useSubscription",
          }))
        )
          return;
        await activateExternal(profile);
      } catch (error) {
        message.value = msg("error.subscriptionSwitchFailed", {
          error: String(error),
        });
      } finally {
        busy.value = false;
      }
    }
    function resetProfileDialog() {
      newProfileName.value = "";
      newProfileUrl.value = "";
      selectedFile.value = null;
      profileError.value = "";
      profileDialog.value?.querySelector("form")?.reset();
    }
    async function openProfileDialog() {
      profileError.value = "";
      profileDialog.value?.showModal();
      await nextTick();
      profileNameInput.value?.focus();
    }
    function closeProfileDialog() {
      const { busy } = storeToRefs(useCoreStore());
      if (!busy.value) profileDialog.value?.close();
    }

    async function acceptDeepLinks(urls: string[] | null) {
      const { showError } = useCoreStore();
      const { revealMain } = useWindowStore();
      const { tab } = storeToRefs(useUiStore());
      if (!urls?.length) return;
      await revealMain();
      for (const raw of urls) {
        const link = parseDeepLink(raw);
        if (!link) continue;
        if (link.kind === "nodes" || link.kind === "settings") tab.value = link.kind;
        else if (link.kind === "connect") tab.value = "connections";
        else if (link.kind === "rejected") showError("deepLink.rejected");
        else if (link.kind === "import") {
          importError.value = "";
          tab.value = "sub";
          importRequest.value = { code: link.code };
        } else if (link.kind === "importUrl") {
          importError.value = "";
          tab.value = "sub";
          importRequest.value = { url: link.url, name: link.name };
        }
      }
    }

    function dismissImport() {
      const { busy } = storeToRefs(useCoreStore());
      if (busy.value) return;
      importRequest.value = null;
      importError.value = "";
    }

    async function confirmImport() {
      const { busy } = storeToRefs(useCoreStore());
      const { showError } = useCoreStore();
      const { t } = useI18nStore();
      const { baseUrl, api } = useAccountStore();
      const request = importRequest.value;
      if (!request || busy.value) return;
      if (!("url" in request) && !baseUrl) {
        importError.value = "error.noServer";
        return;
      }
      importError.value = "";
      try {
        if ("url" in request) {
          // The clash:// link carries the address directly; no code to redeem.
          newProfileName.value = request.name || t("deepLink.importedName");
          newProfileUrl.value = request.url;
        } else {
          busy.value = true;
          const redeemed = redeemedSubscription(
            await api("/api/v1/app/subscription-import-codes/redeem", {
              method: "POST",
              body: JSON.stringify({ code: request.code }),
            })
          );
          busy.value = false;
          if (!redeemed) throw new Error("deepLink.importFailed");
          newProfileName.value = redeemed.name || t("deepLink.importedName");
          newProfileUrl.value = redeemed.url;
        }
        selectedFile.value = null;
        await addExternal();
        if (profileError.value) {
          newProfileName.value = "";
          newProfileUrl.value = "";
          showError("deepLink.importFailed");
        }
      } catch {
        showError("deepLink.importFailed");
      } finally {
        busy.value = false;
        importRequest.value = null;
      }
    }

    async function addExternal() {
      const connection = useConnectionStore();
      const { bridge } = connection;
      const core = useCoreStore();
      const { busy, connected } = storeToRefs(core);
      const { tab } = storeToRefs(useUiStore());
      if (busy.value) return;
      const name = newProfileName.value.trim();
      const url = newProfileUrl.value.trim();
      if (!name || (!url && !selectedFile.value)) {
        profileError.value = "subscriptions.formIncomplete";
        return;
      }
      busy.value = true;
      profileError.value = "";
      try {
        // Deep-link imports already have their own explicit confirmation dialog.
        if (
          connected.value &&
          !importRequest.value &&
          !(await confirmation.request({
            title: "confirmation.switchTitle",
            body: msg("confirmation.switchBody", { name }),
            confirmLabel: "app.importUse",
          }))
        )
          return;
        const downloaded: SubscriptionDownload = selectedFile.value
          ? { raw: await selectedFile.value.text(), userinfo: null }
          : await (await bridge()).fetchSubscriptionDetails(url);
        const raw = await normalizeExternalRaw(downloaded.raw);
        const profile = {
          id: crypto.randomUUID(),
          name,
          url: selectedFile.value ? "" : url,
          raw,
          updatedAt: new Date().toISOString(),
          usage: parseSubscriptionUsage(downloaded.userinfo),
        };
        const profiles = [...externalProfiles.value, profile];
        await persistExternal(profiles);
        externalProfiles.value = profiles;
        newProfileName.value = "";
        newProfileUrl.value = "";
        selectedFile.value = null;
        await activateExternal(profile);
        profileDialog.value?.close();
        tab.value = "sub";
      } catch (error) {
        profileError.value = error instanceof Error ? error.message : String(error);
      } finally {
        busy.value = false;
      }
    }

    function chooseFile(event: Event) {
      selectedFile.value = (event.target as HTMLInputElement).files?.[0] ?? null;
    }

    async function refreshExternal(profile: ExternalProfile) {
      const { bridge } = useConnectionStore();
      const core = useCoreStore();
      const { busy, connected, message } = storeToRefs(core);
      if (!profile.url || busy.value) return;
      busy.value = true;
      try {
        if (
          connected.value &&
          activeExternalId.value === profile.id &&
          !(await confirmation.request({
            title: "confirmation.refreshTitle",
            body: "confirmation.refreshBody",
            confirmLabel: "subscriptions.updateSubscription",
          }))
        )
          return;
        const downloaded = await (await bridge()).fetchSubscriptionDetails(profile.url);
        const raw = await normalizeExternalRaw(downloaded.raw);
        const updated = {
          ...profile,
          raw,
          usage: parseSubscriptionUsage(downloaded.userinfo),
          updatedAt: new Date().toISOString(),
        };
        const profiles = externalProfiles.value.map(item => (item.id === profile.id ? updated : item));
        await persistExternal(profiles);
        externalProfiles.value = profiles;
        if (activeExternalId.value === profile.id) await activateExternal(updated);
        message.value =
          downloaded.metadataSupported === false
            ? "subscriptions.updatedRestartForUsage"
            : msg("subscriptions.updatedNamed", { name: profile.name });
      } catch (error) {
        message.value = error instanceof Error ? error.message : String(error);
      } finally {
        busy.value = false;
      }
    }

    async function removeExternal(profile: ExternalProfile) {
      const connection = useConnectionStore();
      const { bridge, applySnapshot, persistPreferences, dropSelectedNode } = connection;
      const core = useCoreStore();
      const { busy, connected, message } = storeToRefs(core);
      const { t } = useI18nStore();
      const { nodes, selectedNode } = storeToRefs(useNodesStore());
      const { chain, chainGroups } = storeToRefs(useChainStore());
      const probe = useProbeStore();
      const { probeStates } = storeToRefs(probe);
      const { cancelProbes } = probe;
      if (busy.value) return;
      busy.value = true;
      try {
        if (
          !(await confirmation.request({
            title: "confirmation.removeTitle",
            body: msg("confirmation.removeBody", { name: profile.name }),
            confirmLabel: "subscriptions.removeSubscription",
            danger: true,
            details: activeExternalId.value === profile.id ? ["confirmation.activeSubscription"] : [],
          }))
        )
          return;
        const client = await bridge();
        if (activeExternalId.value === profile.id) {
          if (connected.value) applySnapshot(await client.disconnect());
          await client.clearProfile();
          cancelProbes();
          probeStates.value = {};
          activeExternalId.value = "";
          rawProfile.value = "";
          nodes.value = [];
          selectedNode.value = "";
          chain.value = [];
          chainGroups.value = [];
          profileKind.value = "airport";
        }
        const remaining = externalProfiles.value.filter(item => item.id !== profile.id);
        await client.saveSubscriptions(JSON.stringify(remaining));
        externalProfiles.value = remaining;
        dropSelectedNode(profile.id);
        persistPreferences();
      } catch (error) {
        message.value = t("error.subscriptionRemoveFailed", {
          error: String(error),
        });
      } finally {
        busy.value = false;
      }
    }

    // Subscription editing: treat the edited text as the new subscription content — first normalize (sing-box JSON passes through, YAML/share links go through the converter)
    // and validate, then update the locally saved raw; if this subscription is in use, re-activate it to refresh the nodes and push the config again.
    async function saveExternalEdit(id: string, edited: string) {
      const { busy } = storeToRefs(useCoreStore());
      if (busy.value) throw new Error("native.connectionBusy");
      const profile = externalProfiles.value.find(item => item.id === id);
      if (!profile) throw new Error("error.subscriptionGone");
      busy.value = true;
      try {
        const raw = await normalizeExternalRaw(edited.trim());
        const updated = { ...profile, raw, updatedAt: new Date().toISOString() };
        const profiles = externalProfiles.value.map(item => (item.id === id ? updated : item));
        await persistExternal(profiles);
        externalProfiles.value = profiles;
        if (activeExternalId.value === id) await activateExternal(updated);
      } finally {
        busy.value = false;
      }
    }

    // Edit subscription info: rename and change source. When the source URL changes and is non-empty, re-fetch the subscription and validate/replace the nodes; renaming alone only updates metadata.
    async function saveExternalInfo(id: string, name: string, url: string) {
      const { bridge } = useConnectionStore();
      const { busy } = storeToRefs(useCoreStore());
      if (busy.value) throw new Error("native.connectionBusy");
      const profile = externalProfiles.value.find(item => item.id === id);
      if (!profile) throw new Error("error.subscriptionGone");
      const nextName = name.trim();
      const nextUrl = url.trim();
      if (!nextName) throw new Error("subscriptions.formIncomplete");
      busy.value = true;
      try {
        let { raw, usage, updatedAt } = profile;
        if (nextUrl && nextUrl !== profile.url) {
          const downloaded = await (await bridge()).fetchSubscriptionDetails(nextUrl);
          raw = await normalizeExternalRaw(downloaded.raw);
          usage = parseSubscriptionUsage(downloaded.userinfo);
          updatedAt = new Date().toISOString();
        }
        const updated = {
          ...profile,
          name: nextName,
          url: nextUrl,
          raw,
          usage,
          updatedAt,
        };
        const profiles = externalProfiles.value.map(item => (item.id === id ? updated : item));
        await persistExternal(profiles);
        externalProfiles.value = profiles;
        if (activeExternalId.value === id && raw !== profile.raw) await activateExternal(updated);
      } finally {
        busy.value = false;
      }
    }

    // Edit nodes: keptTags = tags of nodes to keep (those left after deletions in the list); add = pasted URI/Base64 text (converted into nodes and appended).
    async function saveExternalNodes(id: string, keptTags: string[], add: string) {
      const { busy } = storeToRefs(useCoreStore());
      if (busy.value) throw new Error("native.connectionBusy");
      const profile = externalProfiles.value.find(item => item.id === id);
      if (!profile) throw new Error("error.subscriptionGone");
      busy.value = true;
      try {
        const isNode = (o: any) =>
          o && typeof o === "object" && typeof o.server === "string" && o.server && Number.isInteger(o.server_port);
        let added: unknown[] = [];
        const text = add.trim();
        if (text) {
          const converted = JSON.parse(await normalizeExternalRaw(text));
          added = Array.isArray(converted?.outbounds) ? converted.outbounds.filter(isNode) : [];
          if (!added.length) throw new Error("error.noNodesAdded");
        }
        const config = JSON.parse(profile.raw);
        const outbounds: any[] = Array.isArray(config.outbounds) ? config.outbounds : [];
        const kept = outbounds.filter(o => isNode(o) && keptTags.includes(o.tag));
        const others = outbounds.filter(o => !isNode(o));
        config.outbounds = [...kept, ...added, ...others];
        const raw = await normalizeExternalRaw(JSON.stringify(config));
        const updated = { ...profile, raw, updatedAt: new Date().toISOString() };
        const profiles = externalProfiles.value.map(item => (item.id === id ? updated : item));
        await persistExternal(profiles);
        externalProfiles.value = profiles;
        if (activeExternalId.value === id) await activateExternal(updated);
      } finally {
        busy.value = false;
      }
    }

    // Edit rules: text is the JSON array for this subscription's own route.rules. Only route.rules is replaced; the rest of the config is left untouched.
    async function saveExternalRules(id: string, text: string) {
      const { busy } = storeToRefs(useCoreStore());
      if (busy.value) throw new Error("native.connectionBusy");
      const profile = externalProfiles.value.find(item => item.id === id);
      if (!profile) throw new Error("error.subscriptionGone");
      let rules: unknown;
      try {
        rules = JSON.parse(text);
      } catch {
        throw new Error("error.rulesNotJson");
      }
      if (!Array.isArray(rules)) throw new Error("error.rulesNotArray");
      busy.value = true;
      try {
        const config = JSON.parse(profile.raw);
        config.route = config.route && typeof config.route === "object" ? config.route : {};
        config.route.rules = rules;
        const raw = await normalizeExternalRaw(JSON.stringify(config));
        const updated = { ...profile, raw, updatedAt: new Date().toISOString() };
        const profiles = externalProfiles.value.map(item => (item.id === id ? updated : item));
        await persistExternal(profiles);
        externalProfiles.value = profiles;
        if (activeExternalId.value === id) await activateExternal(updated);
      } finally {
        busy.value = false;
      }
    }

    async function importSub() {
      const connection = useConnectionStore();
      const { bridge, buildProfileConfig } = connection;
      const core = useCoreStore();
      const { busy, message } = storeToRefs(core);
      const { failure } = core;
      const { logs } = storeToRefs(useLogsStore());
      const { addActivity } = useLogsStore();
      if (!subUrl.value) return;
      busy.value = true;
      try {
        rawProfile.value = await (await bridge()).fetchSubscription(subUrl.value);
        await (await bridge()).saveProfile(buildProfileConfig());
        message.value = "subscriptions.imported";
        addActivity("log.subscriptionImported");
        logs.value = await (await bridge()).logs();
      } catch (error) {
        message.value = failure(error, "error.importFailed");
      } finally {
        busy.value = false;
      }
    }

    async function copySubscription() {
      const { message } = storeToRefs(useCoreStore());
      const url = subUrl.value;
      if (
        !url ||
        !(await confirmation.request({
          title: "confirmation.exposeTitle",
          body: "confirmation.exposeBody",
          confirmLabel: "subscriptions.copySubscriptionUrl",
        }))
      )
        return;
      if (subUrl.value !== url) return;
      try {
        await navigator.clipboard.writeText(url);
        message.value = "subscriptions.urlCopied";
      } catch {
        message.value = "error.copyFailed";
      }
    }

    watch(importRequest, async value => {
      await nextTick();
      const dialog = importDialog.value;
      if (!dialog) return;
      if (value) {
        if (!dialog.open) dialog.showModal();
      } else if (dialog.open) dialog.close();
    });

    // ——— 局域网跨端同步 ———
    // 采集本机「自建订阅 + 当前选择」为统一 payload（跨端 schema，详见计划）。
    function buildLanSyncPayload(): string {
      const { selectedNode } = storeToRefs(useNodesStore());
      const { chain } = storeToRefs(useChainStore());
      const active = externalProfiles.value.find(item => item.id === activeExternalId.value);
      return JSON.stringify({
        v: 1,
        subs: externalProfiles.value.map(p => ({ name: p.name, url: p.url, content: p.raw })),
        sel: {
          activeUrl: active?.url || null,
          activeName: active?.name || null,
          node: selectedNode.value || null,
          chain: [...(chain.value ?? [])],
        },
      });
    }

    // 应用收到的 payload：按 url(无 url 则 name) 合并订阅，再 best-effort 应用选择。返回合并条数。
    async function applyLanSyncPayload(payload: string): Promise<number> {
      const data = JSON.parse(payload);
      if (!data || data.v !== 1 || !Array.isArray(data.subs)) {
        throw new Error("lan.syncBadPayload");
      }
      const keyOf = (s: { url?: string; name?: string }) =>
        s.url && s.url.length ? `u:${s.url}` : `n:${s.name ?? ""}`;
      const byKey = new Map(externalProfiles.value.map(p => [keyOf(p), { ...p }]));
      let count = 0;
      for (const s of data.subs) {
        if (!s || typeof s.content !== "string" || !s.content) continue;
        let raw: string;
        try {
          raw = await normalizeExternalRaw(s.content);
        } catch {
          continue; // 跳过无法解析/转换的订阅
        }
        const existing = byKey.get(keyOf(s));
        if (existing) {
          existing.name = s.name || existing.name;
          existing.raw = raw;
          existing.updatedAt = new Date().toISOString();
        } else {
          byKey.set(keyOf(s), {
            id: crypto.randomUUID(),
            name: s.name || s.url || "sub",
            url: s.url || "",
            raw,
            updatedAt: new Date().toISOString(),
          });
        }
        count++;
      }
      const merged = [...byKey.values()];
      await persistExternal(merged);
      externalProfiles.value = merged;

      // 应用选择：激活对应订阅，再按 payload 覆盖选中节点/链（best-effort，失败不影响已同步的订阅）。
      const sel = data.sel ?? {};
      const active =
        (sel.activeUrl && merged.find(p => p.url === sel.activeUrl)) ||
        (sel.activeName && merged.find(p => p.name === sel.activeName)) ||
        null;
      if (active) {
        await activateExternal(active);
        try {
          const { nodes, selectedNode } = storeToRefs(useNodesStore());
          const { chain } = storeToRefs(useChainStore());
          const ids = new Set(nodes.value.map(n => n.id));
          if (sel.node && (ids.has(sel.node) || sel.node === AUTOMATIC_NODE_ID)) {
            selectedNode.value = sel.node;
          }
          if (Array.isArray(sel.chain)) {
            chain.value = sel.chain.filter((t: unknown) => typeof t === "string" && ids.has(t));
          }
          const connection = useConnectionStore();
          await (await connection.bridge()).saveProfile(connection.buildProfileConfig());
        } catch {
          /* 覆盖选择失败：订阅已同步，忽略 */
        }
      }
      return count;
    }

    return {
      externalProfiles,
      activeExternalId,
      newProfileName,
      newProfileUrl,
      selectedFile,
      subUrl,
      rawProfile,
      profileKind,
      profileError,
      importError,
      importRequest,
      profileDialog,
      importDialog,
      profileNameInput,
      activeSub,
      currentExternal,
      importTarget,
      profileLabel,
      normalizeExternalRaw,
      persistExternal,
      activateExternal,
      selectProfile,
      resetProfileDialog,
      openProfileDialog,
      closeProfileDialog,
      acceptDeepLinks,
      dismissImport,
      confirmImport,
      addExternal,
      chooseFile,
      refreshExternal,
      removeExternal,
      saveExternalEdit,
      saveExternalInfo,
      saveExternalNodes,
      saveExternalRules,
      importSub,
      copySubscription,
      buildLanSyncPayload,
      applyLanSyncPayload,
    };
  },
  { persist: { key: "airport.subscriptions", pick: ["activeExternalId"] } }
);
