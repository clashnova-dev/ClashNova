import { defineStore, storeToRefs } from "pinia";
import { ref } from "vue";
import { msg, type Text } from "../i18n";
import { createAuthenticatedApi } from "../lib/authenticated-api";
import { readClientSetting, writeClientSetting } from "../lib/settings-store";
import { SESSION_KEY } from "../constants/storage";
import { dismissNotice } from "../lib/notifications";
import { enableLogin } from "../constants/flags";
import { useCoreStore } from "./core";
import { useLogsStore } from "./logs";
import { useNodesStore } from "./nodes";
import { useChainStore } from "./chain";
import { useProbeStore } from "./probe";
import { useSubscriptionsStore } from "./subscriptions";
import { useConnectionStore } from "./connection";
import { useUiStore } from "./ui";

// Login address comes from the build (VITE_DEFAULT_SERVER) or a value saved earlier on this machine; it is no longer entered at login time.
export const defaultServer = (import.meta.env.VITE_DEFAULT_SERVER ?? "").trim();

// account: account / login and session. Backend-dependent actions (login, logout, refresh data, session restore) live here.
export const useAccountStore = defineStore(
  "account",
  () => {
    const baseUrl = ref(defaultServer);
    const accountMenu = ref(false);
    const localMode = ref(false);
    const rememberMe = ref(false);
    const email = ref("");
    const password = ref("");
    const access = ref("");
    const refreshToken = ref("");
    const loginError = ref<Text>("");

    let sessionRevision = 0;
    const api = createAuthenticatedApi({
      baseUrl: () => baseUrl.value,
      session: () => ({
        accessToken: access.value,
        refreshToken: refreshToken.value,
        revision: sessionRevision,
      }),
      replaceSession: tokens => {
        access.value = tokens.accessToken;
        refreshToken.value = tokens.refreshToken;
      },
      persistSession,
    });

    async function login() {
      const core = useCoreStore();
      const { busy, message } = storeToRefs(core);
      const { failure } = core;
      const { addActivity } = useLogsStore();
      const { tab } = storeToRefs(useUiStore());
      if (!baseUrl.value) {
        loginError.value = "error.noServer";
        return;
      }
      busy.value = true;
      loginError.value = "";
      try {
        const data = await api("/api/v1/app/auth/login", {
          method: "POST",
          body: JSON.stringify({ email: email.value, password: password.value }),
        });
        sessionRevision++;
        access.value = data.tokens.accessToken;
        refreshToken.value = data.tokens.refreshToken;
        await persistSession();
        password.value = "";
        dismissNotice();
        addActivity("log.signedIn");
        tab.value = "home";
        await refreshData();
      } catch (error) {
        if (!access.value) loginError.value = failure(error, "error.signInFailed");
        else message.value = failure(error, "error.subscriptionLoadFailed");
      } finally {
        busy.value = false;
      }
    }

    async function refreshData() {
      const core = useCoreStore();
      const { busy, message } = storeToRefs(core);
      const { showError, failure } = core;
      const subs = useSubscriptionsStore();
      const { activeSub, subUrl, rawProfile, profileKind, activeExternalId } = storeToRefs(subs);
      const { importSub } = subs;
      const nodesStore = useNodesStore();
      const { nodes, latencies, selectedNode } = storeToRefs(nodesStore);
      const probe = useProbeStore();
      const { probeStates } = storeToRefs(probe);
      const { cancelProbes } = probe;
      const chainStore = useChainStore();
      const { chain, chainGroups } = storeToRefs(chainStore);
      const connection = useConnectionStore();
      const { cachedNode, cachedChain, cachedChainGroups } = connection;
      const { addActivity } = useLogsStore();
      if (!access.value) return;
      busy.value = true;
      try {
        const [subsData, available] = await Promise.all([api("/api/v1/app/subscriptions"), api("/api/v1/app/nodes")]);
        activeSub.value =
          subsData.find(
            (s: any) => s.status === "active" && s.expiresAt && new Date(s.expiresAt).getTime() > Date.now()
          ) || null;
        nodes.value = available;
        profileKind.value = "airport";
        activeExternalId.value = "";
        addActivity(
          msg("log.nodesLoaded", {
            count: available.length,
            online: available.filter((node: (typeof nodes.value)[number]) => node.agentOnline).length,
          })
        );
        cancelProbes();
        probeStates.value = {};
        latencies.value = {};
        selectedNode.value = cachedNode(nodes.value);
        chain.value = cachedChain(nodes.value);
        chainGroups.value = cachedChainGroups();
        subUrl.value = activeSub.value?.subscriptionUrl || "";
        if (subUrl.value && selectedNode.value) await importSub();
        else if (subUrl.value) {
          rawProfile.value = "";
          message.value = "subscriptions.noReadyNodes";
        } else {
          rawProfile.value = "";
          message.value = "subscriptions.nonePurchase";
        }
      } catch (error) {
        showError(failure(error, "error.loadFailed"));
      } finally {
        busy.value = false;
      }
    }

    async function logout() {
      const core = useCoreStore();
      const { busy, message, phase } = storeToRefs(core);
      const { showError, failure } = core;
      const { bridge } = useConnectionStore();
      const subs = useSubscriptionsStore();
      const { subUrl, rawProfile, activeSub, activeExternalId, profileKind } = storeToRefs(subs);
      const nodesStore = useNodesStore();
      const { nodes, selectedNode, latencies } = storeToRefs(nodesStore);
      const chainStore = useChainStore();
      const { chain, chainGroups } = storeToRefs(chainStore);
      const probe = useProbeStore();
      const { probeStates } = storeToRefs(probe);
      const { cancelProbes } = probe;
      const { activityLogs } = storeToRefs(useLogsStore());
      sessionRevision++;
      busy.value = true;
      try {
        await (await bridge()).clearProfile();
      } catch (error) {
        message.value = msg("error.clearProfileFailed", { error: String(error) });
        busy.value = false;
        return;
      }
      try {
        await writeClientSetting("session", { refreshToken: "" }, SESSION_KEY);
      } catch (error) {
        showError(failure(error, "error.preferencesNotSaved"));
        busy.value = false;
        return;
      }
      access.value = "";
      refreshToken.value = "";
      password.value = "";
      subUrl.value = "";
      dismissNotice();
      rawProfile.value = "";
      activeSub.value = null;
      nodes.value = [];
      selectedNode.value = "";
      chain.value = [];
      chainGroups.value = [];
      cancelProbes();
      probeStates.value = {};
      localMode.value = false;
      activeExternalId.value = "";
      profileKind.value = "airport";
      phase.value = "IDLE";
      loginError.value = "";
      latencies.value = {};
      activityLogs.value = [];
      busy.value = false;
    }

    // Optional "remember me": persist only the refresh token (never the password) and silently renew on restart. Off by default to keep a safe default.
    async function persistSession() {
      await writeClientSetting("session", { refreshToken: rememberMe.value ? refreshToken.value : "" }, SESSION_KEY);
    }
    const acceptSession = (value: unknown): { refreshToken: string } | null => {
      const item = value as { refreshToken?: unknown } | null;
      return typeof item?.refreshToken === "string" && item.refreshToken.length <= 16384
        ? { refreshToken: item.refreshToken }
        : null;
    };
    async function restoreSession() {
      const core = useCoreStore();
      const { showError, failure } = core;
      const { activeExternalId } = storeToRefs(useSubscriptionsStore());
      const { tab } = storeToRefs(useUiStore());
      let saved: { refreshToken: string };
      try {
        saved = await readClientSetting("session", { refreshToken: "" }, acceptSession, {
          key: SESSION_KEY,
          parse: raw => {
            try {
              return acceptSession(JSON.parse(raw));
            } catch {
              return null;
            }
          },
        });
      } catch (error) {
        showError(failure(error, "error.preferencesNotSaved"));
        return;
      }
      if (!enableLogin || access.value || activeExternalId.value || !saved.refreshToken || !baseUrl.value) return;
      rememberMe.value = true;
      try {
        const res = await fetch(`${baseUrl.value.replace(/\/$/, "")}/api/v1/app/auth/refresh`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ refreshToken: saved.refreshToken }),
        });
        if (!res.ok) {
          await writeClientSetting("session", { refreshToken: "" }, SESSION_KEY);
          return;
        }
        const body = await res.json();
        sessionRevision++;
        access.value = body.data.accessToken;
        refreshToken.value = body.data.refreshToken;
        await persistSession();
        tab.value = "home";
        await refreshData();
      } catch {
        /* On refresh failure, stay on the login screen. */
      }
    }

    function enterLocal() {
      const { message } = storeToRefs(useCoreStore());
      const { tab } = storeToRefs(useUiStore());
      localMode.value = true;
      tab.value = "sub";
      message.value = "subscriptions.addPrompt";
    }

    return {
      baseUrl,
      accountMenu,
      localMode,
      rememberMe,
      email,
      password,
      access,
      refreshToken,
      loginError,
      api,
      login,
      logout,
      refreshData,
      persistSession,
      restoreSession,
      enterLocal,
    };
  },
  { persist: { key: "airport.account", pick: ["baseUrl", "email"] } }
);
