//! lan-service：ClashNova 局域网文件传输核心。
//!
//! 不依赖 Tauri，便于桌面端（src-tauri 适配层）与移动端（FFI）复用。
//! 完整设计见 `docs/desktop/局域网文件传输设计.md`。
//!
//! 已实现：LocalSend 兼容的 HTTP 收发（transport）、流式 SHA-256、临时文件原子
//! 重命名、文件名防穿越、设备注册表与手动添加、UDP 组播发现（discovery）、
//! 交互式接收审批。未实现：TLS、断点续传、进度事件节流、暂停/取消在途任务。

pub mod checksum;
pub mod discovery;
pub mod error;
pub mod messages;
pub mod protocol;
pub mod security;
pub mod storage;
pub mod tls;
pub mod transfer;
pub mod transport;

pub use error::{LanError, Result};

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use protocol::{DeviceInfo, DeviceType, FileDto, TransportProtocol};
use transfer::{Direction, Progress, TransferSink, TransferState, TransferTask, TransferUpdate};

/// 本机设备信息（对外展示与组播广播用）。
#[derive(Debug, Clone)]
pub struct LocalDevice {
    pub alias: String,
    pub fingerprint: String,
    pub device_type: DeviceType,
    pub port: u16,
    pub protocol: TransportProtocol,
}

/// 发现到的对端设备。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredDevice {
    pub alias: String,
    pub fingerprint: String,
    pub address: IpAddr,
    pub port: u16,
    pub device_type: Option<DeviceType>,
    /// 是否为已配对的可信设备。
    pub trusted: bool,
}

/// 运行期配置。接收服务按此启停，生命周期独立于代理连接（设计文档第 6 节）。
#[derive(Debug, Clone)]
pub struct LanConfig {
    pub alias: String,
    /// 应用数据目录（放置 SQLite、证书等）。
    pub data_dir: PathBuf,
    pub listen_port: u16,
    /// 是否对外可发现（广播 announce 并响应 register）。
    pub discoverable: bool,
    /// 是否接收入站传输。
    pub accept_incoming: bool,
    /// 对可信设备是否自动接收（默认关，见 LAN-007）。
    pub auto_accept_trusted: bool,
    pub default_save_dir: PathBuf,
    /// 最大并发任务数，默认 2。
    pub max_concurrent: usize,
    /// 是否启用 TLS（自签证书 + 指纹 pin）。默认开；测试可关走明文 HTTP。
    pub use_tls: bool,
}

impl LanConfig {
    pub fn new(alias: impl Into<String>, data_dir: PathBuf, default_save_dir: PathBuf) -> Self {
        Self {
            alias: alias.into(),
            data_dir,
            listen_port: protocol::DEFAULT_PORT,
            discoverable: true,
            accept_incoming: true,
            auto_accept_trusted: false,
            default_save_dir,
            max_concurrent: 2,
            use_tls: true,
        }
    }
}

/// 向上层（src-tauri → 前端）推送的事件。对应 `lan://*` Tauri 事件。
#[derive(Debug, Clone)]
pub enum LanEvent {
    /// 设备列表变化。
    DevicesChanged(Vec<DiscoveredDevice>),
    /// 收到待确认的接收请求。
    IncomingRequest {
        task_id: String,
        peer_alias: String,
        /// (文件名, 大小) 列表。
        files: Vec<(String, u64)>,
        total_size: u64,
        text: Option<String>,
    },
    /// 任务状态变更。
    TransferChanged(TransferTask),
    /// 节流后的进度更新。
    Progress(Progress),
    TextReceived(storage::HistoryRow),
    /// 收到对端的配对请求，待本机用户确认（LAN-007）。
    PairRequest {
        request_id: String,
        peer_alias: String,
        peer_fingerprint: String,
    },
}

/// 接收请求的处置。
#[derive(Debug, Clone)]
pub enum IncomingDecision {
    Accept {
        /// 覆盖默认保存目录；`None` 用配置里的 `default_save_dir`。
        save_dir: Option<PathBuf>,
    },
    Reject,
}

/// 交互式接收审批策略：收到 prepare-upload 时推送 [`LanEvent::IncomingRequest`]，
/// 阻塞等待上层经 [`LanService::respond_incoming`] 作出决定（带超时）。
struct InteractiveApprover {
    events_tx: broadcast::Sender<LanEvent>,
    pending: Mutex<HashMap<String, oneshot::Sender<transport::AcceptDecision>>>,
    /// 待确认的配对请求：request_id → 决定回传通道。
    pending_pairs: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    /// 是否接收入站传输（关则一律拒绝）。可运行时切换、持久化。
    accept_incoming: std::sync::atomic::AtomicBool,
    /// 已配对设备是否自动接收（关则仍逐次弹窗确认）。可运行时切换、持久化。
    auto_accept_trusted: std::sync::atomic::AtomicBool,
    /// 自动接收（已配对设备）时使用的默认保存目录。
    default_save_dir: Mutex<PathBuf>,
    storage: Arc<storage::Storage>,
}

// Also removes the approval slot if the HTTP handler is canceled before its timeout.
struct ApprovalGuard<'a> {
    pending: &'a Mutex<HashMap<String, oneshot::Sender<transport::AcceptDecision>>>,
    id: String,
}
impl Drop for ApprovalGuard<'_> {
    fn drop(&mut self) {
        self.pending.lock().expect("pending lock").remove(&self.id);
    }
}

// 配对请求被取消/超时时，移除其待确认槽位。
struct PairGuard<'a> {
    pending: &'a Mutex<HashMap<String, oneshot::Sender<bool>>>,
    id: String,
}
impl Drop for PairGuard<'_> {
    fn drop(&mut self) {
        self.pending
            .lock()
            .expect("pending pairs lock")
            .remove(&self.id);
    }
}

#[async_trait]
impl transport::AcceptPolicy for InteractiveApprover {
    async fn decide(
        &self,
        session_id: &str,
        sender: &DeviceInfo,
        files: &HashMap<String, FileDto>,
    ) -> transport::AcceptDecision {
        use std::sync::atomic::Ordering;
        if !self.accept_incoming.load(Ordering::SeqCst) {
            return transport::AcceptDecision::Reject;
        }
        // 已配对 + 自动接收开启：直接收，不弹窗。关闭自动接收则仍逐次确认。
        if self.auto_accept_trusted.load(Ordering::SeqCst)
            && !sender.fingerprint.is_empty()
            && self
                .storage
                .is_trusted(&sender.fingerprint)
                .unwrap_or(false)
        {
            return transport::AcceptDecision::Accept {
                save_dir: self
                    .default_save_dir
                    .lock()
                    .expect("receive directory lock")
                    .clone(),
            };
        }
        // 未配对（如三方 LocalSend）：回退到交互式逐次确认，接受后记为可信。
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .expect("pending lock")
            .insert(session_id.to_string(), tx);

        let _approval = ApprovalGuard {
            pending: &self.pending,
            id: session_id.into(),
        };
        let total: u64 = files.values().map(|f| f.size).sum();
        let file_list: Vec<(String, u64)> = files
            .values()
            .map(|f| (f.file_name.clone(), f.size))
            .collect();
        let _ = self.events_tx.send(LanEvent::IncomingRequest {
            task_id: session_id.to_string(),
            peer_alias: sender.alias.clone(),
            files: file_list,
            total_size: total,
            text: files
                .values()
                .next()
                .filter(|f| files.len() == 1 && f.file_type == messages::TEXT_TYPE)
                .and_then(|f| f.preview.clone()),
        });

        // 等待用户决定，超时则视为拒绝并清理。
        match tokio::time::timeout(Duration::from_secs(120), rx).await {
            Ok(Ok(decision)) => {
                // 用户接受即记住该设备为可信（TOFU 配对）。
                if matches!(decision, transport::AcceptDecision::Accept { .. })
                    && !sender.fingerprint.is_empty()
                {
                    let _ = self
                        .storage
                        .upsert_trusted(&sender.fingerprint, &sender.alias, None);
                }
                decision
            }
            _ => {
                self.pending
                    .lock()
                    .expect("pending lock")
                    .remove(session_id);
                transport::AcceptDecision::Reject
            }
        }
    }

    /// 配对请求审批：推送 [`LanEvent::PairRequest`]，阻塞等待用户确认（带超时）。
    /// 接受则把对端记为可信（配对即信任，自动接收）。
    async fn decide_pair(&self, sender: &DeviceInfo) -> bool {
        if sender.fingerprint.is_empty() {
            return false;
        }
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending_pairs
            .lock()
            .expect("pending pairs lock")
            .insert(request_id.clone(), tx);
        let _guard = PairGuard {
            pending: &self.pending_pairs,
            id: request_id.clone(),
        };
        let _ = self.events_tx.send(LanEvent::PairRequest {
            request_id: request_id.clone(),
            peer_alias: sender.alias.clone(),
            peer_fingerprint: sender.fingerprint.clone(),
        });
        match tokio::time::timeout(Duration::from_secs(120), rx).await {
            Ok(Ok(true)) => {
                let _ = self
                    .storage
                    .upsert_trusted(&sender.fingerprint, &sender.alias, None);
                true
            }
            _ => false,
        }
    }
}

/// 局域网传输服务门面。
///
/// 持有存储、事件通道、设备注册表与接收审批，统筹发现与传输子系统。
/// 由上层按"可发现性/接收开关"启停，生命周期与代理连接解耦（设计文档第 2、6 节）。
pub struct LanService {
    config: LanConfig,
    /// 本机设备指纹 = 自签证书的 SHA-256（LocalSend https 模式）。
    fingerprint: String,
    identity: tls::Identity,
    events_tx: broadcast::Sender<LanEvent>,
    storage: Arc<storage::Storage>,
    approver: Arc<InteractiveApprover>,
    devices: Arc<Mutex<HashMap<String, DiscoveredDevice>>>,
    tasks: Arc<Mutex<HashMap<String, TransferTask>>>,
    transfer_tx: Mutex<Option<TransferSink>>,
    cancels: transport::CancelRegistry,
    /// 正在暂停的发送任务 id（send_one 据此把取消识别为暂停）。
    pausing: Mutex<HashSet<String>>,
    /// 已暂停的发送任务 → (对端, 文件路径)，供恢复时续发。
    paused: Mutex<HashMap<String, (DiscoveredDevice, PathBuf)>>,
    receiver: tokio::sync::Mutex<Option<transport::Receiver>>,
    beacon: tokio::sync::Mutex<Option<discovery::Beacon>>,
    /// 可运行时修改的本机别名（设备名）；持久化于 lan_settings。
    alias: Mutex<String>,
    /// 是否对外可发现（广播信标）；可运行时切换，持久化于 lan_settings。
    discoverable: std::sync::atomic::AtomicBool,
    /// 发现回填通道（start 时存入），供运行时重启信标复用。
    disc_tx: Mutex<Option<transport::DiscoverySink>>,
}

impl LanService {
    /// 创建服务：打开本地数据库、建立事件通道与接收审批。尚未开始监听（见 [`start`](Self::start)）。
    pub fn new(mut config: LanConfig) -> Result<Self> {
        let db_path = config.data_dir.join("lan-transfer.sqlite3");
        let storage = Arc::new(storage::Storage::open(&db_path)?);
        if let Some(saved) = storage.receive_directory()? {
            if !std::path::Path::new(&saved).is_absolute() || saved.contains('\0') {
                return Err(LanError::Config("Invalid saved receive directory".into()));
            }
            config.default_save_dir = PathBuf::from(saved);
        }
        // 读回持久化的设备名与可发现开关（设置页改过则覆盖默认机器名/默认开关）。
        if let Some(saved) = storage.setting("alias")? {
            let saved = saved.trim();
            if !saved.is_empty() {
                config.alias = saved.to_string();
            }
        }
        if let Some(saved) = storage.setting("discoverable")? {
            config.discoverable = saved != "0";
        }
        if let Some(saved) = storage.setting("accept_incoming")? {
            config.accept_incoming = saved != "0";
        }
        // 自动接收默认关闭：即便对端已配对/可信，首次仍逐次弹窗确认（纵深防御；
        // mTLS 已把信任绑定到已验证证书，这里再降一档默认值）。显式开启持久化为 "1"。
        config.auto_accept_trusted = match storage.setting("auto_accept")? {
            Some(v) => v != "0",
            None => false,
        };
        let identity = tls::Identity::load_or_create(&config.data_dir)?;
        let (events_tx, _rx) = broadcast::channel(256);
        let approver = Arc::new(InteractiveApprover {
            events_tx: events_tx.clone(),
            pending: Mutex::new(HashMap::new()),
            pending_pairs: Mutex::new(HashMap::new()),
            accept_incoming: std::sync::atomic::AtomicBool::new(config.accept_incoming),
            auto_accept_trusted: std::sync::atomic::AtomicBool::new(config.auto_accept_trusted),
            default_save_dir: Mutex::new(config.default_save_dir.clone()),
            storage: storage.clone(),
        });
        let (alias0, discoverable0) = (config.alias.clone(), config.discoverable);
        Ok(Self {
            config,
            fingerprint: identity.fingerprint.clone(),
            identity,
            events_tx,
            storage,
            approver,
            devices: Arc::new(Mutex::new(HashMap::new())),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            transfer_tx: Mutex::new(None),
            cancels: transport::cancel_registry(),
            pausing: Mutex::new(HashSet::new()),
            paused: Mutex::new(HashMap::new()),
            receiver: tokio::sync::Mutex::new(None),
            beacon: tokio::sync::Mutex::new(None),
            alias: Mutex::new(alias0),
            discoverable: std::sync::atomic::AtomicBool::new(discoverable0),
            disc_tx: Mutex::new(None),
        })
    }

    /// 订阅事件流；上层据此向前端 emit `lan://*`。
    pub fn subscribe(&self) -> broadcast::Receiver<LanEvent> {
        self.events_tx.subscribe()
    }

    pub fn config(&self) -> &LanConfig {
        &self.config
    }

    pub fn storage(&self) -> &Arc<storage::Storage> {
        &self.storage
    }

    /// 本机设备指纹（自签证书 SHA-256）。
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn save_directory(&self) -> PathBuf {
        self.approver
            .default_save_dir
            .lock()
            .expect("receive directory lock")
            .clone()
    }

    /// Persist before exposing the new directory; already accepted transfers keep their session path.
    pub fn set_save_directory(&self, directory: &std::path::Path) -> Result<PathBuf> {
        if !directory.is_absolute() {
            return Err(LanError::Config(
                "Receive directory must be absolute".into(),
            ));
        }
        let directory = directory.canonicalize()?;
        if !directory.is_dir() {
            return Err(LanError::Config("Receive directory is not a folder".into()));
        }
        let probe = directory.join(format!(".clashnova-write-check-{}", Uuid::new_v4()));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)?;
        drop(file);
        std::fs::remove_file(probe)?;
        let mut current = self
            .approver
            .default_save_dir
            .lock()
            .expect("receive directory lock");
        self.storage
            .save_receive_directory(&directory.to_string_lossy())?;
        *current = directory.clone();
        Ok(directory)
    }

    /// 当前本机别名（设备名）。
    pub fn alias(&self) -> String {
        self.alias.lock().expect("alias lock").clone()
    }

    /// 是否对外可发现。
    pub fn discoverable(&self) -> bool {
        self.discoverable.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 修改设备名：校验、持久化、立即对外重播（重启信标，对端 5s 内刷新到新名）。
    pub async fn set_alias(&self, alias: &str) -> Result<()> {
        let alias = alias.trim();
        if alias.is_empty() || alias.chars().count() > 64 {
            return Err(LanError::Config("设备名需为 1-64 个字符".into()));
        }
        self.storage.save_setting("alias", alias)?;
        *self.alias.lock().expect("alias lock") = alias.to_string();
        self.refresh_beacon().await;
        Ok(())
    }

    /// 切换是否可被发现：持久化并即时启停信标。
    pub async fn set_discoverable(&self, on: bool) -> Result<()> {
        self.storage
            .save_setting("discoverable", if on { "1" } else { "0" })?;
        self.discoverable
            .store(on, std::sync::atomic::Ordering::SeqCst);
        self.refresh_beacon().await;
        Ok(())
    }

    /// 是否接收入站传输。
    pub fn accept_incoming(&self) -> bool {
        self.approver
            .accept_incoming
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 已配对设备是否自动接收。
    pub fn auto_accept_trusted(&self) -> bool {
        self.approver
            .auto_accept_trusted
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 切换是否接收入站：持久化并即时作用到审批器。
    pub fn set_accept_incoming(&self, on: bool) -> Result<()> {
        self.storage
            .save_setting("accept_incoming", if on { "1" } else { "0" })?;
        self.approver
            .accept_incoming
            .store(on, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// 切换已配对设备是否自动接收：持久化并即时作用到审批器。
    pub fn set_auto_accept(&self, on: bool) -> Result<()> {
        self.storage
            .save_setting("auto_accept", if on { "1" } else { "0" })?;
        self.approver
            .auto_accept_trusted
            .store(on, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// 按当前别名/可发现状态重启组播信标：停旧的；若可发现且接收端在运行则起新的。
    async fn refresh_beacon(&self) {
        if let Some(beacon) = self.beacon.lock().await.take() {
            beacon.shutdown().await;
        }
        if self.receiver.lock().await.is_none() || !self.discoverable() {
            return;
        }
        let disc_tx = self.disc_tx.lock().expect("disc_tx lock").clone();
        let Some(disc_tx) = disc_tx else { return };
        let device = self.local_device_info();
        match discovery::Beacon::start(device, disc_tx).await {
            Ok(beacon) => *self.beacon.lock().await = Some(beacon),
            Err(err) => log::warn!("信标重启失败: {err}"),
        }
    }

    fn local_device_info(&self) -> DeviceInfo {
        DeviceInfo {
            alias: self.alias.lock().expect("alias lock").clone(),
            version: protocol::PROTOCOL_VERSION.to_string(),
            device_model: None,
            device_type: Some(DeviceType::Desktop),
            fingerprint: self.fingerprint.clone(),
            port: Some(self.config.listen_port),
            protocol: Some(if self.config.use_tls {
                TransportProtocol::Https
            } else {
                TransportProtocol::Http
            }),
            download: Some(false),
        }
    }

    /// 当前已知设备快照。
    pub fn devices(&self) -> Vec<DiscoveredDevice> {
        self.devices
            .lock()
            .expect("devices lock")
            .values()
            .cloned()
            .collect()
    }

    /// 接收端已绑定的本地地址（未启动则为 None）。
    pub async fn local_addr(&self) -> Option<SocketAddr> {
        self.receiver.lock().await.as_ref().map(|r| r.local_addr())
    }

    /// 当前进行中的任务快照（非终态）。
    pub fn transfers(&self) -> Vec<TransferTask> {
        self.tasks
            .lock()
            .expect("tasks lock")
            .values()
            .filter(|t| !t.state.is_terminal())
            .cloned()
            .collect()
    }

    /// 历史记录（最近在前）。
    pub fn history(&self) -> Vec<storage::HistoryRow> {
        self.storage.list_history(200).unwrap_or_default()
    }

    /// 清空历史记录。
    pub fn clear_history(&self) -> Result<()> {
        self.storage.clear_history()
    }

    /// 删除一条历史记录（按 id，文件或文本）。
    pub fn remove_history(&self, id: &str) -> Result<()> {
        self.storage.remove_history(id).map(|_| ())
    }

    /// 可信设备列表（TOFU 配对）。
    pub fn trusted_devices(&self) -> Vec<storage::TrustedRow> {
        self.storage.list_trusted().unwrap_or_default()
    }

    /// 解除某设备配对（从可信列表移除）。
    pub fn unpair(&self, fingerprint: &str) -> Result<()> {
        self.storage.remove_trusted(fingerprint)
    }

    /// 启动接收端（HTTP，明文）与组播发现。幂等：已启动则直接返回。
    pub async fn start(&self) -> Result<()> {
        let mut guard = self.receiver.lock().await;
        if guard.is_some() {
            return Ok(());
        }

        // 发现管道：接收端/组播把发现到的对端推进来，由此任务落入注册表并广播。
        let (disc_tx, mut disc_rx) = mpsc::unbounded_channel::<transport::Discovered>();
        *self.disc_tx.lock().expect("disc_tx lock") = Some(disc_tx.clone());
        {
            let devices = self.devices.clone();
            let events = self.events_tx.clone();
            let storage = self.storage.clone();
            tokio::spawn(async move {
                while let Some(found) = disc_rx.recv().await {
                    if found.info.fingerprint.is_empty() {
                        continue;
                    }
                    let dd = DiscoveredDevice {
                        alias: found.info.alias.clone(),
                        fingerprint: found.info.fingerprint.clone(),
                        address: found.addr.ip(),
                        port: found.addr.port(),
                        device_type: found.info.device_type,
                        trusted: storage.is_trusted(&found.info.fingerprint).unwrap_or(false),
                    };
                    let list = {
                        let mut reg = devices.lock().expect("devices lock");
                        reg.insert(found.info.fingerprint.clone(), dd);
                        reg.values().cloned().collect::<Vec<_>>()
                    };
                    let _ = events.send(LanEvent::DevicesChanged(list));
                }
            });
        }

        // 传输更新管道：接收端上报进度/状态，由此任务维护任务表、广播事件、落历史。
        let (transfer_tx, mut transfer_rx) = mpsc::unbounded_channel::<TransferUpdate>();
        *self.transfer_tx.lock().expect("transfer tx lock") = Some(transfer_tx.clone());
        {
            let tasks = self.tasks.clone();
            let events = self.events_tx.clone();
            let storage = self.storage.clone();
            tokio::spawn(async move {
                while let Some(update) = transfer_rx.recv().await {
                    drain_transfer_update(&tasks, &events, &storage, update);
                }
            });
        }

        let device = self.local_device_info();
        let bind = SocketAddr::from(([0, 0, 0, 0], self.config.listen_port));
        let tls = if self.config.use_tls {
            Some(tls::server_config(&self.identity).await?)
        } else {
            None
        };
        let receiver = transport::Receiver::start_with_messages(
            bind,
            self.approver.clone(),
            device.clone(),
            Some(disc_tx.clone()),
            Some(transfer_tx),
            Some(Arc::new(messages::TextInbox::new(
                self.storage.clone(),
                self.approver.clone(),
                self.events_tx.clone(),
            ))),
            tls,
            self.cancels.clone(),
        )
        .await?;
        *guard = Some(receiver);

        // 组播发现（best-effort）：失败不影响接收端运行。
        if self.discoverable() {
            match discovery::Beacon::start(device, disc_tx).await {
                Ok(beacon) => {
                    *self.beacon.lock().await = Some(beacon);
                }
                Err(err) => {
                    log::warn!("组播发现启动失败，仅手动/HTTP 发现可用: {err}");
                }
            }
        }
        Ok(())
    }

    /// 停止接收端与组播发现，释放端口与后台任务。
    pub async fn stop(&self) -> Result<()> {
        if let Some(beacon) = self.beacon.lock().await.take() {
            beacon.shutdown().await;
        }
        if let Some(receiver) = self.receiver.lock().await.take() {
            receiver.shutdown().await;
        }
        Ok(())
    }

    /// 触发一次主动发现：重新广播 announce，并重发当前设备列表。
    pub async fn refresh_devices(&self) -> Result<()> {
        if let Some(beacon) = self.beacon.lock().await.as_ref() {
            let _ = beacon.announce().await;
        }
        let list = self.devices();
        let _ = self.events_tx.send(LanEvent::DevicesChanged(list));
        Ok(())
    }

    /// 手动按地址添加设备（LAN-001 手动连接）：拉取对端 /info 并登记。
    pub async fn add_manual_device(&self, addr: SocketAddr) -> Result<DiscoveredDevice> {
        let addr = connect_addr(addr);
        let scheme = if self.config.use_tls { "https" } else { "http" };
        let url = format!("{scheme}://{addr}{}/info", protocol::API_PREFIX);
        // 首次探测：https 下接受任意自签证书以读取对端指纹（TOFU）。
        let info: DeviceInfo = tls::client(self.config.use_tls, None, Some(&self.identity))?
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let dd = DiscoveredDevice {
            alias: info.alias.clone(),
            fingerprint: if info.fingerprint.is_empty() {
                addr.to_string()
            } else {
                info.fingerprint.clone()
            },
            address: addr.ip(),
            port: info.port.filter(|port| *port > 0).unwrap_or(addr.port()),
            device_type: info.device_type,
            trusted: self.storage.is_trusted(&info.fingerprint).unwrap_or(false),
        };
        let list = {
            let mut reg = self.devices.lock().expect("devices lock");
            reg.insert(dd.fingerprint.clone(), dd.clone());
            reg.values().cloned().collect::<Vec<_>>()
        };
        let _ = self.events_tx.send(LanEvent::DevicesChanged(list));
        Ok(dd)
    }

    /// 向指定指纹的对端发送文件，返回各任务 id。地址由注册表解析，过程上报任务状态。
    pub async fn send_files(&self, fingerprint: &str, paths: Vec<PathBuf>) -> Result<Vec<String>> {
        let device = {
            let reg = self.devices.lock().expect("devices lock");
            reg.get(fingerprint).cloned()
        };
        let Some(device) = device else {
            return Err(LanError::Untrusted(fingerprint.to_string()));
        };
        let mut ids = Vec::new();
        for path in paths {
            let task_id = Uuid::new_v4().to_string();
            self.send_one(task_id.clone(), &device, &path).await?;
            ids.push(task_id);
        }
        Ok(ids)
    }

    /// 发送单个文件任务：可被取消/暂停。暂停时保留 (对端, 路径) 供恢复续发。
    async fn send_one(
        &self,
        task_id: String,
        device: &DiscoveredDevice,
        path: &std::path::Path,
    ) -> Result<()> {
        let addr = connect_addr(SocketAddr::new(device.address, device.port));
        let scheme = if self.config.use_tls { "https" } else { "http" };
        let base = format!("{scheme}://{addr}");
        let expected = if self.config.use_tls {
            Some(device.fingerprint.clone())
        } else {
            None
        };
        let local = self.local_device_info();
        let sink = self.transfer_tx.lock().expect("transfer tx lock").clone();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let size = tokio::fs::metadata(path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let local_path = tokio::fs::canonicalize(path)
            .await?
            .to_string_lossy()
            .into_owned();
        let make = |state| TransferTask {
            id: task_id.clone(),
            direction: Direction::Send,
            peer_alias: device.alias.clone(),
            peer_fingerprint: device.fingerprint.clone(),
            file_name: file_name.clone(),
            size,
            state,
            save_path: Some(local_path.clone()),
        };

        let token = tokio_util::sync::CancellationToken::new();
        self.cancels
            .lock()
            .expect("cancels lock")
            .insert(task_id.clone(), token.clone());
        if let Some(sink) = &sink {
            let _ = sink.send(TransferUpdate::Started(make(TransferState::Transferring)));
        }
        let outcome = tokio::select! {
            _ = token.cancelled() => Err(LanError::Canceled),
            r = transport::send_file_with_progress(&base, &local, path, expected, sink.clone().map(|sink| (sink, task_id.clone())), Some(&self.identity)) => r,
        };
        self.cancels.lock().expect("cancels lock").remove(&task_id);

        // 取消源于暂停则记作 Paused 并保留续传信息。
        let paused = matches!(outcome, Err(LanError::Canceled))
            && self.pausing.lock().expect("pausing lock").remove(&task_id);
        let state = match &outcome {
            Ok(()) => TransferState::Completed,
            Err(LanError::Canceled) if paused => TransferState::Paused,
            Err(LanError::Canceled) => TransferState::Canceled,
            Err(_) => TransferState::Failed,
        };
        if let Some(sink) = &sink {
            let _ = sink.send(TransferUpdate::Finished(make(state)));
        }
        if paused {
            self.paused
                .lock()
                .expect("paused lock")
                .insert(task_id, (device.clone(), path.to_path_buf()));
            return Ok(());
        }
        outcome
    }

    pub async fn send_text(&self, fingerprint: &str, text: String) -> Result<storage::HistoryRow> {
        messages::validate_text(&text)?;
        let device = self
            .devices
            .lock()
            .expect("devices lock")
            .get(fingerprint)
            .cloned()
            .ok_or_else(|| LanError::Untrusted(fingerprint.into()))?;
        let id = Uuid::new_v4().to_string();
        let request = messages::TextRequest {
            id: id.clone(),
            info: self.local_device_info(),
            text,
        };
        let scheme = if self.config.use_tls { "https" } else { "http" };
        let addr = connect_addr(SocketAddr::new(device.address, device.port));
        let base = format!("{scheme}://{addr}");
        let expected = if self.config.use_tls {
            Some(device.fingerprint.clone())
        } else {
            None
        };
        messages::send_text(&base, &request, expected, Some(&self.identity)).await?;
        let mut row = messages::history_row(&id, "send", &device.alias, &request.text);
        row.peer_fingerprint = fingerprint.to_string();
        self.storage.insert_text(&row, fingerprint)?;
        Ok(row)
    }

    /// 处置一个待确认的接收请求（由前端在 IncomingRequest 后调用）。
    pub async fn respond_incoming(&self, task_id: &str, decision: IncomingDecision) -> Result<()> {
        let tx = self
            .approver
            .pending
            .lock()
            .expect("pending lock")
            .remove(task_id);
        let Some(tx) = tx else {
            return Err(LanError::Protocol("无此接收请求或已超时".to_string()));
        };
        let decision = match decision {
            IncomingDecision::Accept { save_dir } => transport::AcceptDecision::Accept {
                save_dir: save_dir.unwrap_or_else(|| self.save_directory()),
            },
            IncomingDecision::Reject => transport::AcceptDecision::Reject,
        };
        let _ = tx.send(decision);
        Ok(())
    }

    /// 主动向某设备发起配对（LAN-007）：POST /pair 带本机信息，对端用户接受则互记可信。
    pub async fn pair(&self, fingerprint: &str) -> Result<()> {
        let device = {
            let reg = self.devices.lock().expect("devices lock");
            reg.get(fingerprint).cloned()
        };
        let Some(device) = device else {
            return Err(LanError::Untrusted(fingerprint.to_string()));
        };
        let addr = connect_addr(SocketAddr::new(device.address, device.port));
        let scheme = if self.config.use_tls { "https" } else { "http" };
        let url = format!("{scheme}://{addr}{}", transport::PAIR_PATH);
        let expected = if self.config.use_tls {
            Some(device.fingerprint.clone())
        } else {
            None
        };
        let resp = tls::client(self.config.use_tls, expected, Some(&self.identity))?
            .post(url)
            .json(&self.local_device_info())
            .send()
            .await?;
        match resp.status() {
            reqwest::StatusCode::OK => {
                self.storage
                    .upsert_trusted(&device.fingerprint, &device.alias, None)?;
                let list = {
                    let mut reg = self.devices.lock().expect("devices lock");
                    if let Some(d) = reg.get_mut(fingerprint) {
                        d.trusted = true;
                    }
                    reg.values().cloned().collect::<Vec<_>>()
                };
                let _ = self.events_tx.send(LanEvent::DevicesChanged(list));
                Ok(())
            }
            reqwest::StatusCode::FORBIDDEN => Err(LanError::Rejected),
            _ => {
                resp.error_for_status()?;
                Err(LanError::Protocol("配对应答异常".into()))
            }
        }
    }

    /// 响应一个收到的配对请求（前端在 PairRequest 后调用）。接受即把对端记为可信。
    pub async fn respond_pair(&self, request_id: &str, accept: bool) -> Result<()> {
        let tx = self
            .approver
            .pending_pairs
            .lock()
            .expect("pending pairs lock")
            .remove(request_id);
        let Some(tx) = tx else {
            return Err(LanError::Protocol("无此配对请求或已超时".into()));
        };
        let _ = tx.send(accept);
        Ok(())
    }

    /// 与某设备（指纹）的会话时间线（文本+文件合并，按时间正序）。
    pub fn conversation(&self, fingerprint: &str) -> Vec<storage::HistoryRow> {
        self.storage
            .list_conversation(fingerprint, 500)
            .unwrap_or_default()
    }

    /// 暂停进行中的发送（保留接收端 .part，之后可 [`resume`](Self::resume) 续传）。
    pub async fn pause(&self, task_id: &str) -> Result<()> {
        self.pausing
            .lock()
            .expect("pausing lock")
            .insert(task_id.to_string());
        let token = self
            .cancels
            .lock()
            .expect("cancels lock")
            .get(task_id)
            .cloned();
        match token {
            Some(token) => {
                token.cancel();
                Ok(())
            }
            None => {
                self.pausing.lock().expect("pausing lock").remove(task_id);
                Err(LanError::Protocol("无此在途任务".to_string()))
            }
        }
    }

    /// 恢复已暂停的发送：从接收端已收偏移续发（断点续传）。
    pub async fn resume(&self, task_id: &str) -> Result<()> {
        let entry = self.paused.lock().expect("paused lock").remove(task_id);
        let Some((device, path)) = entry else {
            return Err(LanError::Protocol("无此暂停任务".to_string()));
        };
        self.send_one(task_id.to_string(), &device, &path).await
    }

    /// 取消进行中的传输（发送或接收）。幂等：无此在途任务则无操作。
    pub async fn cancel(&self, task_id: &str) -> Result<()> {
        if let Some(token) = self.cancels.lock().expect("cancels lock").get(task_id) {
            token.cancel();
        }
        Ok(())
    }
}

/// 把未指定地址（0.0.0.0 / ::）归一到回环，便于连接本机绑定的接收端。
fn connect_addr(addr: SocketAddr) -> SocketAddr {
    if addr.ip().is_unspecified() {
        SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), addr.port())
    } else {
        addr
    }
}

/// 处理一条传输更新：维护任务表、广播事件，结束时落历史。
fn drain_transfer_update(
    tasks: &Arc<Mutex<HashMap<String, TransferTask>>>,
    events: &broadcast::Sender<LanEvent>,
    storage: &Arc<storage::Storage>,
    update: TransferUpdate,
) {
    match update {
        TransferUpdate::Started(task) => {
            tasks
                .lock()
                .expect("tasks lock")
                .insert(task.id.clone(), task.clone());
            let _ = events.send(LanEvent::TransferChanged(task));
        }
        TransferUpdate::Progress {
            task_id,
            bytes_done,
            bytes_total,
        } => {
            let _ = events.send(LanEvent::Progress(Progress {
                task_id,
                bytes_done,
                bytes_total,
                speed_bps: 0,
                eta_secs: None,
            }));
        }
        TransferUpdate::Finished(task) if task.state == TransferState::Paused => {
            // 暂停：保留在任务表（可恢复），不落历史。
            tasks
                .lock()
                .expect("tasks lock")
                .insert(task.id.clone(), task.clone());
            let _ = events.send(LanEvent::TransferChanged(task));
        }
        TransferUpdate::Finished(task) => {
            tasks.lock().expect("tasks lock").remove(&task.id);
            let finished_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .ok();
            let row = storage::HistoryRow {
                id: task.id.clone(),
                direction: match task.direction {
                    Direction::Send => "send".to_string(),
                    Direction::Receive => "receive".to_string(),
                },
                peer_alias: task.peer_alias.clone(),
                peer_fingerprint: task.peer_fingerprint.clone(),
                file_name: task.file_name.clone(),
                size: task.size,
                state: task.state.as_str().to_string(),
                finished_at,
                save_path: task.save_path.clone(),
                text: None,
            };
            let _ = storage.insert_history(&row);
            let _ = events.send(LanEvent::TransferChanged(task));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{DeviceInfo, DeviceType, TransportProtocol, PROTOCOL_VERSION};

    fn peer_info(alias: &str, fp: &str) -> DeviceInfo {
        DeviceInfo {
            alias: alias.into(),
            version: PROTOCOL_VERSION.into(),
            device_model: None,
            device_type: Some(DeviceType::Desktop),
            fingerprint: fp.into(),
            port: None,
            protocol: Some(TransportProtocol::Http),
            download: None,
        }
    }

    fn make_service(dir: &std::path::Path, name: &str) -> Arc<LanService> {
        let mut config = LanConfig::new(name, dir.join("data"), dir.join("save"));
        std::fs::create_dir_all(&config.data_dir).unwrap();
        std::fs::create_dir_all(&config.default_save_dir).unwrap();
        config.listen_port = 0; // 临时端口，避免占用 53317
        config.discoverable = false; // 测试不起组播，规避环境依赖
        config.use_tls = false; // 测试走明文 HTTP
        Arc::new(LanService::new(config).unwrap())
    }

    /// 走真实 HTTPS + 强制 mTLS 的服务（信任/自动接收/配对类测试用）。
    /// 明文模式下对端无客户端证书 => 已验证指纹为空 => 不可能命中信任，
    /// 因此凡依赖"可信对端"的测试都必须经此走 TLS，并用对方的**真实**证书指纹建立信任。
    fn make_service_tls(dir: &std::path::Path, name: &str) -> Arc<LanService> {
        let mut config = LanConfig::new(name, dir.join("data"), dir.join("save"));
        std::fs::create_dir_all(&config.data_dir).unwrap();
        std::fs::create_dir_all(&config.default_save_dir).unwrap();
        config.listen_port = 0;
        config.discoverable = false;
        // use_tls 保持默认 true。
        Arc::new(LanService::new(config).unwrap())
    }

    #[tokio::test]
    async fn canceled_approval_removes_its_pending_slot() {
        let dir = tempfile::tempdir().unwrap();
        let service = make_service(dir.path(), "receiver");
        let mut events = service.subscribe();
        let approver = service.approver.clone();
        let job = tokio::spawn(async move {
            transport::AcceptPolicy::decide(
                approver.as_ref(),
                "canceled-request",
                &peer_info("peer", "fp"),
                &HashMap::new(),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(3), events.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(service
            .approver
            .pending
            .lock()
            .unwrap()
            .contains_key("canceled-request"));
        job.abort();
        let _ = job.await;
        assert!(service.approver.pending.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn text_service_requires_approval_and_keeps_both_histories() {
        let dir = tempfile::tempdir().unwrap();
        let receiver_dir = dir.path().join("receiver");
        let sender_dir = dir.path().join("sender");
        // mTLS 下文本消息携带已验证指纹，走真实 HTTPS。
        let receiver = make_service_tls(&receiver_dir, "receiver");
        let sender = make_service_tls(&sender_dir, "sender");
        receiver.start().await.unwrap();
        let peer = sender
            .add_manual_device(receiver.local_addr().await.unwrap())
            .await
            .unwrap();
        let mut events = receiver.subscribe();
        let other = receiver.clone();
        let approval = tokio::spawn(async move {
            let event = tokio::time::timeout(Duration::from_secs(3), events.recv())
                .await
                .unwrap()
                .unwrap();
            match event {
                LanEvent::IncomingRequest { task_id, text, .. } => {
                    assert_eq!(text.as_deref(), Some("hello from sender"));
                    assert!(other.history().is_empty());
                    other
                        .respond_incoming(&task_id, IncomingDecision::Accept { save_dir: None })
                        .await
                        .unwrap();
                }
                _ => panic!("expected interactive text approval"),
            }
        });
        let row = sender
            .send_text(&peer.fingerprint, "hello from sender".into())
            .await
            .unwrap();
        approval.await.unwrap();
        assert_eq!(row.direction, "send");
        assert_eq!(
            sender.history()[0].text.as_deref(),
            Some("hello from sender")
        );
        assert_eq!(receiver.history()[0].direction, "receive");
        assert_eq!(
            std::fs::read_dir(&receiver.config.default_save_dir)
                .unwrap()
                .count(),
            0
        );
        receiver.stop().await.unwrap();
    }

    #[tokio::test]
    async fn receive_directory_persists_and_applies_to_running_trusted_receives() {
        let dir = tempfile::tempdir().unwrap();
        let service = make_service_tls(&dir.path().join("recv"), "receiver");
        let original = service.save_directory();
        let next = dir.path().join("chosen folder");
        std::fs::create_dir_all(&next).unwrap();
        service.start().await.unwrap();
        service.set_save_directory(&next).unwrap();
        assert_eq!(service.save_directory(), next.canonicalize().unwrap());
        assert!(service
            .set_save_directory(&dir.path().join("missing"))
            .is_err());
        assert_eq!(service.save_directory(), next.canonicalize().unwrap());

        // mTLS 下接收端以"已验证证书指纹"判断信任：须信任发送端的真实身份指纹，而非自报字符串。
        let sender = make_service_tls(&dir.path().join("send"), "sender");
        sender.start().await.unwrap();
        service
            .storage
            .upsert_trusted(sender.fingerprint(), "sender", None)
            .unwrap();
        service.set_auto_accept(true).unwrap();

        let source = dir.path().join("payload.txt");
        std::fs::write(&source, b"receive into chosen folder").unwrap();
        let dd = sender
            .add_manual_device(service.local_addr().await.unwrap())
            .await
            .unwrap();
        sender
            .send_files(&dd.fingerprint, vec![source.clone()])
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !next.join("payload.txt").exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(next.join("payload.txt")).unwrap(),
            b"receive into chosen folder"
        );
        assert!(!original.join("payload.txt").exists());
        sender.stop().await.unwrap();
        service.stop().await.unwrap();
        let restored = LanService::new(service.config.clone()).unwrap();
        assert_eq!(restored.save_directory(), next.canonicalize().unwrap());
    }

    #[tokio::test]
    async fn interactive_approval_accepts_and_receives() {
        let dir = tempfile::tempdir().unwrap();
        let service = make_service_tls(&dir.path().join("recv"), "recv");
        let save_dir = service.config().default_save_dir.clone();
        service.start().await.unwrap();

        // 收到 IncomingRequest 后 accept。
        let mut events = service.subscribe();
        let svc = service.clone();
        let approver = tokio::spawn(async move {
            while let Ok(ev) = events.recv().await {
                if let LanEvent::IncomingRequest { task_id, .. } = ev {
                    svc.respond_incoming(&task_id, IncomingDecision::Accept { save_dir: None })
                        .await
                        .unwrap();
                    break;
                }
            }
        });

        let sender = make_service_tls(&dir.path().join("send"), "sender");
        sender.start().await.unwrap();
        let dd = sender
            .add_manual_device(service.local_addr().await.unwrap())
            .await
            .unwrap();
        let src = dir.path().join("f.bin");
        let payload = vec![3u8; 200 * 1024];
        tokio::fs::write(&src, &payload).await.unwrap();
        sender
            .send_files(&dd.fingerprint, vec![src.clone()])
            .await
            .unwrap();

        approver.await.unwrap();
        let got = tokio::fs::read(save_dir.join("f.bin")).await.unwrap();
        assert_eq!(got, payload);

        // 历史应记录一条已完成的接收记录（drain 为异步，轮询等待）。
        let mut rows = Vec::new();
        for _ in 0..50 {
            rows = service.history();
            if !rows.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].direction, "receive");
        assert_eq!(rows[0].state, "COMPLETED");

        // 接受后发送方应被记为可信设备（按已验证证书指纹 TOFU）。
        assert!(service
            .trusted_devices()
            .iter()
            .any(|t| t.fingerprint == sender.fingerprint()));

        sender.stop().await.unwrap();
        service.stop().await.unwrap();
    }

    #[tokio::test]
    async fn send_files_resolves_manually_added_device() {
        let dir = tempfile::tempdir().unwrap();

        // 对端：AcceptAll 接收端。
        let peer_save = dir.path().join("peer-save");
        tokio::fs::create_dir_all(&peer_save).await.unwrap();
        let peer = transport::Receiver::start(
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(transport::AcceptAll {
                save_dir: peer_save.clone(),
            }),
            peer_info("peer", "peer-fp"),
            None,
            None,
        )
        .await
        .unwrap();
        let peer_addr = peer.local_addr();

        // 发送端 LanService：手动添加对端后按指纹发送。
        let service = make_service(dir.path(), "sender");
        service.start().await.unwrap();
        let dd = service.add_manual_device(peer_addr).await.unwrap();
        assert_eq!(dd.fingerprint, "peer-fp");

        let src = dir.path().join("s.bin");
        let payload = vec![9u8; 128 * 1024];
        tokio::fs::write(&src, &payload).await.unwrap();
        service
            .send_files(&dd.fingerprint, vec![src.clone()])
            .await
            .unwrap();

        let got = tokio::fs::read(peer_save.join("s.bin")).await.unwrap();
        assert_eq!(got, payload);
        tokio::time::timeout(Duration::from_secs(3), async {
            while service.history().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            service.history()[0].save_path.as_deref(),
            Some(src.canonicalize().unwrap().to_string_lossy().as_ref())
        );
        service.stop().await.unwrap();
        peer.shutdown().await;
    }

    #[tokio::test]
    async fn tls_end_to_end_pins_and_rejects_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let mut config =
            LanConfig::new("recv-tls", dir.path().join("data"), dir.path().join("save"));
        std::fs::create_dir_all(&config.data_dir).unwrap();
        std::fs::create_dir_all(&config.default_save_dir).unwrap();
        config.listen_port = 0;
        config.discoverable = false;
        // use_tls 保持默认 true：真正走 HTTPS。
        let service = Arc::new(LanService::new(config).unwrap());
        let save_dir = service.config().default_save_dir.clone();
        let fingerprint = service.fingerprint().to_string();
        service.start().await.unwrap();
        let port = service.local_addr().await.unwrap().port();

        let mut events = service.subscribe();
        let svc = service.clone();
        let approver = tokio::spawn(async move {
            while let Ok(ev) = events.recv().await {
                if let LanEvent::IncomingRequest { task_id, .. } = ev {
                    svc.respond_incoming(&task_id, IncomingDecision::Accept { save_dir: None })
                        .await
                        .unwrap();
                    break;
                }
            }
        });

        let src = dir.path().join("f.bin");
        let payload = vec![5u8; 300 * 1024];
        tokio::fs::write(&src, &payload).await.unwrap();

        // mTLS：发送端需出示自己的客户端证书。
        let sender_id = tls::Identity::load_or_create(&dir.path().join("sender-id")).unwrap();

        // 正确指纹：HTTPS 传输成功。
        transport::send_file(
            &format!("https://127.0.0.1:{port}"),
            &peer_info("sender", "s-fp"),
            &src,
            Some(fingerprint.clone()),
            Some(&sender_id),
        )
        .await
        .unwrap();
        approver.await.unwrap();
        assert_eq!(
            tokio::fs::read(save_dir.join("f.bin")).await.unwrap(),
            payload
        );

        // 错误指纹：TLS 握手阶段即被拒（证明确实在做 pinning，而非仅加密）。
        let mismatch = transport::send_file(
            &format!("https://127.0.0.1:{port}"),
            &peer_info("sender", "s-fp"),
            &src,
            Some("0".repeat(64)),
            Some(&sender_id),
        )
        .await;
        assert!(mismatch.is_err());

        service.stop().await.unwrap();
    }

    /// 修复验证：接收端只认 mTLS 握手验证过的客户端证书指纹，不认请求体自报的指纹。
    /// 冒充者即便在 info.fingerprint 里填上可信设备的指纹，只要出示的是自己的证书，
    /// 就不会被自动接收（落回交互审批），而真正可信的对端照常自动接收。
    #[tokio::test]
    async fn tls_rejects_body_asserted_trusted_fingerprint_from_untrusted_cert() {
        let dir = tempfile::tempdir().unwrap();
        let recv = make_service_tls(&dir.path().join("recv"), "recv");
        recv.start().await.unwrap();
        let save_dir = recv.config().default_save_dir.clone();
        let port = recv.local_addr().await.unwrap().port();

        // 真正可信的发送端：recv 信任其**真实证书指纹**并开启自动接收。
        let trusted = make_service_tls(&dir.path().join("trusted"), "trusted");
        trusted.start().await.unwrap();
        recv.storage
            .upsert_trusted(trusted.fingerprint(), "trusted", None)
            .unwrap();
        recv.set_auto_accept(true).unwrap();

        // 控制组：可信端发送 => 自动接收、落盘，无需审批。
        let ok = dir.path().join("ok.bin");
        std::fs::write(&ok, b"from the real trusted peer").unwrap();
        let dd = trusted
            .add_manual_device(recv.local_addr().await.unwrap())
            .await
            .unwrap();
        trusted
            .send_files(&dd.fingerprint, vec![ok.clone()])
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !save_dir.join("ok.bin").exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("可信端应被自动接收");

        // 冒充者：出示自己的证书，却在请求体里自报可信端的指纹。
        let attacker_id = tls::Identity::load_or_create(&dir.path().join("attacker")).unwrap();
        assert_ne!(attacker_id.fingerprint, trusted.fingerprint());
        let evil = dir.path().join("evil.bin");
        std::fs::write(&evil, b"malware from impostor").unwrap();

        // 订阅事件：冒充者若被弹到 IncomingRequest，即证明"未被当作可信自动接收"。
        let mut events = recv.subscribe();
        let spoofed_fp = trusted.fingerprint().to_string();
        let recv_fp = recv.fingerprint().to_string();
        let attacker = tokio::spawn(async move {
            transport::send_file(
                &format!("https://127.0.0.1:{port}"),
                &peer_info("trusted", &spoofed_fp), // 自报可信指纹
                &evil,
                Some(recv_fp),
                Some(&attacker_id), // 实际出示的是冒充者自己的证书
            )
            .await
        });

        // 必须落到交互审批（即未自动接收），否则漏洞仍在。
        let task_id = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(LanEvent::IncomingRequest { task_id, .. }) = events.recv().await {
                    break task_id;
                }
            }
        })
        .await
        .expect("冒充者绝不能被自动接收，必须落回人工审批");

        // 拒绝之：冒充文件不得落盘，冒充发送以错误结束。
        recv.respond_incoming(&task_id, IncomingDecision::Reject)
            .await
            .unwrap();
        assert!(attacker.await.unwrap().is_err());
        assert!(!save_dir.join("evil.bin").exists());

        trusted.stop().await.unwrap();
        recv.stop().await.unwrap();
    }

    #[tokio::test]
    async fn cancel_aborts_in_flight_send() {
        let dir = tempfile::tempdir().unwrap();
        // 接收端：审批不应答，使发送端 prepare-upload 一直阻塞。
        let recv = make_service(&dir.path().join("r"), "recv");
        recv.start().await.unwrap();
        let recv_port = recv.local_addr().await.unwrap().port();

        let send = make_service(&dir.path().join("s"), "send");
        send.start().await.unwrap();
        let dd = send
            .add_manual_device(format!("127.0.0.1:{recv_port}").parse().unwrap())
            .await
            .unwrap();

        let src = dir.path().join("big.bin");
        tokio::fs::write(&src, vec![1u8; 512 * 1024]).await.unwrap();

        let mut events = send.subscribe();
        let sender = send.clone();
        let fp = dd.fingerprint.clone();
        let handle = tokio::spawn(async move { sender.send_files(&fp, vec![src]).await });

        // 等 Started 事件拿到 task id 后取消在途发送。
        let task_id = loop {
            if let Ok(LanEvent::TransferChanged(t)) = events.recv().await {
                if !t.state.is_terminal() {
                    break t.id;
                }
            }
        };
        send.cancel(&task_id).await.unwrap();

        // 发送因取消返回错误，并落一条 CANCELED 历史。
        assert!(handle.await.unwrap().is_err());
        let mut rows = Vec::new();
        for _ in 0..50 {
            rows = send.history();
            if rows.iter().any(|r| r.state == "CANCELED") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(rows.iter().filter(|r| r.state == "CANCELED").count(), 1);

        recv.stop().await.unwrap();
        send.stop().await.unwrap();
    }

    #[tokio::test]
    async fn pause_then_resume_completes() {
        use crate::transfer::TransferState;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().unwrap();
        let recv = make_service(&dir.path().join("r"), "recv");
        recv.start().await.unwrap();
        let recv_port = recv.local_addr().await.unwrap().port();
        let save_dir = recv.config().default_save_dir.clone();

        // 接收端审批：仅当 accepting=true（即恢复阶段）时接受。
        let accepting = Arc::new(AtomicBool::new(false));
        let acc = accepting.clone();
        let mut recv_events = recv.subscribe();
        let recv2 = recv.clone();
        let approver = tokio::spawn(async move {
            while let Ok(ev) = recv_events.recv().await {
                if let LanEvent::IncomingRequest { task_id, .. } = ev {
                    if acc.load(Ordering::SeqCst) {
                        let _ = recv2
                            .respond_incoming(&task_id, IncomingDecision::Accept { save_dir: None })
                            .await;
                        break;
                    }
                }
            }
        });

        let send = make_service(&dir.path().join("s"), "send");
        send.start().await.unwrap();
        let dd = send
            .add_manual_device(format!("127.0.0.1:{recv_port}").parse().unwrap())
            .await
            .unwrap();
        let src = dir.path().join("p.bin");
        let payload = vec![4u8; 256 * 1024];
        tokio::fs::write(&src, &payload).await.unwrap();

        // 启动发送，拿到 task id 后暂停。
        let mut send_events = send.subscribe();
        let sender = send.clone();
        let fp = dd.fingerprint.clone();
        let srcp = src.clone();
        let handle = tokio::spawn(async move { sender.send_files(&fp, vec![srcp]).await });
        let task_id = loop {
            if let Ok(LanEvent::TransferChanged(t)) = send_events.recv().await {
                if !t.state.is_terminal() {
                    break t.id;
                }
            }
        };
        send.pause(&task_id).await.unwrap();
        handle.await.unwrap().unwrap(); // 暂停不算错误

        // 任务应标记 PAUSED 并保留在任务表（drain 异步，轮询）。
        let mut paused_seen = false;
        for _ in 0..50 {
            if send
                .transfers()
                .iter()
                .any(|t| t.id == task_id && t.state == TransferState::Paused)
            {
                paused_seen = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(paused_seen);

        // 恢复 → 完成。
        accepting.store(true, Ordering::SeqCst);
        send.resume(&task_id).await.unwrap();
        approver.await.unwrap();
        let got = tokio::fs::read(save_dir.join("p.bin")).await.unwrap();
        assert_eq!(got, payload);
        recv.stop().await.unwrap();
        send.stop().await.unwrap();
    }

    #[tokio::test]
    async fn pairing_then_auto_accepts_and_threads_conversation() {
        let dir = tempfile::tempdir().unwrap();
        // 配对与配对后自动接收都依赖"可信对端"，必须走真实 mTLS。
        let recv = make_service_tls(&dir.path().join("r"), "recv");
        recv.start().await.unwrap();
        let recv_port = recv.local_addr().await.unwrap().port();
        let save_dir = recv.config().default_save_dir.clone();

        let send = make_service_tls(&dir.path().join("s"), "send");
        send.start().await.unwrap();
        let peer = send
            .add_manual_device(format!("127.0.0.1:{recv_port}").parse().unwrap())
            .await
            .unwrap();
        assert!(!peer.trusted);

        // recv 收到配对请求后接受。
        let mut recv_events = recv.subscribe();
        let recv2 = recv.clone();
        let pair_approver = tokio::spawn(async move {
            while let Ok(ev) = recv_events.recv().await {
                if let LanEvent::PairRequest { request_id, .. } = ev {
                    recv2.respond_pair(&request_id, true).await.unwrap();
                    break;
                }
            }
        });
        send.pair(&peer.fingerprint).await.unwrap();
        pair_approver.await.unwrap();

        // 双方互为可信。
        assert!(send
            .trusted_devices()
            .iter()
            .any(|t| t.fingerprint == peer.fingerprint));
        assert!(recv
            .trusted_devices()
            .iter()
            .any(|t| t.fingerprint == send.fingerprint()));

        // 配对后发文件：接收端自动接收，无需 respond_incoming。
        // auto_accept 默认已关，显式开启以验证"配对即自动接收"。
        recv.set_auto_accept(true).unwrap();
        let src = dir.path().join("paired.bin");
        let payload = vec![8u8; 100 * 1024];
        tokio::fs::write(&src, &payload).await.unwrap();
        send.send_files(&peer.fingerprint, vec![src]).await.unwrap();
        let mut ok = false;
        for _ in 0..100 {
            if tokio::fs::try_exists(save_dir.join("paired.bin"))
                .await
                .unwrap_or(false)
            {
                ok = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(ok, "配对后文件应自动接收落盘");
        assert_eq!(
            tokio::fs::read(save_dir.join("paired.bin")).await.unwrap(),
            payload
        );

        // 会话按设备分线：send 侧该会话有一条发送记录。
        let mut conv = Vec::new();
        for _ in 0..100 {
            conv = send.conversation(&peer.fingerprint);
            if !conv.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(conv
            .iter()
            .any(|r| r.file_name == "paired.bin" && r.direction == "send"));
        // recv 侧会话（按 send 指纹）有一条接收记录，且带本地保存路径。
        let rconv = recv.conversation(send.fingerprint());
        assert!(rconv
            .iter()
            .any(|r| r.direction == "receive" && r.save_path.is_some()));

        recv.stop().await.unwrap();
        send.stop().await.unwrap();
    }
}
