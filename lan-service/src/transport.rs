//! LocalSend v2 兼容传输：接收端 HTTP 服务 + 发送端客户端。
//!
//! 当前为明文 HTTP（`protocol = http`）。TLS（rustls 自签 + TOFU 指纹）、设备发现、
//! 进度/审批事件在后续迭代接入（见 docs/desktop/局域网文件传输设计.md 第 4、5、7 节）。
//!
//! 已实现：prepare-upload 会话协商、分块流式接收、SHA-256 校验、临时文件原子重命名、
//! 文件名安全校验（防目录穿越）。

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::error::{LanError, Result};
use crate::protocol::{
    DeviceInfo, FileDto, PrepareUploadRequest, PrepareUploadResponse, API_PREFIX,
};
use crate::security::resolve_within;
use crate::transfer::{
    Direction, TransferSink, TransferState, TransferTask, TransferUpdate, PART_SUFFIX,
};

/// 接收端对一次 prepare-upload 的处置（LAN-003/007）。
pub enum AcceptDecision {
    Accept { save_dir: PathBuf },
    Reject,
}

/// 接收授权策略。`decide` 可异步等待（UI 审批，WAITING_APPROVAL）。
#[async_trait]
pub trait AcceptPolicy: Send + Sync {
    async fn decide(
        &self,
        session_id: &str,
        sender: &DeviceInfo,
        files: &HashMap<String, FileDto>,
    ) -> AcceptDecision;

    /// 配对请求审批（LAN-007 显式配对）。默认不支持；交互式实现会提示用户确认，
    /// 接受则把对端记为可信（配对即信任）。
    async fn decide_pair(&self, _sender: &DeviceInfo) -> bool {
        false
    }
}

/// 显式配对端点（自有扩展，LocalSend 规范外）。
pub const PAIR_PATH: &str = "/api/clashnova/v1/pair";

/// 无条件接收到固定目录（仅测试与受控场景）。
pub struct AcceptAll {
    pub save_dir: PathBuf,
}
#[async_trait]
impl AcceptPolicy for AcceptAll {
    async fn decide(
        &self,
        _id: &str,
        _s: &DeviceInfo,
        _f: &HashMap<String, FileDto>,
    ) -> AcceptDecision {
        AcceptDecision::Accept {
            save_dir: self.save_dir.clone(),
        }
    }
}

/// 一律拒绝。安全默认：审批流接入前，运行中的服务不写入任何文件。
pub struct RejectAll;
#[async_trait]
impl AcceptPolicy for RejectAll {
    async fn decide(
        &self,
        _id: &str,
        _s: &DeviceInfo,
        _f: &HashMap<String, FileDto>,
    ) -> AcceptDecision {
        AcceptDecision::Reject
    }
}

/// 一次发现到的对端（IP 取自连接来源，端口取自其 DeviceInfo）。
pub struct Discovered {
    pub info: DeviceInfo,
    pub addr: SocketAddr,
}

/// 发现回填通道：接收端把看到的对端推给上层注册表。
pub type DiscoverySink = tokio::sync::mpsc::UnboundedSender<Discovered>;

/// 任务取消登记表：task_id → 取消令牌，收发两端共享。
pub type CancelRegistry = Arc<Mutex<HashMap<String, tokio_util::sync::CancellationToken>>>;

/// 构造一个空的取消登记表。
pub fn cancel_registry() -> CancelRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

/// 续传偏移查询端点（自有扩展，LocalSend 规范外；标准 LocalSend 客户端不调用）。
const RESUME_OFFSET_PATH: &str = "/api/clashnova/v1/upload-offset";

/// 上传应答：接收端当前已落盘的字节数（用于断点续传偏移协商）。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadAck {
    received: u64,
}

struct PendingFile {
    token: String,
    dto: FileDto,
    /// 稳定任务 id：同一文件的多次（续传）上传共享，算同一任务。
    task_id: String,
}

struct Session {
    save_dir: PathBuf,
    files: HashMap<String, PendingFile>,
    peer_alias: String,
    peer_fingerprint: String,
}

#[derive(Clone)]
struct AppState {
    policy: Arc<dyn AcceptPolicy>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    local: Arc<DeviceInfo>,
    discovered: Option<DiscoverySink>,
    transfers: Option<TransferSink>,
    texts: Option<Arc<crate::messages::TextInbox>>,
    cancels: CancelRegistry,
}

/// 运行中的接收端。丢弃前请调用 [`shutdown`](Receiver::shutdown)。
pub struct Receiver {
    addr: SocketAddr,
    handle: axum_server::Handle,
    join: tokio::task::JoinHandle<()>,
}

impl Receiver {
    /// 在 `bind` 上启动接收端（明文 HTTP）。端口为 0 时由系统分配。
    pub async fn start(
        bind: SocketAddr,
        policy: Arc<dyn AcceptPolicy>,
        local: DeviceInfo,
        discovered: Option<DiscoverySink>,
        transfers: Option<TransferSink>,
    ) -> Result<Self> {
        Self::start_with_messages(
            bind,
            policy,
            local,
            discovered,
            transfers,
            None,
            None,
            cancel_registry(),
        )
        .await
    }
    pub async fn start_with_messages(
        bind: SocketAddr,
        policy: Arc<dyn AcceptPolicy>,
        local: DeviceInfo,
        discovered: Option<DiscoverySink>,
        transfers: Option<TransferSink>,
        texts: Option<Arc<crate::messages::TextInbox>>,
        tls: Option<axum_server::tls_rustls::RustlsConfig>,
        cancels: CancelRegistry,
    ) -> Result<Self> {
        let state = AppState {
            policy,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            local: Arc::new(local),
            discovered,
            transfers,
            texts,
            cancels,
        };
        let app = Router::new()
            .route(
                crate::messages::TEXT_PATH,
                post(receive_text).layer(DefaultBodyLimit::max(512 * 1024)),
            )
            .route(
                &format!("{API_PREFIX}/prepare-upload"),
                post(prepare_upload),
            )
            .route(
                &format!("{API_PREFIX}/upload"),
                post(upload).layer(DefaultBodyLimit::disable()),
            )
            .route(RESUME_OFFSET_PATH, post(upload_offset))
            .route(&format!("{API_PREFIX}/cancel"), post(cancel))
            .route(&format!("{API_PREFIX}/register"), post(register))
            .route(PAIR_PATH, post(pair))
            .route(&format!("{API_PREFIX}/info"), get(info))
            .with_state(state);

        let handle = axum_server::Handle::new();
        let service = app.into_make_service_with_connect_info::<SocketAddr>();
        let srv_handle = handle.clone();
        let join = tokio::spawn(async move {
            let result = match tls {
                Some(config) => {
                    // 强制 mTLS：自定义 acceptor 握手后取出已验证的客户端证书指纹注入请求。
                    axum_server::bind(bind)
                        .handle(srv_handle)
                        .acceptor(crate::tls::PeerCertAcceptor::new(config))
                        .serve(service)
                        .await
                }
                None => {
                    axum_server::bind(bind)
                        .handle(srv_handle)
                        .serve(service)
                        .await
                }
            };
            if let Err(err) = result {
                log::warn!("LAN 接收端退出: {err}");
            }
        });
        // 等待实际绑定地址（端口为 0 时由系统分配）。
        let addr = tokio::time::timeout(Duration::from_secs(5), handle.listening())
            .await
            .ok()
            .flatten()
            .ok_or_else(|| LanError::Config("LAN 接收端未能绑定监听地址".to_string()))?;
        Ok(Self { addr, handle, join })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// 停止服务并等待后台任务结束。
    pub async fn shutdown(self) {
        self.handle.graceful_shutdown(Some(Duration::from_secs(1)));
        let _ = self.join.await;
    }
}

/// 把对端记入发现回填通道（忽略无指纹的匿名对端）。
fn note_peer(state: &AppState, info: &DeviceInfo, ip: IpAddr) {
    if info.fingerprint.is_empty() {
        return;
    }
    if let Some(sink) = &state.discovered {
        let port = info.port.unwrap_or(crate::protocol::DEFAULT_PORT);
        let _ = sink.send(Discovered {
            info: info.clone(),
            addr: SocketAddr::new(ip, port),
        });
    }
}

/// 从请求扩展取出 mTLS 握手验证过的对端指纹（[`crate::tls::PeerFingerprint`]）。
/// 未走 mTLS / 对端未出示证书时为 `None`（匿名，不授予任何信任）。
struct VerifiedPeer(Option<String>);

#[async_trait]
impl<St: Send + Sync> axum::extract::FromRequestParts<St> for VerifiedPeer {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &St,
    ) -> std::result::Result<Self, Self::Rejection> {
        Ok(VerifiedPeer(
            parts
                .extensions
                .get::<crate::tls::PeerFingerprint>()
                .map(|p| p.0.clone()),
        ))
    }
}

/// 用 TLS 握手验证过的指纹覆盖对端自报指纹；无已验证指纹（匿名）则置空，
/// 从而既不被 `is_trusted` 命中、也不会被 `note_peer` 记入发现。
fn with_verified_fp(mut info: DeviceInfo, verified: Option<String>) -> DeviceInfo {
    info.fingerprint = verified.unwrap_or_default();
    info
}

/// GET /info：返回本机设备信息（调试/手动发现用）。
async fn info(State(state): State<AppState>) -> Json<DeviceInfo> {
    Json((*state.local).clone())
}

/// POST /pair：对端发起配对；提示本机用户确认，接受则互记可信并返回本机信息。
async fn pair(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    VerifiedPeer(peer_fp): VerifiedPeer,
    Json(info): Json<DeviceInfo>,
) -> Response {
    // 以 mTLS 验证过的证书指纹为准（匿名=空指纹，下面即拒）。
    let info = with_verified_fp(info, peer_fp);
    if info.fingerprint.is_empty() || info.fingerprint.len() > 256 || info.alias.len() > 256 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    note_peer(&state, &info, peer.ip());
    if state.policy.decide_pair(&info).await {
        Json((*state.local).clone()).into_response()
    } else {
        StatusCode::FORBIDDEN.into_response()
    }
}

/// POST /register：对端登记自己的设备信息；记录之并返回本机信息。
async fn register(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    VerifiedPeer(peer_fp): VerifiedPeer,
    Json(info): Json<DeviceInfo>,
) -> Json<DeviceInfo> {
    let info = with_verified_fp(info, peer_fp);
    note_peer(&state, &info, peer.ip());
    Json((*state.local).clone())
}

async fn receive_text(
    State(state): State<AppState>,
    VerifiedPeer(peer_fp): VerifiedPeer,
    Json(mut request): Json<crate::messages::TextRequest>,
) -> StatusCode {
    match state.texts {
        Some(inbox) => {
            // 文本消息的信任同样以已验证指纹为准，不采信请求体自报。
            request.info = with_verified_fp(request.info, peer_fp);
            inbox.receive(request).await
        }
        None => StatusCode::NOT_FOUND,
    }
}

async fn prepare_upload(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    VerifiedPeer(peer_fp): VerifiedPeer,
    Json(req): Json<PrepareUploadRequest>,
) -> Response {
    let PrepareUploadRequest { info, files: req_files } = req;
    // 发送方身份以 mTLS 验证过的证书指纹为准，不采信请求体自报的 info.fingerprint。
    let info = with_verified_fp(info, peer_fp);
    if req_files
        .values()
        .any(|f| f.file_type == crate::messages::TEXT_TYPE)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    note_peer(&state, &info, peer.ip());
    let session_id = Uuid::new_v4().to_string();
    match state.policy.decide(&session_id, &info, &req_files).await {
        AcceptDecision::Reject => StatusCode::FORBIDDEN.into_response(),
        AcceptDecision::Accept { save_dir } => {
            let mut files = HashMap::new();
            let mut resp_files = HashMap::new();
            for (file_id, dto) in req_files {
                // 文件名安全校验（防目录穿越）；非法则拒绝整个会话。
                if resolve_within(&save_dir, &dto.file_name).is_err() {
                    return StatusCode::BAD_REQUEST.into_response();
                }
                let token = Uuid::new_v4().to_string();
                resp_files.insert(file_id.clone(), token.clone());
                files.insert(
                    file_id,
                    PendingFile {
                        token,
                        dto,
                        task_id: Uuid::new_v4().to_string(),
                    },
                );
            }
            state.sessions.lock().expect("sessions lock").insert(
                session_id.clone(),
                Session {
                    save_dir,
                    files,
                    peer_alias: info.alias.clone(),
                    peer_fingerprint: info.fingerprint.clone(),
                },
            );
            Json(PrepareUploadResponse {
                session_id,
                files: resp_files,
            })
            .into_response()
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadParams {
    session_id: String,
    file_id: String,
    token: String,
    /// 从该偏移开始追加（断点续传）；缺省为 0（整文件上传，LocalSend 兼容）。
    #[serde(default)]
    offset: Option<u64>,
}

async fn upload(
    State(state): State<AppState>,
    Query(params): Query<UploadParams>,
    body: Body,
) -> Response {
    // 校验会话/令牌，取出路径、期望哈希/大小与任务信息；锁只在此块内持有。
    let (final_path, temp_path, expected_sha, expected_size, task) = {
        let sessions = state.sessions.lock().expect("sessions lock");
        let Some(session) = sessions.get(&params.session_id) else {
            return StatusCode::CONFLICT.into_response();
        };
        let Some(pending) = session.files.get(&params.file_id) else {
            return StatusCode::FORBIDDEN.into_response();
        };
        if pending.token != params.token {
            return StatusCode::FORBIDDEN.into_response();
        }
        let final_path = match resolve_within(&session.save_dir, &pending.dto.file_name) {
            Ok(p) => p,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        };
        let mut temp = final_path.clone().into_os_string();
        temp.push(PART_SUFFIX);
        let task = TransferTask {
            id: pending.task_id.clone(),
            direction: Direction::Receive,
            peer_alias: session.peer_alias.clone(),
            peer_fingerprint: session.peer_fingerprint.clone(),
            file_name: pending.dto.file_name.clone(),
            size: pending.dto.size,
            state: TransferState::Transferring,
            save_path: None,
        };
        (
            final_path,
            PathBuf::from(temp),
            pending.dto.sha256.clone(),
            pending.dto.size,
            task,
        )
    };

    // 当前已落盘字节 = 临时文件大小；要求请求偏移与之一致（否则让发送端重查）。
    let current = tokio::fs::metadata(&temp_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if params.offset.unwrap_or(0) != current {
        return (StatusCode::CONFLICT, Json(UploadAck { received: current })).into_response();
    }

    // 稳定任务的取消令牌：跨续传调用共享。
    let token = state
        .cancels
        .lock()
        .expect("cancels lock")
        .entry(task.id.clone())
        .or_insert_with(tokio_util::sync::CancellationToken::new)
        .clone();
    // 首个分块时广播任务开始。
    if current == 0 {
        if let Some(sink) = &state.transfers {
            let _ = sink.send(TransferUpdate::Started(task.clone()));
        }
    }

    let mut reporter =
        ProgressReporter::new(state.transfers.clone(), task.id.clone(), expected_size);
    let result = receive_append(
        body,
        &temp_path,
        current,
        expected_size,
        &mut reporter,
        &token,
    )
    .await;

    match result {
        Ok(total) if total >= expected_size => {
            // 收满：校验整文件 SHA-256，原子重命名。
            let finalize = finalize_file(&temp_path, &final_path, expected_sha.as_deref()).await;
            finish_session_file(&state, &params.session_id, &params.file_id);
            state.cancels.lock().expect("cancels lock").remove(&task.id);
            let (final_state, status) = match &finalize {
                Ok(()) => (TransferState::Completed, StatusCode::OK),
                Err(LanError::ChecksumMismatch { .. }) => {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    (TransferState::Failed, StatusCode::UNPROCESSABLE_ENTITY)
                }
                Err(_) => {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    (TransferState::Failed, StatusCode::INTERNAL_SERVER_ERROR)
                }
            };
            if let Some(sink) = &state.transfers {
                let mut finished = task.clone();
                finished.state = final_state;
                if final_state == TransferState::Completed {
                    finished.save_path = Some(final_path.to_string_lossy().into_owned());
                }
                let _ = sink.send(TransferUpdate::Finished(finished));
            }
            (status, Json(UploadAck { received: total })).into_response()
        }
        Ok(total) => {
            // 部分完成（发送端提前结束）：保留临时文件与会话，等待续传。
            (StatusCode::OK, Json(UploadAck { received: total })).into_response()
        }
        Err(LanError::Canceled) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            finish_session_file(&state, &params.session_id, &params.file_id);
            state.cancels.lock().expect("cancels lock").remove(&task.id);
            if let Some(sink) = &state.transfers {
                let mut finished = task.clone();
                finished.state = TransferState::Canceled;
                let _ = sink.send(TransferUpdate::Finished(finished));
            }
            StatusCode::CONFLICT.into_response()
        }
        Err(_) => {
            // 网络中断：保留临时文件与会话（可续传），不删不报结束，令发送端重试。
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// 从会话移除已完结（完成/取消/失败）的文件；会话空则删除。
fn finish_session_file(state: &AppState, session_id: &str, file_id: &str) {
    let mut sessions = state.sessions.lock().expect("sessions lock");
    if let Some(session) = sessions.get_mut(session_id) {
        session.files.remove(file_id);
        if session.files.is_empty() {
            sessions.remove(session_id);
        }
    }
}

/// `POST /upload-offset`：返回该文件当前已落盘字节数，供发送端协商续传偏移。
async fn upload_offset(
    State(state): State<AppState>,
    Query(params): Query<UploadParams>,
) -> Response {
    let temp_path = {
        let sessions = state.sessions.lock().expect("sessions lock");
        let Some(session) = sessions.get(&params.session_id) else {
            return StatusCode::CONFLICT.into_response();
        };
        let Some(pending) = session.files.get(&params.file_id) else {
            return StatusCode::FORBIDDEN.into_response();
        };
        if pending.token != params.token {
            return StatusCode::FORBIDDEN.into_response();
        }
        let Ok(final_path) = resolve_within(&session.save_dir, &pending.dto.file_name) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let mut temp = final_path.into_os_string();
        temp.push(PART_SUFFIX);
        PathBuf::from(temp)
    };
    let received = tokio::fs::metadata(&temp_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    Json(UploadAck { received }).into_response()
}

/// 节流进度上报（≥200ms 或完成时发一次）。
struct ProgressReporter {
    sink: Option<TransferSink>,
    task_id: String,
    total: u64,
    last: Instant,
}

impl ProgressReporter {
    fn new(sink: Option<TransferSink>, task_id: String, total: u64) -> Self {
        Self {
            sink,
            task_id,
            total,
            last: Instant::now(),
        }
    }

    fn report(&mut self, done: u64) {
        let Some(sink) = &self.sink else {
            return;
        };
        let now = Instant::now();
        if done >= self.total || now.duration_since(self.last) >= Duration::from_millis(200) {
            self.last = now;
            let _ = sink.send(TransferUpdate::Progress {
                task_id: self.task_id.clone(),
                bytes_done: done,
                bytes_total: self.total,
            });
        }
    }
}

/// 从 `start_offset` 起把 body 分块追加到临时文件，返回新的总字节数。
/// 不做哈希与重命名（留给 [`finalize_file`]），以支持断点续传的多次追加。
async fn receive_append(
    body: Body,
    temp_path: &Path,
    start_offset: u64,
    expected_size: u64,
    reporter: &mut ProgressReporter,
    token: &tokio_util::sync::CancellationToken,
) -> Result<u64> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(temp_path)
        .await?;
    let mut stream = body.into_data_stream();
    let mut total = start_offset;
    loop {
        let next = tokio::select! {
            _ = token.cancelled() => return Err(LanError::Canceled),
            chunk = stream.next() => chunk,
        };
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.map_err(|e| LanError::Protocol(e.to_string()))?;
        file.write_all(&chunk).await?;
        total += chunk.len() as u64;
        reporter.report(total);
        if total >= expected_size {
            break;
        }
    }
    file.flush().await?;
    Ok(total)
}

/// 收满后校验整文件 SHA-256（流式重读），通过则原子重命名至目标文件。
async fn finalize_file(temp_path: &Path, final_path: &Path, expected: Option<&str>) -> Result<()> {
    if let Some(expected) = expected {
        let actual = crate::checksum::sha256_file(temp_path).await?;
        if actual != expected {
            return Err(LanError::ChecksumMismatch {
                expected: expected.to_string(),
                actual,
            });
        }
    }
    tokio::fs::rename(temp_path, final_path).await?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelParams {
    session_id: String,
}

async fn cancel(State(state): State<AppState>, Query(params): Query<CancelParams>) -> StatusCode {
    state
        .sessions
        .lock()
        .expect("sessions lock")
        .remove(&params.session_id);
    StatusCode::OK
}

/// 向 `base_url`（如 `https://host:port`）发送单个文件（prepare-upload → upload）。
/// https 时按 `expected_fingerprint` pin 对端证书。
pub async fn send_file(
    base_url: &str,
    sender: &DeviceInfo,
    path: &Path,
    expected_fingerprint: Option<String>,
    identity: Option<&crate::tls::Identity>,
) -> Result<()> {
    send_file_with_progress(base_url, sender, path, expected_fingerprint, None, identity).await
}

struct ProgressMonitor(tokio::task::JoinHandle<()>);
impl Drop for ProgressMonitor {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Sender progress comes from the receiver's committed file offset, never a simulated timer.
pub async fn send_file_with_progress(
    base_url: &str,
    sender: &DeviceInfo,
    path: &Path,
    expected_fingerprint: Option<String>,
    progress: Option<(TransferSink, String)>,
    identity: Option<&crate::tls::Identity>,
) -> Result<()> {
    let meta = tokio::fs::metadata(path).await?;
    let size = meta.len();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| LanError::InvalidPath(path.display().to_string()))?
        .to_string();
    let sha = crate::checksum::sha256_file(path).await?;
    let file_id = Uuid::new_v4().to_string();

    let dto = FileDto {
        id: file_id.clone(),
        file_name,
        size,
        file_type: "application/octet-stream".to_string(),
        sha256: Some(sha),
        preview: None,
        metadata: None,
    };
    let mut files = HashMap::new();
    files.insert(file_id.clone(), dto);
    let req = PrepareUploadRequest {
        info: sender.clone(),
        files,
    };

    let client = crate::tls::client(
        base_url.starts_with("https"),
        expected_fingerprint,
        identity,
    )?;
    let prepared: PrepareUploadResponse = client
        .post(format!("{base_url}{API_PREFIX}/prepare-upload"))
        .json(&req)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let token = prepared
        .files
        .get(&file_id)
        .ok_or_else(|| LanError::Protocol("接收端未返回该文件的上传 token".to_string()))?
        .clone();

    let session_id = prepared.session_id;
    let _monitor = progress.as_ref().map(|(sink, task)| {
        let sink = sink.clone();
        let task = task.clone();
        let client = client.clone();
        let base = base_url.to_owned();
        let sid = session_id.clone();
        let fid = file_id.clone();
        let token = token.clone();
        ProgressMonitor(tokio::spawn(async move {
            let mut ticks = tokio::time::interval(Duration::from_millis(250));
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut reporter = ProgressReporter::new(Some(sink), task, size);
            loop {
                ticks.tick().await;
                if let Ok(Ok(done)) = tokio::time::timeout(
                    Duration::from_millis(800),
                    upload_offset_query(&client, &base, &sid, &fid, &token),
                )
                .await
                {
                    reporter.report(done.min(size));
                }
            }
        }))
    });

    // 断点续传：每轮先查接收端已收字节，从该偏移续发；最多重试数次。
    const MAX_ATTEMPTS: u32 = 6;
    let mut attempt: u32 = 0;
    loop {
        let offset = upload_offset_query(&client, base_url, &session_id, &file_id, &token)
            .await
            .unwrap_or(0);
        if size > 0 && offset >= size {
            break;
        }
        let result = upload_range(
            &client,
            base_url,
            &session_id,
            &file_id,
            &token,
            offset,
            path,
        )
        .await;
        match result {
            Ok(received) if received >= size => break,
            _ => {
                attempt += 1;
                if attempt >= MAX_ATTEMPTS {
                    return match result {
                        Err(e) => Err(e),
                        Ok(_) => Err(LanError::Protocol("续传多次仍未完成".to_string())),
                    };
                }
                tokio::time::sleep(Duration::from_millis(150 * attempt as u64)).await;
            }
        }
    }
    if let Some((sink, task_id)) = progress {
        let _ = sink.send(TransferUpdate::Progress {
            task_id,
            bytes_done: size,
            bytes_total: size,
        });
    }
    Ok(())
}

/// 查询接收端当前已收到的字节数（续传偏移）。
async fn upload_offset_query(
    client: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    file_id: &str,
    token: &str,
) -> Result<u64> {
    let url = format!(
        "{base_url}{RESUME_OFFSET_PATH}?sessionId={session_id}&fileId={file_id}&token={token}"
    );
    let ack: UploadAck = client
        .post(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(ack.received)
}

/// 从 `offset` 起上传文件剩余部分，返回接收端应答的已收字节数。
async fn upload_range(
    client: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    file_id: &str,
    token: &str,
    offset: u64,
    path: &Path,
) -> Result<u64> {
    use tokio::io::AsyncSeekExt;
    let mut file = tokio::fs::File::open(path).await?;
    file.seek(std::io::SeekFrom::Start(offset)).await?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = reqwest::Body::wrap_stream(stream);
    let url = format!(
        "{base_url}{API_PREFIX}/upload?sessionId={session_id}&fileId={file_id}&token={token}&offset={offset}"
    );
    let ack: UploadAck = client
        .post(url)
        .body(body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(ack.received)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{DeviceType, TransportProtocol, PROTOCOL_VERSION};

    fn sender_info() -> DeviceInfo {
        DeviceInfo {
            alias: "tester".into(),
            version: PROTOCOL_VERSION.into(),
            device_model: None,
            device_type: Some(DeviceType::Desktop),
            fingerprint: "test-fp".into(),
            port: None,
            protocol: Some(TransportProtocol::Http),
            download: None,
        }
    }

    #[tokio::test]
    async fn sends_and_verifies_a_file_over_loopback() {
        let dir = tempfile::tempdir().unwrap();
        let save_dir = dir.path().join("recv");
        tokio::fs::create_dir_all(&save_dir).await.unwrap();

        let src = dir.path().join("source.bin");
        let payload = vec![7u8; 500 * 1024];
        tokio::fs::write(&src, &payload).await.unwrap();

        let receiver = Receiver::start(
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(AcceptAll {
                save_dir: save_dir.clone(),
            }),
            sender_info(),
            None,
            None,
        )
        .await
        .unwrap();
        let base = format!("http://{}", receiver.local_addr());

        send_file(&base, &sender_info(), &src, None, None).await.unwrap();

        let got = tokio::fs::read(save_dir.join("source.bin")).await.unwrap();
        assert_eq!(got, payload);
        // 临时文件已被重命名清理。
        assert!(!save_dir.join(format!("source.bin{PART_SUFFIX}")).exists());

        receiver.shutdown().await;
    }

    #[tokio::test]
    async fn sender_reports_intermediate_confirmed_offsets_and_stops_monitoring() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let bytes = Arc::new(AtomicU64::new(0));
        let app = Router::new()
            .route(
                &format!("{API_PREFIX}/prepare-upload"),
                post(|Json(req): Json<PrepareUploadRequest>| async move {
                    Json(PrepareUploadResponse {
                        session_id: "session".into(),
                        files: req
                            .files
                            .keys()
                            .map(|id| (id.clone(), "token".into()))
                            .collect(),
                    })
                }),
            )
            .route(
                RESUME_OFFSET_PATH,
                post(|State(bytes): State<Arc<AtomicU64>>| async move {
                    Json(serde_json::json!({ "received": bytes.load(Ordering::SeqCst) }))
                }),
            )
            .route(
                &format!("{API_PREFIX}/upload"),
                post(
                    |State(bytes): State<Arc<AtomicU64>>, body: Body| async move {
                        let mut stream = body.into_data_stream();
                        while let Some(chunk) = stream.next().await {
                            bytes.fetch_add(chunk.unwrap().len() as u64, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                        Json(serde_json::json!({ "received": bytes.load(Ordering::SeqCst) }))
                    },
                ),
            )
            .with_state(bytes);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("progress.bin");
        let size = 1024 * 1024;
        tokio::fs::write(&file, vec![8u8; size]).await.unwrap();
        let (sink, mut updates) = tokio::sync::mpsc::unbounded_channel();
        send_file_with_progress(
            &format!("http://{address}"),
            &sender_info(),
            &file,
            None,
            Some((sink, "observed".into())),
            None,
        )
        .await
        .unwrap();
        let mut samples = Vec::new();
        while let Ok(TransferUpdate::Progress {
            task_id,
            bytes_done,
            bytes_total,
        }) = updates.try_recv()
        {
            assert_eq!(task_id, "observed");
            assert_eq!(bytes_total, size as u64);
            samples.push(bytes_done);
        }
        assert!(
            samples.iter().any(|&n| n > 0 && n < size as u64),
            "missing intermediate confirmed progress: {samples:?}"
        );
        assert_eq!(samples.last(), Some(&(size as u64)));
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(updates.try_recv().is_err());
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn zero_byte_file_is_actually_created_at_receiver() {
        let dir = tempfile::tempdir().unwrap();
        let save = dir.path().join("save");
        tokio::fs::create_dir_all(&save).await.unwrap();
        let file = dir.path().join("empty.txt");
        tokio::fs::write(&file, b"").await.unwrap();
        let receiver = Receiver::start(
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(AcceptAll {
                save_dir: save.clone(),
            }),
            sender_info(),
            None,
            None,
        )
        .await
        .unwrap();
        send_file(
            &format!("http://{}", receiver.local_addr()),
            &sender_info(),
            &file,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            tokio::fs::metadata(save.join("empty.txt"))
                .await
                .unwrap()
                .len(),
            0
        );
        receiver.shutdown().await;
    }

    #[tokio::test]
    async fn reject_policy_refuses_transfer() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("x.bin");
        tokio::fs::write(&src, b"hello").await.unwrap();

        let receiver = Receiver::start(
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(RejectAll),
            sender_info(),
            None,
            None,
        )
        .await
        .unwrap();
        let base = format!("http://{}", receiver.local_addr());

        assert!(send_file(&base, &sender_info(), &src, None, None).await.is_err());
        receiver.shutdown().await;
    }

    #[tokio::test]
    async fn resumable_upload_appends_from_offset() {
        let dir = tempfile::tempdir().unwrap();
        let save_dir = dir.path().join("recv");
        tokio::fs::create_dir_all(&save_dir).await.unwrap();
        let receiver = Receiver::start(
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(AcceptAll {
                save_dir: save_dir.clone(),
            }),
            sender_info(),
            None,
            None,
        )
        .await
        .unwrap();
        let base = format!("http://{}", receiver.local_addr());
        let client = reqwest::Client::new();

        let payload: Vec<u8> = (0..300_000u32).map(|i| i as u8).collect();
        let sha = {
            use sha2::{Digest, Sha256};
            crate::checksum::hex_lower(&Sha256::digest(&payload))
        };
        let file_id = "f1".to_string();
        let dto = FileDto {
            id: file_id.clone(),
            file_name: "resumed.bin".to_string(),
            size: payload.len() as u64,
            file_type: "application/octet-stream".to_string(),
            sha256: Some(sha),
            preview: None,
            metadata: None,
        };
        let mut files = HashMap::new();
        files.insert(file_id.clone(), dto);
        let req = PrepareUploadRequest {
            info: sender_info(),
            files,
        };
        let prepared: PrepareUploadResponse = client
            .post(format!("{base}{API_PREFIX}/prepare-upload"))
            .json(&req)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let token = prepared.files.get(&file_id).unwrap().clone();
        let sid = prepared.session_id;
        let half = payload.len() / 2;

        // 第一段（模拟中断）：收到一半，保留 part，尚未落盘。
        let ack1: UploadAck = client
            .post(format!(
                "{base}{API_PREFIX}/upload?sessionId={sid}&fileId={file_id}&token={token}&offset=0"
            ))
            .body(payload[..half].to_vec())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(ack1.received, half as u64);
        assert!(!save_dir.join("resumed.bin").exists());

        // 偏移查询应返回已收字节。
        let probe: UploadAck = client
            .post(format!(
                "{base}{RESUME_OFFSET_PATH}?sessionId={sid}&fileId={file_id}&token={token}"
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(probe.received, half as u64);

        // 第二段：从偏移续发剩余，凑满后校验落盘。
        let ack2: UploadAck = client
            .post(format!(
                "{base}{API_PREFIX}/upload?sessionId={sid}&fileId={file_id}&token={token}&offset={half}"
            ))
            .body(payload[half..].to_vec())
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(ack2.received, payload.len() as u64);

        let got = tokio::fs::read(save_dir.join("resumed.bin")).await.unwrap();
        assert_eq!(got, payload);
        receiver.shutdown().await;
    }
}
