import { storeToRefs } from "pinia";
import { regionCode } from "../lib/regions";
import { subscriptionHost } from "../lib/subscription-info";
import { AUTOMATIC_NODE_ID } from "../lib/automatic-selection";
import { isNative, isMac, isLinux, currentPlatform } from "../lib/platform";
import { enableLogin, githubUrl } from "../constants/flags";
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
  const { logs, activityLogs, logViewport, followLogs, logsLoading, logsError, allLogs, logsText } =
    storeToRefs(logsStore);
  const { renderLogLine, logLine, addActivity, onLogScroll, scrollLogs, refreshLogs, pollLogs } = logsStore;

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
    importRequest,
    profileDialog,
    importDialog,
    profileNameInput,
    activeSub,
    currentExternal,
    importTarget,
    profileLabel,
  } = storeToRefs(subs);
  const {
    normalizeExternalRaw,
    persistExternal,
    activateExternal,
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
    importSub,
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
    probeStates,
    probeErrors,
    probeProgress,
    probeTotal,
    probeConcurrency,
    probeCancelling,
    automaticSelection,
    sortedNodes,
  } = storeToRefs(probe);
  const {
    refreshAutomaticSelection,
    latencyTone,
    latencyLabel,
    cancelProbes,
    testLatencies,
    testChain,
    clearChainTest,
    testSpeed,
    cancelSpeedTest,
  } = probe;

  const account = useAccountStore();
  const { baseUrl, accountMenu, localMode, rememberMe, email, password, access, refreshToken, loginError } =
    storeToRefs(account);
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
    chainNames,
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
    trafficLabel,
    airportUsage,
    usagePercent,
    remainingTraffic,
    automaticNodeName,
    selectedNodeName,
    selectedRegionLabel,
    selectedLatencyText,
    phaseTone,
    statusChipText,
    connectionNodeNames,
    connectionDuration,
  } = storeToRefs(ui);
  const { colorScheme, applyTheme, fmt, openExternal } = ui;

  const windowStore = useWindowStore();
  const { windowAction, preventContextMenu } = windowStore;

  const connection = useConnectionStore();
  const { routeMode, chinaGeosite, adBlock } = storeToRefs(connection);
  const {
    bridge,
    browserBridge,
    applySnapshot,
    cachedNode,
    persistPreferences,
    syncTray,
    trayAction,
    buildProfileConfig,
    reconcileConnectionError,
    reapplyProfile,
    applySelectedNode,
    commitChain,
    updateMode,
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
    allLogs,
    logsText,
    renderLogLine,
    logLine,
    addActivity,
    onLogScroll,
    scrollLogs,
    refreshLogs,
    pollLogs,
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
    probeStates,
    probeErrors,
    probeProgress,
    probeTotal,
    probeConcurrency,
    probeCancelling,
    automaticSelection,
    sortedNodes,
    refreshAutomaticSelection,
    latencyTone,
    latencyLabel,
    cancelProbes,
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
    refreshToken,
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
    chainNames,
    chainSummary,
    saveChainGroup,
    deleteChainGroup,
    // ui
    theme,
    colorScheme,
    applyTheme,
    tab,
    mainContent,
    navigation,
    title,
    label,
    fmt,
    trafficLabel,
    airportUsage,
    usagePercent,
    remainingTraffic,
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
    preventContextMenu,
    // connection
    routeMode,
    chinaGeosite,
    adBlock,
    bridge,
    browserBridge,
    applySnapshot,
    cachedNode,
    persistPreferences,
    syncTray,
    trayAction,
    buildProfileConfig,
    reconcileConnectionError,
    reapplyProfile,
    applySelectedNode,
    commitChain,
    updateMode,
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
    currentPlatform,
    AUTOMATIC_NODE_ID,
  };
}
export type AppStore = ReturnType<typeof create>;
let store: AppStore | null = null;
export function useApp(): AppStore {
  return (store ??= create());
}
