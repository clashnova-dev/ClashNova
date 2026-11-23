mod app_icons;
mod app_policies;
mod external_url;
mod conversion;
mod subscription_url;
mod startup;
use startup::check_proxy_egress;
mod recovery;
pub mod watchdog;
mod lan;
mod lan_transfer;
mod connection;
mod connections;
mod message;
mod network_info;
mod unlock;
mod network_settings;
mod notifications;
mod shortcuts;
mod tun;
mod tun_service;
#[cfg(windows)]
use network_settings::default_proxy_port;
use network_settings::NetworkSettings;
mod port_reclaim;
mod probe;
mod proxy_conflict;
mod storage;
mod settings_store;
mod tray;
mod tray_speed;
use tray::sync_tray;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod unix_proxy;

use connection::{reduce, ConnEvent, Phase};
use message::{msg, text};
#[cfg(windows)]
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::WindowEvent;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_log::{Target, TargetKind};

/// Prevent CUI sidecars from opening a visible console when the GUI app launches them.
pub(crate) fn hide_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

struct Core {
    phase: Phase,
    message: String,
    child: Option<CoreProcess>,
    tun_credentials: Option<PathBuf>,
    config_path: Option<PathBuf>,
    logs: Arc<Mutex<Vec<String>>>,
    auto_reconnect: bool,
    recovery: recovery::Recovery,
    recovery_intent_path: Option<PathBuf>,
    mode: String,
    network: NetworkSettings,
    traffic_endpoint: Option<tray_speed::Endpoint>,
    proxy_backup: Option<ProxyBackup>,
    proxy_backup_path: Option<PathBuf>,
    /// The kill switch was armed for this connection. While it is set and no core runs, the
    /// service may be blocking the network: the block is kept alive, shown, and must be released.
    tun_guard: bool,
    guard_checked: Option<Instant>,
}

#[cfg(windows)]
#[derive(Serialize, Deserialize)]
struct ProxyBackup {
    #[serde(default = "default_proxy_port")]
    owned_port: u16,
    enabled: u32,
    server: Option<String>,
    override_hosts: Option<String>,
    #[serde(default)]
    auto_config_url: Option<String>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
type ProxyBackup = unix_proxy::Backup;

#[derive(Serialize, Clone)]
struct Snapshot {
    phase: Phase,
    message: String,
    logs: Vec<String>,
    /// The tunnel dropped and the kill switch is holding the network closed.
    blocked: bool,
    /// The kill switch is armed: were the tunnel to drop, the network would be held closed.
    /// Lets the UI confirm protection is really in effect, not only that the setting is on.
    guarded: bool,
}

pub(crate) fn remember(logs: &Mutex<Vec<String>>, line: &str) {
    log::info!("{line}");
    let mut logs = logs.lock().expect("log buffer");
    logs.push(format!(
        "{}  {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        line
    ));
    if logs.len() > 200 {
        let extra = logs.len() - 200;
        logs.drain(0..extra);
    }
}

fn relay(reader: impl Read + Send + 'static, logs: Arc<Mutex<Vec<String>>>) {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            let Ok(line) = line else { break };
            if !line.is_empty() {
                remember(&logs, &line);
            }
        }
    });
}

impl Core {
    fn new() -> Self {
        Self {
            phase: Phase::Idle,
            message: "native.noSubscriptionImported".into(),
            child: None,
            tun_credentials: None,
            config_path: None,
            logs: Arc::new(Mutex::new(Vec::new())),
            auto_reconnect: true,
            recovery: recovery::Recovery::default(),
            recovery_intent_path: None,
            mode: "system".into(),
            network: NetworkSettings::default(),
            traffic_endpoint: None,
            proxy_backup: None,
            proxy_backup_path: None,
            tun_guard: false,
            guard_checked: None,
        }
    }

    fn push_log(&mut self, line: impl Into<String>) {
        remember(&self.logs, &line.into());
    }

    fn record_recovery_intent(&mut self, connected: bool) {
        if let Some(path) = &self.recovery_intent_path {
            if let Err(error) = storage::write_private(path, if connected { b"true" } else { b"false" }) {
                self.push_log(msg("recovery.intentWriteFailed", &[("error", text(error))]));
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            phase: self.phase,
            message: self.message.clone(),
            logs: self.logs.lock().expect("log buffer").clone(),
            blocked: self.tun_guard && self.child.is_none(),
            guarded: self.tun_guard,
        }
    }

    fn apply(&mut self, event: ConnEvent) -> Result<(), String> {
        self.phase = reduce(self.phase, event)?;
        Ok(())
    }
}

#[cfg(windows)]
fn notify_windows_proxy_changed() {
    use std::ffi::c_void;
    #[link(name = "wininet")]
    extern "system" {
        fn InternetSetOptionW(
            handle: *mut c_void,
            option: u32,
            buffer: *mut c_void,
            length: u32,
        ) -> i32;
    }
    unsafe {
        // INTERNET_OPTION_SETTINGS_CHANGED / INTERNET_OPTION_REFRESH.
        InternetSetOptionW(std::ptr::null_mut(), 39, std::ptr::null_mut(), 0);
        InternetSetOptionW(std::ptr::null_mut(), 37, std::ptr::null_mut(), 0);
    }
}

#[cfg(windows)]
fn enable_system_proxy(backup_path: &Path, port: u16) -> Result<ProxyBackup, String> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE},
        RegKey,
    };
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            KEY_READ | KEY_WRITE,
        )
        .map_err(|e| e.to_string())?;
    let backup = ProxyBackup {
        owned_port: port,
        enabled: key.get_value("ProxyEnable").unwrap_or(0),
        server: key.get_value("ProxyServer").ok(),
        override_hosts: key.get_value("ProxyOverride").ok(),
        auto_config_url: key.get_value("AutoConfigURL").ok(),
    };
    fs::write(
        backup_path,
        serde_json::to_vec(&backup).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let target = format!("127.0.0.1:{port}");
    let mut last_error = String::new();
    for _ in 0..2 {
        let result = (|| -> Result<(), String> {
            key.set_value("ProxyServer", &target)
                .map_err(|error| error.to_string())?;
            key.set_value("ProxyOverride", &"<local>;localhost;127.0.0.1")
                .map_err(|error| error.to_string())?;
            if backup.auto_config_url.is_some() {
                match key.delete_value("AutoConfigURL") {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            key.set_value("ProxyEnable", &1u32)
                .map_err(|error| error.to_string())?;
            notify_windows_proxy_changed();
            let enabled: u32 = key
                .get_value("ProxyEnable")
                .map_err(|error| error.to_string())?;
            let server: String = key
                .get_value("ProxyServer")
                .map_err(|error| error.to_string())?;
            if enabled != 1 || server != target {
                return Err("native.systemProxyMismatch".into());
            }
            Ok(())
        })();
        match result {
            Ok(()) => return Ok(backup),
            Err(error) => last_error = error,
        }
    }
    Err(msg(
        "native.systemProxyApplyFailed",
        &[("error", text(last_error))],
    ))
}

#[cfg(windows)]
fn restore_system_proxy(backup: ProxyBackup) -> Result<(), String> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE},
        RegKey,
    };
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            KEY_READ | KEY_WRITE,
        )
        .map_err(|e| e.to_string())?;
    let current: String = key.get_value("ProxyServer").unwrap_or_default();
    if current != format!("127.0.0.1:{}", backup.owned_port)
        && current != backup.server.as_deref().unwrap_or_default()
    {
        return Ok(());
    }
    key.set_value("ProxyEnable", &backup.enabled)
        .map_err(|e| e.to_string())?;
    if let Some(value) = backup.server {
        key.set_value("ProxyServer", &value)
            .map_err(|e| e.to_string())?;
    } else {
        let _ = key.delete_value("ProxyServer");
    }
    if let Some(value) = backup.override_hosts {
        key.set_value("ProxyOverride", &value)
            .map_err(|e| e.to_string())?;
    } else {
        let _ = key.delete_value("ProxyOverride");
    }
    if let Some(value) = backup.auto_config_url {
        key.set_value("AutoConfigURL", &value)
            .map_err(|error| error.to_string())?;
    }
    notify_windows_proxy_changed();
    Ok(())
}

fn restore_saved_proxy(core: &mut Core) -> Result<(), String> {
    let Some(path) = core.proxy_backup_path.as_ref() else {
        return Ok(());
    };
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(msg(
                "native.proxyBackupReadFailed",
                &[("error", text(error))],
            ))
        }
    };
    let mut last_error = String::new();
    for _ in 0..2 {
        let backup: ProxyBackup =
            serde_json::from_slice(&raw).map_err(|_| "native.proxyBackupInvalid")?;
        match restore_system_proxy(backup) {
            Ok(()) => {
                fs::remove_file(path).map_err(|error| error.to_string())?;
                core.push_log("log.systemProxyRestored");
                return Ok(());
            }
            Err(error) => last_error = error,
        }
    }
    Err(msg(
        "native.systemProxyRestoreFailed",
        &[("error", text(last_error))],
    ))
}

fn restore_active_proxy(core: &mut Core) -> Result<(), String> {
    // The private recovery file is authoritative and remains available after a
    // partial failure. Never overwrite it with a new proxy snapshot.
    core.proxy_backup = None;
    restore_saved_proxy(core)
}
#[cfg(not(windows))]
fn restore_system_proxy(backup: ProxyBackup) -> Result<(), String> {
    unix_proxy::restore(backup)
}

fn emit_state(app: &AppHandle, core: &Core) {
    let _ = app.emit("connection://state", core.snapshot());
}

#[tauri::command]
async fn connection_state(state: State<'_, Arc<Mutex<Core>>>) -> Result<Snapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state
            .lock()
            .map(|core| core.snapshot())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn read_logs(state: State<'_, Arc<Mutex<Core>>>) -> Result<Vec<String>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state
            .lock()
            .map(|core| core.snapshot().logs)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn load_network_settings(state: State<Arc<Mutex<Core>>>) -> Result<NetworkSettings, String> {
    let core = state.try_lock().map_err(|_| "native.connectionBusy")?;
    Ok(core.network.clone())
}

#[tauri::command]
fn save_network_settings(
    app: AppHandle,
    state: State<Arc<Mutex<Core>>>,
    mut settings: NetworkSettings,
) -> Result<NetworkSettings, String> {
    let mut core = state.try_lock().map_err(|_| "native.connectionBusy")?;
    if !matches!(core.phase, Phase::Idle | Phase::Failed) {
        return Err("native.disconnectToChangeNetwork".into());
    }
    settings.ensure_available()?;
    // Saving is the user's choice. Until then, an old file that still says IPv6
    // resolution is on is treated as off, so TUN does not black-hole the network.
    settings.tun.ipv6_chosen = true;
    let path = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?
        .join("network-settings.json");
    storage::write_private(
        &path,
        &serde_json::to_vec(&settings).map_err(|error| error.to_string())?,
    )?;
    core.network = settings.clone();
    core.push_log(msg(
        "log.networkSettingsSaved",
        &[
            ("address", text(settings.listen_address())),
            ("port", settings.proxy_port.into()),
        ],
    ));
    Ok(settings)
}

#[tauri::command]
async fn proxy_diagnostics() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(diagnose_system_proxy)
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(not(windows))]
fn diagnose_system_proxy() -> Result<String, String> {
    unix_proxy::diagnose()
}

#[cfg(windows)]
fn diagnose_system_proxy() -> Result<String, String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .map_err(|error| error.to_string())?;
    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    Ok(if enabled == 0 {
        "native.systemProxyDisabled"
    } else {
        "native.systemProxyEnabled"
    }
    .into())
}

#[tauri::command]
async fn begin_probe(config: String, tags: Vec<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || probe::begin(config, tags))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn probe_delay(session: String, tag: String) -> Result<probe::DelayResult, String> {
    tauri::async_runtime::spawn_blocking(move || probe::delay(session, tag))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn end_probe(session: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || probe::end(&session))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn probe_node(host: String, port: u16) -> Result<u32, String> {
    if host.is_empty() || host.len() > 253 || port == 0 {
        return Err("native.nodeAddressInvalid".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let addresses = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|err| msg("native.nodeResolveFailed", &[("error", text(err))]))?;
        for address in addresses.take(4) {
            if TcpStream::connect_timeout(&address, Duration::from_millis(900)).is_ok() {
                return Ok(started.elapsed().as_millis().min(u32::MAX as u128) as u32);
            }
        }
        Err("native.nodeConnectTimeout".into())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[derive(Serialize)]
struct SubscriptionDownload {
    raw: String,
    userinfo: Option<String>,
}

#[tauri::command]
async fn fetch_subscription(url: String) -> Result<String, String> {
    Ok(fetch_subscription_details(url).await?.raw)
}

#[tauri::command]
async fn fetch_subscription_details(url: String) -> Result<SubscriptionDownload, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let parsed = subscription_url::validate(&url)?;
        let origin = parsed.clone();
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                let url = attempt.url();
                // These reasons are not keys: reqwest reports a refused redirect only as
                // "error following redirect", so they show up in debug output, never in the UI.
                if attempt.previous().len() >= 3 {
                    attempt.error("too many subscription redirects")
                } else if subscription_url::redirect_allowed(&origin, url) {
                    attempt.follow()
                } else {
                    attempt.error("subscription redirected to an insecure address")
                }
            }))
            .build()
            .map_err(|err| err.to_string())?;
        let user_agent = if parsed
            .query_pairs()
            .any(|(key, value)| key == "clash" && value == "1")
        {
            "clash.meta"
        } else {
            "Airport/0.1.0 sing-box"
        };
        let mut response = client
            .get(parsed)
            .header("User-Agent", user_agent)
            .send()
            .map_err(|err| {
                msg(
                    "native.subscriptionFetchFailed",
                    &[("error", text(err.without_url()))],
                )
            })?;
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err("native.subscriptionForbidden".into());
        }
        if !response.status().is_success() {
            return Err(msg(
                "native.subscriptionHttpStatus",
                &[("status", text(response.status()))],
            ));
        }
        let final_url = response.url();
        let final_local = final_url
            .host_str()
            .is_some_and(|host| host == "127.0.0.1" || host == "localhost");
        if final_url.scheme() != "https" && !(final_url.scheme() == "http" && final_local) {
            return Err("native.subscriptionRedirectInsecure".into());
        }
        let userinfo = response
            .headers()
            .get("subscription-userinfo")
            .or_else(|| {
                response
                    .headers()
                    .iter()
                    .find(|(key, _)| key.as_str().ends_with("-subscription-userinfo"))
                    .map(|(_, value)| value)
            })
            .and_then(|value| value.to_str().ok())
            .filter(|value| value.len() <= 4096)
            .map(str::to_owned);
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|err| err.to_string())?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("error.subscriptionTooLarge".into());
        }
        let raw = String::from_utf8(bytes).map_err(|_| "native.subscriptionNotUtf8".to_string())?;
        Ok(SubscriptionDownload { raw, userinfo })
    })
    .await
    .map_err(|err| err.to_string())?
}

fn resolve_sidecar(name: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let dir = exe.parent().ok_or("native.appDirNotFound")?;
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let filename = format!("{name}{suffix}");
    let triple_name = format!("{name}-{}{suffix}", env!("AIRPORT_TARGET"));
    let mut candidates = vec![dir.join(&filename), dir.join(&triple_name)];
    // Source-tree fallbacks are development-only. Releases must use the
    // installed sidecars, never an arbitrary executable from PATH.
    if cfg!(debug_assertions) {
        candidates.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("binaries")
                .join(&triple_name),
        );
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| msg("native.sidecarMissing", &[("name", text(name))]))
}

fn resolve_converter() -> Result<PathBuf, String> {
    resolve_sidecar("sing-box-sub")
}

#[tauri::command]
async fn convert_subscription(raw: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if raw.len() > 2 * 1024 * 1024 {
            return Err("error.subscriptionTooLarge".into());
        }
        let converter = resolve_converter()?;
        let mut input = tempfile::NamedTempFile::new().map_err(|err| err.to_string())?;
        use std::io::Write;
        input
            .write_all(raw.as_bytes())
            .map_err(|err| err.to_string())?;
        input.flush().map_err(|err| err.to_string())?;
        let mut cmd = Command::new(converter);
        hide_console(&mut cmd).arg(input.path()).arg("--only-nodes").arg("--out").arg("-");
        let output = conversion::output(&mut cmd, Duration::from_secs(15), 4 * 1024 * 1024)?;
        let nodes: Vec<serde_json::Value> =
            serde_json::from_slice(&output).map_err(|_| "native.converterOutputInvalid")?;
        if nodes.is_empty() {
            return Err("native.subscriptionNoNodes".into());
        }
        serde_json::to_string(&serde_json::json!({"outbounds": nodes}))
            .map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

fn load_subscription_file(path: &std::path::Path) -> Result<String, String> {
    let metadata = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok("null".into()),
        other => other.map_err(|error| error.to_string())?,
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("native.subscriptionListInvalid".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("native.subscriptionListInvalid".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "native.subscriptionListInvalid")?;
    if !value.is_array() { return Err("native.subscriptionListInvalid".into()); }
    String::from_utf8(bytes).map_err(|_| "native.subscriptionListInvalid".into())
}

#[tauri::command]
fn load_subscriptions(app: AppHandle) -> Result<String, String> {
    let path = app
        .path()
        .app_config_dir()
        .map_err(|err| err.to_string())?
        .join("subscriptions.json");
    load_subscription_file(&path)
}

#[tauri::command]
fn save_subscriptions(app: AppHandle, data: String) -> Result<(), String> {
    if data.len() > 16 * 1024 * 1024
        || !serde_json::from_str::<serde_json::Value>(&data)
            .map_err(|err| err.to_string())?
            .is_array()
    {
        return Err("native.subscriptionListInvalid".into());
    }
    let dir = app.path().app_config_dir().map_err(|err| err.to_string())?;
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let path = dir.join("subscriptions.json");
    storage::write_private(&path, data.as_bytes())?;
    Ok(())
}

#[tauri::command]
fn save_profile(
    app: AppHandle,
    state: State<Arc<Mutex<Core>>>,
    config: String,
) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|err| err.to_string())?;
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let path = dir.join("config.json");
    let mut core = state.try_lock().map_err(|_| "native.connectionBusy")?;
    if !matches!(core.phase, Phase::Idle | Phase::Failed) {
        return Err("native.disconnectToSaveProfile".into());
    }
    let config = core.network.apply_to_config(&config)?;
    storage::write_private(&path, config.as_bytes())?;
    core.config_path = Some(path);
    core.message = "native.subscriptionSaved".into();
    core.push_log("log.profileSaved");
    Ok(())
}

#[tauri::command]
fn clear_profile(state: State<Arc<Mutex<Core>>>) -> Result<(), String> {
    let mut core = state.lock().map_err(|err| err.to_string())?;
    stop_core(&mut core)?;
    if let Some(path) = core.config_path.as_ref() {
        if path.exists() {
            fs::remove_file(path).map_err(|err| err.to_string())?;
        }
    }
    core.config_path = None;
    core.message = "native.signedOutProfileCleared".into();
    core.push_log("log.signedOut");
    Ok(())
}

/// The sing-box core, built from `core/` for the target instead of fetched as a release binary.
fn resolve_binary() -> Result<PathBuf, String> {
    resolve_sidecar("airport-core")
}

fn startup_progress(core: &mut Core, app: Option<&AppHandle>, stage: &str, started: Instant) {
    let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    core.push_log(msg("log.startupProgress", &[("stage", text(stage)), ("milliseconds", elapsed.into())]));
    if let Some(app) = app.filter(|_| core.phase != Phase::Connected) {
        let _ = app.emit("connection://progress", serde_json::json!({"stage": stage, "elapsedMs": elapsed}));
    }
}

fn spawn_core(core: &mut Core, app: Option<&AppHandle>) -> Result<(), String> {
    let started = Instant::now();
    startup_progress(core, app, "startup.preparing", started);
    let _service_guard = tun_service::connection_guard()?;
    let path = core
        .config_path
        .clone()
        .ok_or("native.importSubscriptionFirst")?;
    let port = core.network.proxy_port;
    if let Some(detail) = proxy_conflict::detail(port) {
        core.apply(ConnEvent::Fail)?;
        core.message = msg("native.otherProxyInUse", &[("detail", text(detail))]);
        core.push_log(core.message.clone());
        return Err(core.message.clone());
    }
    // Read-only. Overlaps config, process start and the node check. Applied only if that check passes.
    #[cfg(not(windows))]
    let prepared = if core.mode == "system" {
        Some(thread::spawn(move || unix_proxy::prepare_enable(port)))
    } else {
        None
    };
    let listen = core.network.listen_address().to_owned();
    let reclaiming = thread::spawn(move || port_reclaim::reclaim(port, &listen));
    let config = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut config: serde_json::Value =
        serde_json::from_str(&core.network.apply_to_config(&config)?).map_err(|e| e.to_string())?;
    tun::configure(&mut config, core.mode == "tun", &core.network.tun)?;
    config["log"] = serde_json::json!({"level":"info", "timestamp":false});
    let endpoint = tray_speed::Endpoint::new()?;
    endpoint.configure(&mut config);
    storage::write_private(&path, config.to_string().as_bytes())?;
    core.traffic_endpoint = Some(endpoint);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let reclaimed = reclaiming.join().unwrap_or_default();
    if !reclaimed.is_empty() {
        core.push_log(msg("log.proxyPortReclaimed", &[("port", port.into())]));
    }
    if let Err(error) = core.network.ensure_available() {
        core.apply(ConnEvent::Fail)?;
        core.message = error;
        return Err(core.message.clone());
    }
    let binary = match resolve_binary() {
        Ok(binary) => binary,
        Err(message) => {
            core.apply(ConnEvent::Fail)?;
            core.message = message;
            core.push_log(core.message.clone());
            return Err(core.message.clone());
        }
    };
    // Spawn the core straight away: `sing-box run` parses and validates the config itself, so a
    // separate `sing-box check` beforehand only doubled the process spawn (cold and
    // Gatekeeper-gated on macOS) on every connect. A bad config now surfaces as a start failure
    // (local proxy) or is caught by the TUN service's own `safe_config`, instead of blocking the
    // connect on an extra launch each time.
    startup_progress(core, app, "startup.starting", started);
    let process = if core.mode == "tun" {
        tun_service::ServiceProcess::start(
            core.tun_credentials
                .as_deref()
                .ok_or("native.tunServiceNotInitialized")?,
            config,
            core.logs.clone(),
        )
        .map(CoreProcess::Service)
    } else {
        {
            let mut cmd = Command::new(&binary);
            hide_console(&mut cmd)
                .arg("run")
                .arg("-c")
                .arg(&path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map(|mut child| {
                    if let Some(out) = child.stdout.take() {
                        relay(out, core.logs.clone());
                    }
                    if let Some(err) = child.stderr.take() {
                        relay(err, core.logs.clone());
                    }
                    CoreProcess::Local(child)
                })
                .map_err(|e| e.to_string())
        }
    };
    match process {
        Ok(child) => {
            core.child = Some(child);
            core.apply(ConnEvent::CoreSpawned)?;
            core.message = "status.connecting".into();
            if let Some(app) = app { emit_state(app, core); }
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = core
                    .child
                    .as_mut()
                    .unwrap()
                    .try_wait()
                    .map_err(|err| err.to_string())?
                {
                    core.child = None;
                    core.apply(ConnEvent::Fail)?;
                    core.message = msg("native.coreStartFailed", &[("status", text(status))]);
                    return Err(core.message.clone());
                }
                if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
                    break;
                }
                if Instant::now() >= deadline {
                    if let Some(mut child) = core.child.take() {
                        let _ = terminate_child(&mut child);
                    }
                    core.apply(ConnEvent::Fail)?;
                    core.message = "native.coreListenTimeout".into();
                    return Err(core.message.clone());
                }
                thread::sleep(Duration::from_millis(20));
            }
            startup_progress(core, app, "startup.verifying", started);
            let egress = check_proxy_egress(port);
            if let Err(err) = egress {
                let exited = core
                    .child
                    .as_mut()
                    .and_then(|child| child.try_wait().ok().flatten());
                if let Some(mut child) = core.child.take() {
                    let _ = terminate_child(&mut child);
                }
                core.apply(ConnEvent::Fail)?;
                core.message = if core.mode == "tun" && exited.is_some() {
                    "native.tunCoreStartFailed".into()
                } else {
                    msg("native.egressCheckFailed", &[("error", text(err))])
                };
                core.push_log(core.message.clone());
                return Err(core.message.clone());
            }
            startup_progress(core, app, "startup.applying", started);
            if core.mode == "system" {
                let backup_path = core
                    .proxy_backup_path
                    .clone()
                    .ok_or("native.proxyBackupPathUnknown")?;
                #[cfg(not(windows))]
                let enabled = match prepared.unwrap().join() {
                    Ok(Ok((backup, changes))) => {
                        unix_proxy::commit_enable(&backup_path, backup, changes)
                    }
                    Ok(Err(error)) => Err(error),
                    Err(_) => Err(msg(
                        "native.systemProxyApplyFailed",
                        &[("error", text("prepare"))],
                    )),
                };
                #[cfg(windows)]
                let enabled = enable_system_proxy(&backup_path, port);
                match enabled {
                    Ok(backup) => core.proxy_backup = Some(backup),
                    Err(err) => {
                        if let Err(recovery_error) = restore_saved_proxy(core) {
                            core.push_log(recovery_error);
                        }
                        if let Some(mut child) = core.child.take() {
                            let _ = terminate_child(&mut child);
                        }
                        core.apply(ConnEvent::Fail)?;
                        core.message =
                            msg("native.systemProxyEnableFailed", &[("error", text(err))]);
                        return Err(core.message.clone());
                    }
                }
            }
            // The tunnel is verified end to end, which is the moment the kill switch can take over:
            // armed any earlier, a core that fails to start would block a network it never served.
            if core.mode == "tun" && (core.network.tun.kill_switch || core.tun_guard) {
                let wanted = core.network.tun.kill_switch;
                let armed = match core.child.as_ref() {
                    Some(CoreProcess::Service(service)) if wanted => service.arm(),
                    // The setting was turned off after a drop: lift what is left, protect nothing.
                    Some(CoreProcess::Service(_)) => core
                        .tun_credentials
                        .as_deref()
                        .map_or(Ok(()), tun_service::release_guard),
                    _ => Ok(()),
                };
                match armed {
                    Ok(()) => core.tun_guard = wanted,
                    // After a drop the service may still be blocking, and arming is what lifts
                    // that: reporting a working connection over a closed network would be a lie.
                    Err(error) if core.tun_guard => {
                        if let Some(mut child) = core.child.take() {
                            let _ = terminate_child(&mut child);
                        }
                        core.apply(ConnEvent::Fail)?;
                        core.message = error;
                        core.push_log(core.message.clone());
                        return Err(core.message.clone());
                    }
                    Err(error) => {
                        core.push_log(msg("log.tunGuardArmFailed", &[("error", text(error))]))
                    }
                }
            }
            core.apply(ConnEvent::CoreReady)?;
            startup_progress(core, app, "status.connected", started);
            core.message = msg(
                "native.proxyStarted",
                &[
                    ("address", text(core.network.listen_address())),
                    ("port", port.into()),
                ],
            );
            core.push_log(msg("log.coreStarted", &[("path", text(binary.display()))]));
            Ok(())
        }
        Err(err) => {
            core.apply(ConnEvent::Fail)?;
            core.message = msg("native.coreSpawnFailed", &[("error", text(err))]);
            core.push_log(core.message.clone());
            Err(core.message.clone())
        }
    }
}

#[tauri::command]
async fn connect(app: AppHandle, state: State<'_, Arc<Mutex<Core>>>, mode: String) -> Result<Snapshot, String> {
    if !matches!(mode.as_str(), "system" | "tun" | "manual") {
        return Err("native.connectionModeInvalid".into());
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut core = state.lock().map_err(|error| error.to_string())?;
        if !matches!(core.phase, Phase::Idle | Phase::Failed) {
            return Err("native.cannotConnectNow".into());
        }
        core.mode = mode;
        if let Err(error) = start_core(&mut core, Some(&app)) {
            if let Err(cleanup) = failed_recovery_cleanup(&mut core) {
                stop_unsafe_recovery(&mut core, cleanup);
                return Err(core.message.clone());
            }
            let _ = core.apply(ConnEvent::Fail);
            core.record_recovery_intent(false);
            core.message = error.clone();
            guard_notice(&mut core);
            return Err(core.message.clone());
        }
        Ok(core.snapshot())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn set_connection_mode(state: State<Arc<Mutex<Core>>>, mode: String) -> Result<(), String> {
    if mode != "system" && mode != "tun" && mode != "manual" {
        return Err("native.connectionModeInvalid".into());
    }
    let mut core = state.lock().map_err(|err| err.to_string())?;
    if core.phase != Phase::Idle && core.phase != Phase::Failed {
        return Err("native.disconnectToChangeMode".into());
    }
    core.mode = mode;
    Ok(())
}

fn start_core(core: &mut Core, app: Option<&AppHandle>) -> Result<(), String> {
    if core.config_path.is_none() {
        return Err("native.signInAndImportFirst".into());
    }
    if matches!(core.phase, Phase::Idle | Phase::Failed) {
        core.apply(ConnEvent::Start)?;
    } else {
        return Err("native.cannotConnectNow".into());
    }
    // A prior failed stop cannot be treated as a free port.
    stop_child(core)?;
    restore_saved_proxy(core)?;
    core.auto_reconnect = true;
    core.recovery.reset();
    // A block left by a dropped tunnel is only lifted by a tunnel coming back. Connecting any
    // other way is choosing to go without one.
    if core.mode != "tun" {
        release_tun_guard(core);
    }
    core.message = "status.preparing".into();
    if let Some(app) = app { emit_state(app, core); }
    spawn_core(core, app)?;
    core.recovery.healthy(Instant::now());
    core.record_recovery_intent(true);
    Ok(())
}

#[tauri::command]
async fn disconnect(state: State<'_, Arc<Mutex<Core>>>) -> Result<Snapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut core = state.lock().map_err(|error| error.to_string())?;
        stop_core(&mut core)?;
        Ok(core.snapshot())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Open an external link (About page: project repository, chat group) in the user's
/// default browser. Only web and Telegram links are allowed, so a build-time
/// misconfiguration cannot turn this into an arbitrary launcher.
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    external_url::open(&url)
}

enum CoreProcess {
    Local(Child),
    Service(tun_service::ServiceProcess),
}
impl CoreProcess {
    fn try_wait(&mut self) -> Result<Option<String>, String> {
        match self {
            Self::Local(child) => child
                .try_wait()
                .map(|status| status.map(|s| s.to_string()))
                .map_err(|e| e.to_string()),
            Self::Service(service) => service.poll(),
        }
    }
}
fn stop_child(core: &mut Core) -> Result<(), String> {
    if let Some(mut child) = core.child.take() {
        if let Err(error) = terminate_child(&mut child) {
            core.child = Some(child);
            return Err(error);
        }
    }
    Ok(())
}

fn terminate_child(child: &mut CoreProcess) -> Result<(), String> {
    match child {
        CoreProcess::Local(local) => terminate_local_child(local),
        CoreProcess::Service(service) => service
            .stop()
            .map_err(|error| msg("native.tunStopUnconfirmed", &[("error", text(error))])),
    }
}

// Give sing-box time to remove native DNS and routes before forcing termination.
fn terminate_local_child(child: &mut Child) -> Result<(), String> {
    if child.try_wait().ok().flatten().is_some() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill")
            .args(["-TERM", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(40));
        }
    }
    if let Err(error) = child.kill() {
        if child.try_wait().map_err(|e| e.to_string())?.is_none() {
            return Err(error.to_string());
        }
    }
    child.wait().map(|_| ()).map_err(|e| e.to_string())
}

/// Lifts the kill switch. Disconnecting is the user's decision to be without the tunnel, so
/// it must also end whatever the tunnel's absence was blocking.
fn release_tun_guard(core: &mut Core) {
    if !std::mem::take(&mut core.tun_guard) {
        return;
    }
    core.guard_checked = None;
    if let Some(path) = core.tun_credentials.clone() {
        if let Err(error) = tun_service::release_guard(&path) {
            core.push_log(msg("log.tunGuardReleaseFailed", &[("error", text(error))]));
        }
    }
}

/// Words a failure for a user whose network is being held closed by the kill switch.
fn guard_notice(core: &mut Core) {
    if core.tun_guard && core.child.is_none() {
        core.message = msg("native.tunGuardBlocking", &[("error", text(&core.message))]);
    }
}

fn stop_core(core: &mut Core) -> Result<(), String> {
    core.record_recovery_intent(false);
    core.recovery.reset();
    core.traffic_endpoint = None;
    core.auto_reconnect = false;
    release_tun_guard(core);
    let recovery = restore_active_proxy(core);
    if core.phase == Phase::Idle && core.child.is_none() {
        return recovery;
    }
    let _ = core.apply(ConnEvent::Stop);
    let stopped = stop_child(core);
    if let Err(error) = recovery.and(stopped) {
        let _ = core.apply(ConnEvent::Fail);
        core.message = error.clone();
        core.push_log(error.clone());
        return Err(error);
    }
    if core.phase == Phase::Disconnecting {
        core.apply(ConnEvent::Stopped)?;
    }
    core.message = "status.disconnected".into();
    core.push_log("log.disconnectedManually");
    Ok(())
}

fn queue_core_recovery(core: &mut Core, reason: String) {
    core.traffic_endpoint = None;
    if let Some((attempt, delay)) = core.recovery.schedule(Instant::now()) {
        if core.phase != Phase::Reconnecting {
            let _ = core.apply(ConnEvent::Fail);
            let _ = core.apply(ConnEvent::Retry);
        }
        core.message = msg(
            "recovery.coreScheduled",
            &[
                ("attempt", attempt.into()),
                ("seconds", delay.into()),
                ("error", text(reason)),
            ],
        );
    } else {
        let _ = core.apply(ConnEvent::Fail);
        core.auto_reconnect = false;
        core.record_recovery_intent(false);
        core.message = msg("recovery.coreExhausted", &[("error", text(reason))]);
        guard_notice(core);
    }
    core.push_log(core.message.clone());
}

fn failed_recovery_cleanup(core: &mut Core) -> Result<(), String> {
    core.traffic_endpoint = None;
    let stopped = stop_child(core);
    let restored = restore_active_proxy(core);
    stopped.and(restored)
}

fn stop_unsafe_recovery(core: &mut Core, error: String) {
    core.auto_reconnect = false;
    core.recovery.reset();
    core.record_recovery_intent(false);
    if core.phase == Phase::Connected {
        let _ = core.apply(ConnEvent::CoreExited);
    }
    let _ = core.apply(ConnEvent::Fail);
    core.message = msg("recovery.cleanupBlocked", &[("error", text(error))]);
    core.push_log(core.message.clone());
    guard_notice(core);
}

fn handle_core_exit(core: &mut Core, status: String, uncertain: bool) {
    // A failed status read is not evidence that the old process exited. Stop and reap
    // it first; if that cannot be confirmed, never start a second core on its port.
    if uncertain {
        if let Err(error) = failed_recovery_cleanup(core) {
            stop_unsafe_recovery(core, error);
            return;
        }
    } else {
        core.child = None;
        core.traffic_endpoint = None;
    }
    if matches!(core.phase, Phase::Idle | Phase::Disconnecting) {
        return;
    }
    core.push_log(msg("log.coreExited", &[("status", text(&status))]));
    if let Err(error) = restore_active_proxy(core) {
        stop_unsafe_recovery(core, error);
        return;
    }
    let was_connected = core.phase == Phase::Connected;
    if was_connected {
        let _ = core.apply(ConnEvent::CoreExited);
    }
    if core.auto_reconnect && was_connected {
        core.message = "native.coreExitedReconnecting".into();
        queue_core_recovery(core, status);
    } else {
        let reconnecting = !was_connected && core.phase == Phase::Reconnecting;
        let _ = core.apply(ConnEvent::Fail);
        core.auto_reconnect = false;
        core.record_recovery_intent(false);
        core.message = msg(
            if reconnecting {
                "native.reconnectFailed"
            } else {
                "native.coreCrashed"
            },
            &[("status", text(status))],
        );
        guard_notice(core);
    }
}

fn watch_child(app: AppHandle, state: Arc<Mutex<Core>>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(500));
        let mut core = match state.lock() {
            Ok(guard) => guard,
            Err(_) => continue,
        };
        let observation = core
            .child
            .as_mut()
            .and_then(|child| match child.try_wait() {
                Ok(Some(status)) => Some((status.to_string(), false)),
                Ok(None) => None,
                Err(error) => Some((error.to_string(), true)),
            });
        if let Some((status, uncertain)) = observation {
            handle_core_exit(&mut core, status, uncertain);
            emit_state(&app, &core);
        }
        if core.child.is_some() && core.phase == Phase::Connected {
            core.recovery.healthy(Instant::now());
        }
        if core.child.is_none()
            && core.tun_guard
            && core
                .guard_checked
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(2))
        {
            core.guard_checked = Some(Instant::now());
            if let Some(path) = core.tun_credentials.clone() {
                if matches!(tun_service::guard_keepalive(&path), Ok(false)) {
                    core.tun_guard = false;
                    emit_state(&app, &core);
                }
            }
        }
        if core.child.is_none()
            && core.auto_reconnect
            && core.phase == Phase::Reconnecting
            && core.recovery.due(Instant::now())
        {
            match spawn_core(&mut core, Some(&app)) {
                Ok(()) => {
                    core.recovery.healthy(Instant::now());
                    core.record_recovery_intent(true);
                }
                Err(error) => match failed_recovery_cleanup(&mut core) {
                    Ok(()) => queue_core_recovery(&mut core, error),
                    Err(cleanup) => stop_unsafe_recovery(&mut core, cleanup),
                },
            }
            emit_state(&app, &core);
        }
    });
}

/// Only a watchdog relaunch can request a one-shot connection restoration.
#[tauri::command]
async fn take_crash_recovery(
    watchdog: State<'_, watchdog::Guard>,
    state: State<'_, Arc<Mutex<Core>>>,
) -> Result<bool, String> {
    if !watchdog.take_reconnect() {
        return Ok(false);
    }
    let credentials = {
        let core = state.lock().map_err(|e| e.to_string())?;
        if core.child.is_some() {
            return Ok(false);
        }
        core.tun_credentials.clone()
    };
    tauri::async_runtime::spawn_blocking(move || {
        // A previous TUN lease must expire before conflict detection or starting another core.
        if let Some(path) = credentials.filter(|p| p.exists()) {
            tun_service::await_previous_session(&path)?;
        }
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let core = Arc::new(Mutex::new(Core::new()));
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .targets([
                    #[cfg(debug_assertions)]
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_persisted_scope::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Position only: the size follows the screen shown (login card or main UI) and is set by the webview.
                .with_state_flags(tauri_plugin_window_state::StateFlags::POSITION)
                .build(),
        )
        .manage(core.clone())
        .manage(Arc::new(port_reclaim::Reclaimer::default()))
        .invoke_handler(tauri::generate_handler![
            notifications::send_desktop_notification,
            tun_service::tun_service_status,
            tun_service::install_tun_service,
            tun_service::uninstall_tun_service,
            network_info::get_exit_info,
            lan::get_lan_addresses,
            network_info::network_check,
            unlock::unlock_check,
            unlock::unlock_cancel,
            network_info::get_auto_selection,
            shortcuts::set_shortcut_recording,
            shortcuts::load_shortcuts,
            shortcuts::save_shortcuts,
            connections::list_connections,
            connections::application_icon,
            connections::close_connections,
            app_policies::set_application_policy,
            sync_tray,
            load_network_settings,
            settings_store::get_client_setting,
            settings_store::set_client_setting,
            proxy_diagnostics,
            port_reclaim::inspect_proxy_port,
            port_reclaim::release_proxy_port,
            save_network_settings,
            connection_state,
            take_crash_recovery,
            read_logs,
            probe_node,
            begin_probe,
            probe_delay,
            end_probe,
            fetch_subscription,
            fetch_subscription_details,
            convert_subscription,
            load_subscriptions,
            save_subscriptions,
            save_profile,
            clear_profile,
            set_connection_mode,
            connect,
            disconnect,
            open_external,
            lan_transfer::lan_get_local_device,
            lan_transfer::lan_add_manual_device,
            lan_transfer::lan_get_settings,
            lan_transfer::lan_update_settings,
            lan_transfer::lan_refresh_devices,
            lan_transfer::lan_list_transfers,
            lan_transfer::lan_send_files,
            lan_transfer::lan_send_text,
            lan_transfer::lan_respond_incoming,
            lan_transfer::lan_pause_transfer,
            lan_transfer::lan_resume_transfer,
            lan_transfer::lan_cancel_transfer,
            lan_transfer::lan_get_history,
            lan_transfer::lan_clear_history,
            lan_transfer::lan_list_trusted,
            lan_transfer::lan_unpair,
            lan_transfer::lan_pair,
            lan_transfer::lan_respond_pair,
            lan_transfer::lan_get_conversation,
            lan_transfer::lan_open_path,
            lan_transfer::lan_reveal_path,
            lan_transfer::lan_delete_file,
            lan_transfer::lan_copy_file_to_clipboard,
            lan_transfer::lan_clipboard_files,
            lan_transfer::lan_start,
            lan_transfer::lan_stop,
            lan_transfer::lan_pick_files,
            lan_transfer::lan_pick_save_directory
        ])
        .setup(move |app| {
            app.manage(Arc::new(settings_store::SettingsStore::open(app.path().app_data_dir()?.join("airport-settings.json"))));
            {
                // 局域网传输服务状态，生命周期独立于代理连接（见 docs/desktop/局域网文件传输设计.md 第 6 节）。
                let lan_dir = app.path().app_data_dir()?.join("lan");
                fs::create_dir_all(&lan_dir)?;
                let save_dir = app.path().download_dir().unwrap_or_else(|_| lan_dir.clone());
                // 默认别名用机器名，避免多台设备都叫 “ClashNova” 无法区分（取不到则回退）。
                let alias = {
                    let host = tauri_plugin_os::hostname();
                    let host = host.strip_suffix(".local").unwrap_or(&host).trim();
                    if host.is_empty() { "ClashNova".to_string() } else { host.to_string() }
                };
                app.manage(lan_transfer::LanTransferState::new(airport_lan::LanConfig::new(
                    alias,
                    lan_dir,
                    save_dir,
                )));
                // 常驻拉起：接收端与组播发现随 App 启动运行，不依赖打开局域网页面。
                log::info!("[lan] setup: calling autostart");
                lan_transfer::autostart(app.handle());
            }
            // Keep the main window hidden at its real size. The webview shows it
            // once, after it knows whether this launch is the login card or the main window.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
                let _ = window.set_resizable(true);
                let _ = window.set_min_size(Some(tauri::LogicalSize::new(820.0, 650.0)));
                let _ = window.set_size(tauri::LogicalSize::new(960.0, 670.0));
            }
            {
                let dir = app.path().app_config_dir()?;
                fs::create_dir_all(&dir)?;
                let mut guard = core.lock().expect("connection state lock");
                guard.push_log("log.logRetention");
                guard.recovery_intent_path = Some(dir.join("connection-intent.json"));
                let watchdog = watchdog::Guard::start(&dir).unwrap_or_else(|error| {
                    guard.push_log(msg("recovery.supervisorFailed", &[("error", text(error))]));
                    watchdog::Guard::disabled()
                });
                app.manage(watchdog);
                guard.proxy_backup_path = Some(dir.join("proxy-backup.json"));
                guard.tun_credentials = Some(dir.join("tun-service.json"));
                // A previous run of the app may have ended while the kill switch was blocking.
                // This run knows nothing of that connection, so it starts with the network open.
                let credentials = dir.join("tun-service.json");
                thread::spawn(move || {
                    let _ = tun_service::release_guard(&credentials);
                });
                if let Err(error) = restore_saved_proxy(&mut guard) {
                    guard.push_log(error);
                }
                match network_settings::load(&dir.join("network-settings.json")) {
                    Ok(settings) => guard.network = settings,
                    Err(error) => guard.push_log(error),
                }
            }
            watch_child(app.handle().clone(), core.clone());
            tray::setup(app)?;
            shortcuts::setup(app.handle())?;
            tray_speed::start(app.handle().clone(), core.clone());
            connections::start(app.handle().clone(), core.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(
                event,
                WindowEvent::Focused(false) | WindowEvent::CloseRequested { .. }
            ) {
                shortcuts::end_recording(window.app_handle());
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("desktop client failed to start")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                // Mark an intentional exit before updater/relaunch or slow network cleanup.
                if let Err(error) = app.state::<watchdog::Guard>().stop() {
                    log::error!("Could not mark normal client exit: {error}");
                }
                probe::stop_all();
                let state = app.state::<Arc<Mutex<Core>>>();
                if let Ok(mut core) = state.lock() {
                    let _ = stop_core(&mut core);
                };
            }
            #[cfg(target_os = "macos")]
            if matches!(event, tauri::RunEvent::Reopen { .. }) {
                show_main(app);
            }
        });
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn subscription_files_distinguish_missing_empty_and_invalid_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subscriptions.json");
        assert_eq!(load_subscription_file(&path).unwrap(), "null");
        fs::write(&path, b"[]").unwrap();
        assert_eq!(load_subscription_file(&path).unwrap(), "[]");
        for invalid in ["broken", "null", "{}"] {
            fs::write(&path, invalid).unwrap();
            assert!(load_subscription_file(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        }
        fs::write(&path, vec![b' '; 16 * 1024 * 1024 + 1]).unwrap();
        assert!(load_subscription_file(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn subscription_files_do_not_follow_symbolic_links() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.json");
        fs::write(&original, b"[]").unwrap();
        let link = dir.path().join("subscriptions.json");
        std::os::unix::fs::symlink(&original, &link).unwrap();
        assert!(load_subscription_file(&link).is_err());
    }

    struct RunningCore(Core);
    impl Drop for RunningCore {
        fn drop(&mut self) {
            let _ = stop_core(&mut self.0);
        }
    }

    #[test]
    fn failed_start_and_repeated_stop_do_not_leave_a_child() {
        let mut core = Core::new();
        assert!(start_core(&mut core, None).is_err());
        assert_eq!(core.phase, Phase::Idle);
        stop_core(&mut core).unwrap();
        stop_core(&mut core).unwrap();
        assert!(core.child.is_none());
        assert!(!core.auto_reconnect);
    }

    #[test]
    fn manual_disconnect_cancels_retries_and_persisted_intent() {
        let dir = tempfile::tempdir().unwrap();
        let mut core = Core::new();
        core.recovery_intent_path = Some(dir.path().join("connection-intent.json"));
        core.record_recovery_intent(true);
        core.phase = Phase::Reconnecting;
        queue_core_recovery(&mut core, "test crash".into());
        stop_core(&mut core).unwrap();
        assert_eq!(core.phase, Phase::Idle);
        assert!(!core.auto_reconnect);
        assert!(!core.recovery.due(Instant::now() + Duration::from_secs(60)));
        assert_eq!(fs::read(core.recovery_intent_path.unwrap()).unwrap(), b"false");
    }

    #[test]
    fn repeated_short_core_runs_exhaust_recovery_and_clear_intent() {
        let dir = tempfile::tempdir().unwrap();
        let mut core = Core::new();
        core.recovery_intent_path = Some(dir.path().join("connection-intent.json"));
        core.record_recovery_intent(true);
        for _ in 0..5 {
            core.phase = Phase::Connected;
            handle_core_exit(&mut core, "crashed".into(), false);
            assert_eq!(core.phase, Phase::Reconnecting);
            assert!(core.auto_reconnect);
        }
        core.phase = Phase::Connected;
        handle_core_exit(&mut core, "crashed".into(), false);
        assert_eq!(core.phase, Phase::Failed);
        assert!(!core.auto_reconnect);
        assert_eq!(fs::read(core.recovery_intent_path.unwrap()).unwrap(), b"false");
    }

    #[cfg(unix)]
    #[test]
    fn uncertain_child_status_is_stopped_and_reaped_before_retry() {
        let mut core = Core::new();
        let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        core.child = Some(CoreProcess::Local(child));
        core.phase = Phase::Connected;
        handle_core_exit(&mut core, "status unavailable".into(), true);
        assert!(core.child.is_none());
        assert_eq!(core.phase, Phase::Reconnecting);
        // This child has been reaped, so it is no longer in our process table.
        let result = Command::new("/bin/kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status().unwrap();
        assert!(!result.success());
    }

    #[test]
    fn failed_proxy_cleanup_blocks_recovery_and_does_not_report_connected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("proxy-backup.json");
        fs::write(&path, b"invalid backup").unwrap();
        let mut core = Core::new();
        core.proxy_backup_path = Some(path.clone());
        core.phase = Phase::Connected;
        handle_core_exit(&mut core, "crashed".into(), false);
        assert_eq!(core.phase, Phase::Failed);
        assert!(!core.auto_reconnect);
        assert!(path.exists());
        assert!(!core.recovery.due(Instant::now() + Duration::from_secs(60)));
    }

    #[test]
    fn rejects_occupied_port_before_spawning() {
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"inbounds":[{"type":"mixed"}]}"#).unwrap();
        let mut core = Core::new();
        core.config_path = Some(path);
        core.network.proxy_port = occupied.local_addr().unwrap().port();
        assert!(start_core(&mut core, None).is_err());
        assert_eq!(core.phase, Phase::Failed);
        assert!(core.child.is_none());
    }

    #[test]
    #[ignore = "requires AIRPORT_TEST_PROFILE and a working real node"]
    fn real_node_custom_port_connect_disconnect() {
        let raw = fs::read_to_string(std::env::var("AIRPORT_TEST_PROFILE").unwrap()).unwrap();
        let mut config: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let nodes = config["outbounds"].as_array().unwrap();
        let preferred = std::env::var("AIRPORT_TEST_NODE").unwrap_or_default();
        let node = nodes
            .iter()
            .find(|node| node["tag"] == preferred && node["server"].is_string())
            .or_else(|| nodes.iter().find(|node| node["server"].is_string()))
            .unwrap();
        let tag = node["tag"].clone();
        config.as_object_mut().unwrap().remove("experimental");
        config["route"] = serde_json::json!({"final": tag});
        config["inbounds"] = serde_json::json!([{"type":"mixed","tag":"mixed-in","listen":"127.0.0.1","listen_port":2080}]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        storage::write_private(&path, config.to_string().as_bytes()).unwrap();
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let mut running = RunningCore(Core::new());
        running.0.mode = "manual".into();
        running.0.network = NetworkSettings {
            proxy_port: port,
            allow_lan: true,
            ..Default::default()
        };
        running.0.config_path = Some(path.clone());
        running.0.proxy_backup_path = Some(dir.path().join("proxy-backup.json"));
        start_core(&mut running.0, None).expect("real proxy connection");
        assert_eq!(running.0.phase, Phase::Connected);
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["inbounds"][0]["listen"], "0.0.0.0");
        assert_eq!(saved["inbounds"][0]["listen_port"], port);
        check_proxy_egress(port).expect("HTTPS through custom mixed port");
        let exit = tauri::async_runtime::block_on(network_info::query_exit_info(Some(port)))
            .expect("proxy exit IP query");
        assert!(exit.via_proxy);
        assert!(exit.ip.parse::<std::net::IpAddr>().is_ok());
        stop_core(&mut running.0).unwrap();
        assert_eq!(running.0.phase, Phase::Idle);
        assert!(running.0.child.is_none());
        assert!(TcpListener::bind(("0.0.0.0", port)).is_ok());
        assert!(!running.0.proxy_backup_path.as_ref().unwrap().exists());
    }
}
