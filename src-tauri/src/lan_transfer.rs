//! Tauri 适配层：把 airport-lan 的 `LanService` 暴露为 Tauri 命令。
//!
//! Files and approved text messages share device discovery and the LAN listener.
//!
//! 约定：命令参数用单字名（`task`/`dir`/`paths`…），规避前端 camelCase 与
//! Rust snake_case 的映射歧义，与项目既有命令风格一致。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use airport_lan::protocol::DeviceType;
use airport_lan::transfer::TransferTask;
use airport_lan::{IncomingDecision, LanConfig, LanError, LanEvent, LanService};

/// Managed state：持有配置与惰性创建的服务（创建会打开本地 SQLite）。
pub struct LanTransferState {
    config: Mutex<LanConfig>,
    service: Mutex<Option<Arc<LanService>>>,
    /// 事件转发器是否已挂上（只挂一次）。
    started: Mutex<bool>,
}

impl LanTransferState {
    pub fn new(config: LanConfig) -> Self {
        Self {
            config: Mutex::new(config),
            service: Mutex::new(None),
            started: Mutex::new(false),
        }
    }

    /// 惰性获取（或创建）服务。锁在本函数内释放，调用方可安全 `.await`。
    fn service(&self) -> Result<Arc<LanService>, LanError> {
        let mut guard = self.service.lock().expect("lan service lock");
        if let Some(svc) = guard.as_ref() {
            return Ok(svc.clone());
        }
        let config = self.config.lock().expect("lan config lock").clone();
        let svc = Arc::new(LanService::new(config)?);
        *guard = Some(svc.clone());
        Ok(svc)
    }
}

/// 把核心错误映射为前端可翻译的 i18n key。
fn map_err(err: LanError) -> String {
    match err {
        LanError::Unimplemented(_) => "native.lanNotImplemented",
        LanError::InvalidText => "native.lanInvalidText",
        LanError::TextUnsupported => "native.lanTextUnsupported",
        LanError::Rejected => "native.lanRejected",
        _ => "native.lanError",
    }
    .to_string()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IncomingFileView {
    name: String,
    size: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IncomingRequestView {
    task_id: String,
    peer_alias: String,
    files: Vec<IncomingFileView>,
    total_size: u64,
    text: Option<String>,
}

/// 把核心事件转发为前端 `lan://*` 事件。
fn emit_event(app: &AppHandle, event: LanEvent) {
    match event {
        LanEvent::DevicesChanged(devices) => {
            let _ = app.emit("lan://devices", devices);
        }
        LanEvent::IncomingRequest {
            task_id,
            peer_alias,
            files,
            total_size,
            text,
        } => {
            let view = IncomingRequestView {
                task_id,
                peer_alias,
                files: files
                    .into_iter()
                    .map(|(name, size)| IncomingFileView { name, size })
                    .collect(),
                total_size,
                text,
            };
            let _ = app.emit("lan://incoming", view);
        }
        LanEvent::TransferChanged(task) => {
            let _ = app.emit("lan://transfer", task);
        }
        LanEvent::TextReceived(message) => {
            let _ = app.emit("lan://text", message);
        }
        LanEvent::Progress(progress) => {
            let _ = app.emit("lan://progress", progress);
        }
        LanEvent::PairRequest {
            request_id,
            peer_alias,
            peer_fingerprint,
        } => {
            let _ = app.emit(
                "lan://pair",
                PairRequestView {
                    request_id,
                    peer_alias,
                    peer_fingerprint,
                },
            );
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PairRequestView {
    request_id: String,
    peer_alias: String,
    peer_fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDeviceView {
    pub alias: String,
    pub fingerprint: String,
    pub device_type: DeviceType,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanSettingsView {
    pub alias: String,
    pub discoverable: bool,
    pub accept_incoming: bool,
    pub auto_accept_trusted: bool,
    pub default_save_dir: String,
    pub max_concurrent: u32,
}

// 历史记录直接复用 airport_lan::storage::HistoryRow（已是 camelCase 可序列化）。

fn settings_view(config: &LanConfig) -> LanSettingsView {
    LanSettingsView {
        alias: config.alias.clone(),
        discoverable: config.discoverable,
        accept_incoming: config.accept_incoming,
        auto_accept_trusted: config.auto_accept_trusted,
        default_save_dir: config.default_save_dir.to_string_lossy().into_owned(),
        max_concurrent: config.max_concurrent as u32,
    }
}

#[tauri::command]
pub async fn lan_get_local_device(
    state: State<'_, LanTransferState>,
) -> Result<LocalDeviceView, String> {
    let service = state.service().map_err(map_err)?;
    let (alias, port) = {
        let config = state.config.lock().expect("lan config lock");
        (config.alias.clone(), config.listen_port)
    };
    Ok(LocalDeviceView {
        alias,
        fingerprint: service.fingerprint().to_string(),
        device_type: DeviceType::Desktop,
        port,
    })
}

/// 手动按地址添加设备（LAN-001 手动连接）：接受 `IP` 或 `IP:端口`，拉取 /info 并登记。
#[tauri::command]
pub async fn lan_add_manual_device(
    state: State<'_, LanTransferState>,
    addr: String,
) -> Result<airport_lan::DiscoveredDevice, String> {
    let socket = addr
        .trim()
        .parse::<std::net::SocketAddr>()
        .or_else(|_| {
            addr.trim()
                .parse::<std::net::IpAddr>()
                .map(|ip| std::net::SocketAddr::new(ip, 53317))
        })
        .map_err(|_| "native.lanInvalidAddress".to_string())?;
    state
        .service()
        .map_err(map_err)?
        .add_manual_device(socket)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn lan_get_settings(
    state: State<'_, LanTransferState>,
) -> Result<LanSettingsView, String> {
    let mut view = {
        let config = state.config.lock().expect("lan config lock");
        settings_view(&config)
    };
    // 别名/可发现/保存目录以运行中的服务为准（持久化后的实时值）。
    let service = state.service().map_err(map_err)?;
    view.alias = service.alias();
    view.discoverable = service.discoverable();
    view.default_save_dir = service.save_directory().to_string_lossy().into_owned();
    Ok(view)
}

#[tauri::command]
pub async fn lan_update_settings(
    state: State<'_, LanTransferState>,
    settings: LanSettingsView,
) -> Result<LanSettingsView, String> {
    let service = state.service().map_err(map_err)?;
    let directory = if PathBuf::from(&settings.default_save_dir) == service.save_directory() {
        service.save_directory()
    } else {
        service
            .set_save_directory(std::path::Path::new(&settings.default_save_dir))
            .map_err(|_| "native.lanSaveDirectoryFailed".to_string())?
    };
    // 设备名 / 可被发现：持久化并即时作用到运行中的服务（信标重播新名、启停发现）。
    service.set_alias(&settings.alias).await.map_err(map_err)?;
    service
        .set_discoverable(settings.discoverable)
        .await
        .map_err(map_err)?;
    let view = {
        let mut config = state.config.lock().expect("lan config lock");
        config.alias = settings.alias;
        config.discoverable = settings.discoverable;
        config.accept_incoming = settings.accept_incoming;
        config.auto_accept_trusted = settings.auto_accept_trusted;
        config.default_save_dir = directory;
        config.max_concurrent = settings.max_concurrent.max(1) as usize;
        settings_view(&config)
    };
    Ok(view)
}

#[tauri::command]
pub async fn lan_refresh_devices(state: State<'_, LanTransferState>) -> Result<(), String> {
    let svc = state.service().map_err(map_err)?;
    svc.refresh_devices().await.map_err(map_err)
}

#[tauri::command]
pub async fn lan_list_transfers(
    state: State<'_, LanTransferState>,
) -> Result<Vec<TransferTask>, String> {
    Ok(state.service().map_err(map_err)?.transfers())
}

#[tauri::command]
pub async fn lan_send_files(
    state: State<'_, LanTransferState>,
    fingerprint: String,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    let svc = state.service().map_err(map_err)?;
    let paths = paths.into_iter().map(PathBuf::from).collect();
    svc.send_files(&fingerprint, paths).await.map_err(map_err)
}

#[tauri::command]
pub async fn lan_send_text(
    state: State<'_, LanTransferState>,
    fingerprint: String,
    text: String,
) -> Result<airport_lan::storage::HistoryRow, String> {
    state
        .service()
        .map_err(map_err)?
        .send_text(&fingerprint, text)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn lan_respond_incoming(
    state: State<'_, LanTransferState>,
    task: String,
    accept: bool,
    dir: Option<String>,
) -> Result<(), String> {
    let svc = state.service().map_err(map_err)?;
    let decision = if accept {
        IncomingDecision::Accept {
            save_dir: dir.map(PathBuf::from),
        }
    } else {
        IncomingDecision::Reject
    };
    svc.respond_incoming(&task, decision).await.map_err(map_err)
}

/// 向某设备发起配对（LAN-007）。配对成功后双方互信、自动接收。
#[tauri::command]
pub async fn lan_pair(
    state: State<'_, LanTransferState>,
    fingerprint: String,
) -> Result<(), String> {
    state
        .service()
        .map_err(map_err)?
        .pair(&fingerprint)
        .await
        .map_err(map_err)
}

/// 响应收到的配对请求（接受即把对端记为可信）。
#[tauri::command]
pub async fn lan_respond_pair(
    state: State<'_, LanTransferState>,
    request: String,
    accept: bool,
) -> Result<(), String> {
    state
        .service()
        .map_err(map_err)?
        .respond_pair(&request, accept)
        .await
        .map_err(map_err)
}

/// 取某设备的会话时间线（文本+文件，按时间正序）。
#[tauri::command]
pub async fn lan_get_conversation(
    state: State<'_, LanTransferState>,
    fingerprint: String,
) -> Result<Vec<airport_lan::storage::HistoryRow>, String> {
    Ok(state.service().map_err(map_err)?.conversation(&fingerprint))
}

fn recorded_file_path(
    storage: &airport_lan::storage::Storage,
    path: &str,
) -> Result<PathBuf, String> {
    let parsed = std::path::Path::new(path);
    if path.contains('\0')
        || !parsed.is_absolute()
        || !storage.contains_file_path(path).map_err(map_err)?
    {
        return Err("native.lanFileUnavailable".into());
    }
    Ok(parsed.to_path_buf())
}

/// Open only a file whose local path belongs to transfer history.
#[tauri::command]
pub async fn lan_open_path(
    app: AppHandle,
    state: State<'_, LanTransferState>,
    path: String,
) -> Result<(), String> {
    let storage = state.service().map_err(map_err)?.storage().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = recorded_file_path(&storage, &path)?
            .canonicalize()
            .map_err(|_| "native.lanFileUnavailable".to_string())?;
        if !path.is_file() {
            return Err("native.lanFileUnavailable".into());
        }
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reveal the transferred file, or open its parent if the file has been removed.
#[tauri::command]
pub async fn lan_reveal_path(
    app: AppHandle,
    state: State<'_, LanTransferState>,
    path: String,
) -> Result<(), String> {
    let storage = state.service().map_err(map_err)?.storage().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = recorded_file_path(&storage, &path)?;
        if let Ok(file) = path.canonicalize() {
            if file.is_file() {
                return app
                    .opener()
                    .reveal_item_in_dir(file)
                    .map_err(|e| e.to_string());
            }
        }
        let parent = path
            .parent()
            .and_then(|dir| dir.canonicalize().ok())
            .filter(|dir| dir.is_dir())
            .ok_or_else(|| "native.lanFileUnavailable".to_string())?;
        app.opener()
            .open_path(parent.to_string_lossy(), None::<&str>)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 删除原文件并移除该条历史记录（右键菜单“删除”）。
#[tauri::command]
pub async fn lan_delete_file(
    state: State<'_, LanTransferState>,
    id: String,
    path: Option<String>,
) -> Result<(), String> {
    if let Some(p) = path.as_ref() {
        // 文件已不在不算失败（可能被手动删过）。
        if std::path::Path::new(p).exists() {
            std::fs::remove_file(p).map_err(|e| e.to_string())?;
        }
    }
    state
        .service()
        .map_err(map_err)?
        .remove_history(&id)
        .map_err(map_err)
}

/// 把文件放入系统剪贴板（右键“复制”）：可在访达/资源管理器直接粘贴该文件。
#[tauri::command]
pub async fn lan_copy_file_to_clipboard(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || copy_file_clipboard(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// 读取系统剪贴板中的文件路径（右键“粘贴”即把这些文件发给对方）。
#[tauri::command]
pub async fn lan_clipboard_files() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(clipboard_files)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn run_ok(cmd: &mut std::process::Command) -> std::io::Result<()> {
    if cmd.status()?.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("clipboard command failed"))
    }
}

/// 把文件写入系统剪贴板（跨平台）。
fn copy_file_clipboard(path: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let esc = path.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!("set the clipboard to POSIX file \"{esc}\"");
        run_ok(std::process::Command::new("osascript").args(["-e", &script]))?;
    }
    #[cfg(target_os = "windows")]
    {
        let esc = path.replace('\'', "''");
        let script = format!("Set-Clipboard -LiteralPath '{esc}'");
        run_ok(std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::io::Write;
        let mut child = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "text/uri-list"])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() {
            write!(stdin, "file://{path}\n")?;
        }
        child.wait()?;
    }
    Ok(())
}

/// 读取系统剪贴板里的文件路径（跨平台，best-effort；非文件内容返回空）。
fn clipboard_files() -> std::io::Result<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("osascript")
            .args(["-e", "POSIX path of (the clipboard as «class furl»)"])
            .output()?;
        if !out.status.success() {
            return Ok(vec![]);
        }
        Ok(parse_lines(&out.stdout))
    }
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-Clipboard -Format FileDropList) | ForEach-Object { $_.FullName }",
            ])
            .output()?;
        if !out.status.success() {
            return Ok(vec![]);
        }
        Ok(parse_lines(&out.stdout))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let out = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "text/uri-list", "-o"])
            .output()?;
        if !out.status.success() {
            return Ok(vec![]);
        }
        Ok(parse_lines(&out.stdout)
            .into_iter()
            .map(|l| l.strip_prefix("file://").map(percent_decode).unwrap_or(l))
            .collect())
    }
}

fn parse_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tauri::command]
pub async fn lan_pause_transfer(
    state: State<'_, LanTransferState>,
    task: String,
) -> Result<(), String> {
    state
        .service()
        .map_err(map_err)?
        .pause(&task)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn lan_resume_transfer(
    state: State<'_, LanTransferState>,
    task: String,
) -> Result<(), String> {
    state
        .service()
        .map_err(map_err)?
        .resume(&task)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn lan_cancel_transfer(
    state: State<'_, LanTransferState>,
    task: String,
) -> Result<(), String> {
    state
        .service()
        .map_err(map_err)?
        .cancel(&task)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn lan_get_history(
    state: State<'_, LanTransferState>,
) -> Result<Vec<airport_lan::storage::HistoryRow>, String> {
    state
        .service()
        .map_err(map_err)?
        .storage()
        .list_history(200)
        .map_err(map_err)
}

#[tauri::command]
pub async fn lan_clear_history(state: State<'_, LanTransferState>) -> Result<(), String> {
    state
        .service()
        .map_err(map_err)?
        .clear_history()
        .map_err(map_err)
}

/// 可信设备列表（TOFU 配对）。
#[tauri::command]
pub async fn lan_list_trusted(
    state: State<'_, LanTransferState>,
) -> Result<Vec<airport_lan::storage::TrustedRow>, String> {
    Ok(state.service().map_err(map_err)?.trusted_devices())
}

/// 解除某设备配对（从可信列表移除）。
#[tauri::command]
pub async fn lan_unpair(
    state: State<'_, LanTransferState>,
    fingerprint: String,
) -> Result<(), String> {
    state
        .service()
        .map_err(map_err)?
        .unpair(&fingerprint)
        .map_err(map_err)
}

/// 启动接收端与发现，并把核心事件转发到前端（转发器只挂一次）。幂等。
///
/// 既由 App 启动时常驻拉起（[`autostart`]），也由前端进入局域网页面时调用，
/// 两者幂等叠加：服务只会真正启动一次，事件转发器也只挂一次。
pub async fn ensure_started(app: &AppHandle, state: &LanTransferState) -> Result<(), String> {
    log::info!("[lan] ensure_started: begin");
    let service = state.service().map_err(|e| {
        log::warn!("[lan] ensure_started: service() failed: {e:?}");
        map_err(e)
    })?;
    log::info!(
        "[lan] ensure_started: service ready, fingerprint={}",
        service.fingerprint()
    );
    let first = {
        let mut started = state.started.lock().expect("lan started lock");
        let was = *started;
        *started = true;
        !was
    };
    if first {
        let mut rx = service.subscribe();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => emit_event(&app, event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
    }
    log::info!("[lan] ensure_started: calling service.start()");
    match service.start().await {
        Ok(()) => {
            log::info!(
                "[lan] ensure_started: start OK, local_addr={:?}",
                service.local_addr().await
            );
            Ok(())
        }
        Err(e) => {
            log::warn!("[lan] ensure_started: start failed: {e:?}");
            Err(map_err(e))
        }
    }
}

/// App 启动即常驻拉起局域网服务：接收端与组播发现不再依赖打开「局域网」页面。
/// 失败仅告警，不阻断 App 启动（用户之后进页面会再次尝试）。
pub fn autostart(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        log::info!("[lan] autostart: task entered");
        let state = app.state::<LanTransferState>();
        if let Err(err) = ensure_started(&app, state.inner()).await {
            log::warn!("[lan] autostart: LAN service startup failed: {err}");
        }
    });
}

/// 前端进入局域网页面时调用（常驻已起时为幂等空操作）。
#[tauri::command]
pub async fn lan_start(app: AppHandle, state: State<'_, LanTransferState>) -> Result<(), String> {
    ensure_started(&app, state.inner()).await
}

/// 停止接收端与发现。
#[tauri::command]
pub async fn lan_stop(state: State<'_, LanTransferState>) -> Result<(), String> {
    state
        .service()
        .map_err(map_err)?
        .stop()
        .await
        .map_err(map_err)
}

/// 弹出系统文件选择框，返回所选文件的绝对路径（用于发送）。
#[tauri::command]
pub async fn lan_pick_files(app: AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_files(move |files| {
        let _ = tx.send(files);
    });
    let files = rx.await.map_err(|_| "native.lanError".to_string())?;
    Ok(files
        .into_iter()
        .flatten()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

/// Choose and persist the LAN receive directory using the system folder picker.
#[tauri::command]
pub async fn lan_pick_save_directory(
    app: AppHandle,
    state: State<'_, LanTransferState>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let service = state.service().map_err(map_err)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_directory(service.save_directory())
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });
    let selected = rx
        .await
        .map_err(|_| "native.lanSaveDirectoryFailed".to_string())?;
    let Some(folder) = selected else {
        return Ok(None);
    };
    let folder = folder
        .into_path()
        .map_err(|_| "native.lanSaveDirectoryFailed".to_string())?;
    let result = tauri::async_runtime::spawn_blocking(move || service.set_save_directory(&folder))
        .await
        .map_err(|_| "native.lanSaveDirectoryFailed".to_string())?
        .map_err(|_| "native.lanSaveDirectoryFailed".to_string())?;
    state
        .config
        .lock()
        .expect("lan config lock")
        .default_save_dir = result.clone();
    Ok(Some(result.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod file_action_tests {
    use super::*;
    #[test]
    fn file_actions_are_limited_to_recorded_absolute_paths() {
        let storage = airport_lan::storage::Storage::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recorded document.pdf");
        std::fs::write(&file, b"test").unwrap();
        let path = file.to_string_lossy().into_owned();
        assert!(recorded_file_path(&storage, &path).is_err());
        storage.with_conn(|conn| {
            conn.execute("INSERT INTO transfer_history (id,direction,peer_alias,peer_fingerprint,file_name,size,state,save_path) VALUES ('file','receive','peer','fp','recorded document.pdf',4,'COMPLETED',?1)", [&path])?;
            Ok(())
        }).unwrap();
        assert_eq!(recorded_file_path(&storage, &path).unwrap(), file);
        assert!(recorded_file_path(&storage, "https://example.test").is_err());
        assert!(recorded_file_path(&storage, "relative.pdf").is_err());
        assert!(recorded_file_path(&storage, &format!("{path}' OR 1=1 --")).is_err());
        storage.clear_history().unwrap();
        assert!(recorded_file_path(&storage, &path).is_err());
    }
}
