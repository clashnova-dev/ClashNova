import { storeToRefs } from "pinia";
import { regionCode } from "../lib/regions";
import { subscriptionHost } from "../lib/subscription-info";
import { AUTOMATIC_NODE_ID } from "../lib/automatic-selection";
import { isNative, isMac, isLinux } from "../lib/platform";
import { enableLogin } from "../constants/flags";
import { githubUrl } from "../constants/domains";
import { useI18nStore } from "../stores/i18n";
import { useCoreStore } from "../stores/core";
import { useLogsStore } from "../stores/logs";
import { useNodesStore } from "../stores/nodes";
import { useSubscriptionsStore } from "../stores/subscriptions";
import { useSettingsStore } from "../stores/settings";
import { useProbeStore } from "../stores/probe";
import { useAccountStore } from "../stores/account";
import { useUpdateStore } from "../stores/update";
import { useChainStore } from "../stores/chain";
import { useUiStore } from "../stores/ui";
import { useWindowStore } from "../stores/window";
import { useConnectionStore } from "../stores/connection";

// useApp: an aggregate facade over the Pinia stores. All state and methods have moved into the
// domain stores under ../stores/*; this only wires them up: it re-exposes each store's state/getters
// (via storeToRefs) and actions in the original return shape, so App.vue and the pages consume it unchanged.
// Only the members the UI actually consumes are forwarded; store-internal helpers stay in their store.
export function create() {
  const i18n = useI18nStore();
  const { locale } = storeToRefs(i18n);
  const { t, tx } = i18n;

  const core = useCoreStore();
  const {
    phase,
    message,
    busy,
    connectionAction,
    connectionProgress,
    guardBlocking,
    guardArmed,
    connectionMode,
    connected,
  } = storeToRefs(core);

  const logsStore = useLogsStore();
  const { logs, activityLogs, logViewport, followLogs, logsLoading, logsError } = storeToRefs(logsStore);
  const { renderLogLine, onLogScroll, scrollLogs, refreshLogs } = logsStore;

  const nodesStore = useNodesStore();
  const { nodes, latencies, coldLatencies, warmedLatencies, nodeSort, selectedNode } = storeToRefs(nodesStore);

  const subs = useSubscriptionsStore();
  const {
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
    profileDialog,
    importDialog,
    profileNameInput,
    activeSub,
    currentExternal,
    importTarget,
    profileLabel,
  } = storeToRefs(subs);
  const {
    selectProfile,
    resetProfileDialog,
    openProfileDialog,
    closeProfileDialog,
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
    copySubscription,
  } = subs;

  const settings = useSettingsStore();
  const {
    networkSettings,
    networkSecurity,
    appPolicies,
    draftProxyPort,
    draftAllowLan,
    draftTun,
    draftTunMtu,
    draftTunExclude,
    networkLoaded,
    networkError,
    showTraySpeed,
    autostartEnabled,
    autostartReady,
    autostartBusy,
    autostartError,
    proxyDiagnostic,
    diagnosticBusy,
    networkDirty,
    localProxyAddress,
  } = storeToRefs(settings);
  const {
    refreshProxyDiagnostics,
    saveNetworkSettings,
    setNetworkSecurity,
    setAppPolicy,
    setTunProtection,
    repairNetwork,
    toggleAutostart,
  } = settings;

  const probe = useProbeStore();
  const {
    testing,
    probeSummary,
    speedTesting,
    speedProgressMbps,
    speedPing,
    speedResult,
    speedUpProgressMbps,
    speedUpResult,
    speedError,
    probeErrors,
    probeProgress,
    probeTotal,
    probeConcurrency,
    probeCancelling,
    automaticSelection,
    sortedNodes,
  } = storeToRefs(probe);
  const { latencyTone, latencyLabel, testLatencies, testChain, clearChainTest, testSpeed, cancelSpeedTest } = probe;

  const account = useAccountStore();
  const { baseUrl, accountMenu, localMode, rememberMe, email, password, access, loginError } = storeToRefs(account);
  const { api, login, logout, refreshData, enterLocal } = account;

  const update = useUpdateStore();
  const { installedVersion, checkingUpdates, installingUpdate, updateOffer, updateStatus } = storeToRefs(update);
  const { checkUpdates, installUpdate } = update;

  const chain = useChainStore();
  const {
    chain: chainRef,
    chainGroups,
    chainTesting,
    chainDelay,
    chainError,
    chainActive,
    chainSummary,
  } = storeToRefs(chain);
  const { saveChainGroup, deleteChainGroup } = chain;

  const ui = useUiStore();
  const {
    theme,
    tab,
    mainContent,
    navigation,
    title,
    label,
    airportUsage,
    automaticNodeName,
    selectedNodeName,
    selectedRegionLabel,
    selectedLatencyText,
    phaseTone,
    statusChipText,
    connectionNodeNames,
    connectionDuration,
  } = storeToRefs(ui);
  const { fmt, openExternal } = ui;

  const windowStore = useWindowStore();
  const { windowAction } = windowStore;

  const connection = useConnectionStore();
  const { routeMode, chinaGeosite, adBlock } = storeToRefs(connection);
  const {
    bridge,
    buildProfileConfig,
    commitChain,
    setConnectionMode,
    setRouteMode,
    setRuleToggle,
    selectNode,
    releaseGuard,
    toggle,
    bootstrap,
    teardown,
  } = connection;

  return {
    // i18n
    locale,
    t,
    tx,
    // core
    phase,
    message,
    busy,
    connectionAction,
    connectionProgress,
    guardBlocking,
    guardArmed,
    connectionMode,
    connected,
    // logs
    logs,
    activityLogs,
    logViewport,
    followLogs,
    logsLoading,
    logsError,
    renderLogLine,
    onLogScroll,
    scrollLogs,
    refreshLogs,
    // nodes
    nodes,
    latencies,
    coldLatencies,
    warmedLatencies,
    nodeSort,
    selectedNode,
    // subscriptions
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
    profileDialog,
    importDialog,
    profileNameInput,
    activeSub,
    currentExternal,
    importTarget,
    profileLabel,
    selectProfile,
    resetProfileDialog,
    openProfileDialog,
    closeProfileDialog,
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
    copySubscription,
    // settings
    networkSettings,
    networkSecurity,
    appPolicies,
    draftProxyPort,
    draftAllowLan,
    draftTun,
    draftTunMtu,
    draftTunExclude,
    networkLoaded,
    networkError,
    showTraySpeed,
    autostartEnabled,
    autostartReady,
    autostartBusy,
    autostartError,
    proxyDiagnostic,
    diagnosticBusy,
    networkDirty,
    localProxyAddress,
    refreshProxyDiagnostics,
    saveNetworkSettings,
    setNetworkSecurity,
    setAppPolicy,
    setTunProtection,
    repairNetwork,
    toggleAutostart,
    // probe
    testing,
    probeSummary,
    speedTesting,
    speedProgressMbps,
    speedPing,
    speedResult,
    speedUpProgressMbps,
    speedUpResult,
    speedError,
    probeErrors,
    probeProgress,
    probeTotal,
    probeConcurrency,
    probeCancelling,
    automaticSelection,
    sortedNodes,
    latencyTone,
    latencyLabel,
    testLatencies,
    testChain,
    clearChainTest,
    testSpeed,
    cancelSpeedTest,
    // account
    baseUrl,
    accountMenu,
    localMode,
    rememberMe,
    email,
    password,
    access,
    loginError,
    api,
    login,
    logout,
    refreshData,
    enterLocal,
    // update
    installedVersion,
    checkingUpdates,
    installingUpdate,
    updateOffer,
    updateStatus,
    checkUpdates,
    installUpdate,
    // chain
    chain: chainRef,
    chainGroups,
    chainTesting,
    chainDelay,
    chainError,
    chainActive,
    chainSummary,
    saveChainGroup,
    deleteChainGroup,
    // ui
    theme,
    tab,
    mainContent,
    navigation,
    title,
    label,
    fmt,
    airportUsage,
    automaticNodeName,
    selectedNodeName,
    selectedRegionLabel,
    selectedLatencyText,
    phaseTone,
    statusChipText,
    connectionNodeNames,
    connectionDuration,
    openExternal,
    // window
    windowAction,
    // connection
    routeMode,
    chinaGeosite,
    adBlock,
    bridge,
    buildProfileConfig,
    commitChain,
    setConnectionMode,
    setRouteMode,
    setRuleToggle,
    selectNode,
    releaseGuard,
    toggle,
    bootstrap,
    teardown,
    // constants / pass-through
    enableLogin,
    githubUrl,
    regionCode,
    subscriptionHost,
    isNative,
    isMac,
    isLinux,
    AUTOMATIC_NODE_ID,
  };
}
export type AppStore = ReturnType<typeof create>;
let store: AppStore | null = null;
export function useApp(): AppStore {
  return (store ??= create());
}
