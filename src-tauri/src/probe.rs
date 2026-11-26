use crate::message::{msg, text};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    error::Error as StdError,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

static SESSION: Mutex<Option<Arc<Session>>> = Mutex::new(None);
static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static STOPPING: AtomicBool = AtomicBool::new(false);
/// 并发探测上限，与 UI「测速并发」上限一致（见 src/lib/preferences.ts 的 1–64 clamp）。
/// 作为后端兜底：前端 worker 数已 clamp 到同值，二者对齐可避免高并发时节点被误判为失败。
const MAX_ACTIVE_PROBES: usize = 64;
/// 测速计时窗口 10s(下载/上传共用):消耗随实测网速放大 ≈ 网速 × 10s(如 100Mbps≈125MB、
/// 1Gbps≈1.25GB),据此在测速弹窗向用户提示流量。MAX_BYTES 仅作安全上限,防计时异常时跑飞
/// (约 1.7Gbps 以内均由窗口主导)。进度回报间隔 250ms。供下载 `speed()` 与上传流式 body 共享。
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const WINDOW: Duration = Duration::from_secs(10);
const EMIT_EVERY: Duration = Duration::from_millis(250);
struct Slot;
impl Drop for Slot {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
    }
}
struct Process(Mutex<Child>);
impl Process {
    fn stop(&self) {
        let mut child = self.0.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.stop();
    }
}
struct Session {
    id: String,
    process: Process,
    ports: HashMap<String, u16>,
    /// Servers between this machine and each probed node: 1 for a node dialled directly,
    /// more when it is reached through a chain. A probe has to budget for every hop.
    hops: HashMap<String, u32>,
    cancelled: AtomicBool,
    _file: tempfile::NamedTempFile,
}
impl Session {
    fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.process.stop();
    }
}

fn isolated_config(raw: &str, targets: &[(String, u16)]) -> Result<Value, String> {
    if raw.len() > crate::network_settings::MAX_CONFIG_BYTES {
        return Err("native.configTooLarge".into());
    }
    if targets.is_empty() || targets.len() > 512 {
        return Err("native.probeNodeCount".into());
    }
    let source: Value = serde_json::from_str(raw).map_err(|_| "native.configInvalid")?;
    // Only the outbounds are retained below; validate them before starting a local process.
    airport_tun_service::validate_profile_options(&source["outbounds"])?;
    let outbounds = source["outbounds"]
        .as_array()
        .ok_or("native.configNoNodes")?;
    let mut inbounds = Vec::new();
    let mut rules = Vec::new();
    for (index, (tag, port)) in targets.iter().enumerate() {
        if !outbounds
            .iter()
            .any(|o| o["tag"].as_str() == Some(tag) && o["server"].as_str().is_some())
        {
            return Err("native.probeNodeNotFound".into());
        }
        let inbound = format!("probe-{index}");
        inbounds
            .push(json!({"type":"mixed","tag":inbound,"listen":"127.0.0.1","listen_port":port}));
        rules.push(json!({"inbound":[inbound],"action":"route","outbound":tag}));
    }
    // One shared core, an isolated local inbound per node; never inherit TUN/API/provider routing.
    // sing-box 1.14 dials a domain only through a domain resolver. Without one, a Hysteria2
    // (UDP) node follows a dead IPv6 answer until this probe's budget runs out and shows as a timeout.
    Ok(json!({
        "log": {"disabled": true},
        "dns": {"servers": [{"type": "local", "tag": "probe-dns"}]},
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": {
            "rules": rules,
            "final": targets[0].0,
            "auto_detect_interface": true,
            "default_domain_resolver": {"server": "probe-dns", "strategy": "prefer_ipv4"}
        }
    }))
}

pub fn begin(raw: String, tags: Vec<String>) -> Result<String, String> {
    if STOPPING.load(Ordering::SeqCst) {
        return Err("native.appQuitting".into());
    }
    // Serialize setup so a cancelled/replaced round cannot leave a second test core behind.
    let mut current = SESSION.lock().unwrap();
    if current.is_some() {
        return Err("native.probeStillRunning".into());
    }
    if tags.is_empty() || tags.len() > 512 {
        return Err("native.probeNodeCount".into());
    }
    let mut listeners = Vec::new();
    let mut targets = Vec::new();
    for tag in tags {
        let socket = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        targets.push((tag, socket.local_addr().map_err(|e| e.to_string())?.port()));
        listeners.push(socket);
    }
    let cfg = isolated_config(&raw, &targets)?;
    let outbounds = cfg["outbounds"].as_array().cloned().unwrap_or_default();
    let hops: HashMap<String, u32> = targets
        .iter()
        .map(|(tag, _)| (tag.clone(), hop_count(&outbounds, tag)))
        .collect();
    let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    file.write_all(
        serde_json::to_string(&cfg)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    drop(listeners);
    let mut cmd = Command::new(super::resolve_binary()?);
    let process = Process(Mutex::new(
        crate::hide_console(&mut cmd)
            .args(["run", "-c"])
            .arg(file.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "native.probeCoreSpawnFailed")?,
    ));
    let addr = ([127, 0, 0, 1], targets.last().unwrap().1).into();
    // The first launch after install is a cold start (the OS verifies the binary), which can take
    // well over 3 seconds; the loop still leaves as soon as the port answers.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if STOPPING.load(Ordering::SeqCst) {
            return Err("native.appQuitting".into());
        }
        if process
            .0
            .lock()
            .unwrap()
            .try_wait()
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Err("native.probeConfigRejected".into());
        }
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            break;
        }
        if Instant::now() >= deadline {
            return Err("native.probeCoreStartTimeout".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let id = SEQUENCE.fetch_add(1, Ordering::SeqCst).to_string();
    *current = Some(Arc::new(Session {
        id: id.clone(),
        process,
        ports: targets.into_iter().collect(),
        hops,
        cancelled: AtomicBool::new(false),
        _file: file,
    }));
    Ok(id)
}
pub fn end(id: &str) {
    let session = {
        let mut current = SESSION.lock().unwrap();
        if current.as_ref().is_some_and(|s| s.id == id) {
            current.take()
        } else {
            None
        }
    };
    if let Some(session) = session {
        session.stop();
    }
}
pub fn stop_all() {
    STOPPING.store(true, Ordering::SeqCst);
    let session = SESSION.lock().unwrap().take();
    if let Some(session) = session {
        session.stop();
    }
}
#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DelayResult {
    delay: u32,
    cold_delay: u32,
    warmed: bool,
}

/// Turn a failed probe request into a message that keeps the real cause. Only a genuine
/// timeout is reported as one; a DNS, TLS, refused-connection, authentication or proxy-tunnel
/// failure keeps its own reason instead of being hidden behind "timed out". TLS verification is
/// never relaxed to make a node pass, so a certificate error surfaces here as itself.
fn describe_request_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "native.probeTimedOut".into();
    }
    // Walk to the deepest cause so the message carries the concrete reason (e.g. "dns error",
    // "invalid peer certificate", "connection refused") rather than reqwest's generic wrapper.
    let mut cause: &dyn StdError = error;
    while let Some(source) = cause.source() {
        cause = source;
    }
    msg("native.probeFailed", &[("error", text(cause))])
}

fn measure_delay(
    client: &reqwest::blocking::Client,
    url: &str,
    budget: Duration,
    warm: Duration,
) -> Result<DelayResult, String> {
    let started = Instant::now();
    let first = client
        .head(url)
        .timeout(budget)
        .send()
        .map_err(|error| describe_request_error(&error))?;
    if first.status() != reqwest::StatusCode::NO_CONTENT {
        return Err(msg(
            "native.probeHttpStatus",
            &[("status", text(first.status()))],
        ));
    }
    drop(first);
    let cold_delay = started.elapsed().as_millis().clamp(1, u32::MAX as u128) as u32;
    let started = Instant::now();
    let second = client.head(url).timeout(warm).send();
    match second {
        Ok(response) if response.status() == reqwest::StatusCode::NO_CONTENT => Ok(DelayResult {
            delay: started.elapsed().as_millis().clamp(1, u32::MAX as u128) as u32,
            cold_delay,
            warmed: true,
        }),
        _ => Ok(DelayResult {
            delay: cold_delay,
            cold_delay,
            warmed: false,
        }),
    }
}

/// How long a probe may take for a node reached through `hops` servers.
///
/// A single hop is measured in a batch of up to hundreds: a node that cannot answer in seconds
/// is not worth using, and none of them may hold a slot for a minute. A chain is one explicit
/// measurement the user is waiting for, so it gets the time it needs — every hop costs its own
/// TCP and TLS handshake, and a timeout would hide the real cost instead of reporting it.
const fn probe_budget(hops: u32) -> (Duration, Duration) {
    if hops <= 1 {
        (Duration::from_secs(6), Duration::from_secs(2))
    } else {
        (Duration::from_secs(60), Duration::from_secs(10))
    }
}

/// How many servers one outbound dials through: itself, plus every `detour` it names.
/// A loop is counted once, so a malformed profile cannot make this run away.
fn hop_count(outbounds: &[Value], tag: &str) -> u32 {
    let by_tag: HashMap<&str, &Value> = outbounds
        .iter()
        .filter_map(|o| o["tag"].as_str().map(|t| (t, o)))
        .collect();
    let mut hops = 1;
    let mut current = tag;
    let mut seen = HashSet::from([tag]);
    while let Some(outbound) = by_tag.get(current) {
        match outbound["detour"].as_str().filter(|next| !next.is_empty()) {
            Some(next) if seen.insert(next) => {
                hops += 1;
                current = next;
            }
            _ => break,
        }
    }
    hops
}

pub fn delay(id: String, tag: String) -> Result<DelayResult, String> {
    let session = SESSION
        .lock()
        .unwrap()
        .as_ref()
        .filter(|s| s.id == id)
        .cloned()
        .ok_or("native.probeEnded")?;
    if session.cancelled.load(Ordering::SeqCst) {
        return Err("native.probeStopped".into());
    }
    if ACTIVE.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIVE_PROBES {
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
        return Err("native.probeSlotsBusy".into());
    }
    let _slot = Slot;
    let port = session.ports.get(&tag).ok_or("native.probeNodeNotFound")?;
    let (budget, warm) = probe_budget(*session.hops.get(&tag).unwrap_or(&1));
    let client = reqwest::blocking::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).map_err(|e| e.to_string())?)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    measure_delay(
        &client,
        "https://cp.cloudflare.com/generate_204",
        budget,
        warm,
    )
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpeedResult {
    /// 实测平均下载速率（Mbps）。
    mbps: f64,
    /// 测速窗口内的峰值瞬时速率（Mbps）。
    peak_mbps: f64,
    /// 累计下载字节数。
    bytes: u64,
    /// 实际计时时长（毫秒）。
    millis: u64,
}

/// 单节点下载测速。复用隔离测试核中该节点的本地端口，经代理流式拉取一段数据并计时，
/// 期间按固定间隔通过 `on_progress(瞬时Mbps, 累计字节)` 回报进度以驱动前端仪表盘。
///
/// 下载源为一组免鉴权的公开大文件,按序回退。计时窗口 10s,消耗随实测网速放大 ≈ 网速 × 10s
/// (见测速弹窗的消耗表);MAX_BYTES(2GiB)仅作安全上限,约 1.7Gbps 以内均由窗口主导。
pub fn speed<F: Fn(f64, u64)>(id: String, tag: String, on_progress: F) -> Result<SpeedResult, String> {
    // 多源回退。Cloudflare(全球 anycast)最准,通时首选;被机场/CDN 封则毫秒级落下一个。次选
    // dl.google.com——同为全球边缘(就近接入故测量准),且为非「测速」域名、机场基本不封,是被封
    // 时的主力;单区的 OVH/Hetzner 大文件仅作最后兜底(跨洲链路会低估真实速度,排在全球边缘源之后)。
    // 首项 Cloudflare bytes 取 2GiB 以够高速节点跑满 10s 窗口;其余读满窗口或文件 EOF 即断开。
    const SOURCES: &[&str] = &[
        "https://speed.cloudflare.com/__down?bytes=2147483648",
        "https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb",
        "https://proof.ovh.net/files/1Gb.dat",
        "https://ash-speed.hetzner.com/1GB.bin",
    ];
    // 常见浏览器 UA,降低被 CDN 以「疑似脚本」为由直接拒绝(403)的概率。
    const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

    let session = SESSION
        .lock()
        .unwrap()
        .as_ref()
        .filter(|s| s.id == id)
        .cloned()
        .ok_or("native.probeEnded")?;
    if session.cancelled.load(Ordering::SeqCst) {
        return Err("native.probeStopped".into());
    }
    let port = session.ports.get(&tag).ok_or("native.probeNodeNotFound")?;
    let client = reqwest::blocking::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).map_err(|e| e.to_string())?)
        // 通用大文件镜像常带跨区/HTTPS 跳转,允许有限次重定向;上限防重定向环。
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent(UA)
        // 兜底:单次请求(含流式读取)空闲超过该时长即判定失败;正常窗口由下方 10s 控制。
        .timeout(Duration::from_secs(25))
        // 连接(含代理 CONNECT)阶段单独限时,让失效源快速失败、尽快回退到下一个。
        .connect_timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    // 依次尝试各源,记录最后一次失败原因;首个返回 2xx 的即用于测速,全部失败才整体报错。
    let mut response: Option<reqwest::blocking::Response> = None;
    let mut last_error: Option<String> = None;
    for url in SOURCES {
        // 用户关闭弹窗会 end 掉测试核,逐源重试前先感知,避免无谓拉起下一个源。
        if session.cancelled.load(Ordering::SeqCst) {
            return Err("native.probeStopped".into());
        }
        match client.get(*url).send() {
            Ok(resp) if resp.status().is_success() => {
                response = Some(resp);
                break;
            }
            Ok(resp) => {
                last_error = Some(msg(
                    "native.probeHttpStatus",
                    &[("status", text(resp.status()))],
                ));
            }
            Err(error) => last_error = Some(describe_request_error(&error)),
        }
    }
    // SOURCES 非空,循环至少跑一轮,未命中时 last_error 必有值;兜底键仅为类型完整,不会触达。
    let mut response = match response {
        Some(resp) => resp,
        None => return Err(last_error.unwrap_or_else(|| "native.probeTimedOut".into())),
    };

    let started = Instant::now();
    let mut total: u64 = 0;
    let mut peak: f64 = 0.0;
    let mut window_start = Instant::now();
    let mut window_bytes: u64 = 0;
    let mut buf = [0u8; 65536];
    loop {
        // 用户关闭弹窗会 end 掉测试核，此处提前感知以尽快退出下载循环。
        if session.cancelled.load(Ordering::SeqCst) {
            return Err("native.probeStopped".into());
        }
        let read = response.read(&mut buf).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        total += read as u64;
        window_bytes += read as u64;
        let elapsed = window_start.elapsed();
        if elapsed >= EMIT_EVERY {
            let mbps = window_bytes as f64 * 8.0 / 1_000_000.0 / elapsed.as_secs_f64();
            peak = peak.max(mbps);
            on_progress(mbps, total);
            window_start = Instant::now();
            window_bytes = 0;
        }
        if total >= MAX_BYTES || started.elapsed() >= WINDOW {
            break;
        }
    }
    let secs = started.elapsed().as_secs_f64().max(0.001);
    let mbps = total as f64 * 8.0 / 1_000_000.0 / secs;
    Ok(SpeedResult {
        mbps,
        peak_mbps: peak.max(mbps),
        bytes: total,
        millis: started.elapsed().as_millis() as u64,
    })
}

/// 上传测速用的流式请求体:按 10s 窗口与安全上限产生数据(全 0;TLS 下不可压缩,测的是真实
/// 吞吐),按 250ms 间隔经 `cb` 回报瞬时速率;读到窗口/上限或被取消即结束。统计量写入共享
/// [`UploadStats`],body 被 reqwest 取走后仍可读回最终字节/峰值/时长。
#[derive(Default)]
struct UploadStats {
    bytes: AtomicU64,
    /// 峰值瞬时速率(Mbps)的 f64 位型;单线程写,无需 CAS。
    peak_bits: AtomicU64,
    /// 实际传输时长(ms),自首字节起计。
    millis: AtomicU64,
}

struct UploadBody {
    session: Arc<Session>,
    stats: Arc<UploadStats>,
    cb: Arc<dyn Fn(f64, u64) + Send + Sync>,
    /// 首次被读取(真正开始上传)时才起计,排除连接/握手耗时。
    start: Option<Instant>,
    window_start: Instant,
    window_bytes: u64,
    total: u64,
    cap: u64,
}

impl Read for UploadBody {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // 弹窗关闭会 end 测试核;以错误中断上传,send() 随之失败并由上层归为取消。
        if self.session.cancelled.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "native.probeStopped",
            ));
        }
        let start = match self.start {
            Some(s) => s,
            None => {
                let now = Instant::now();
                self.start = Some(now);
                self.window_start = now;
                now
            }
        };
        let elapsed = start.elapsed();
        if self.total >= self.cap || elapsed >= WINDOW {
            self.stats
                .millis
                .store(elapsed.as_millis() as u64, Ordering::SeqCst);
            return Ok(0);
        }
        let n = buf.len().min((self.cap - self.total) as usize);
        // memset(编译器优化为块填充),避免把 reqwest 缓冲区残留内存发到网络;测的是纯吞吐。
        buf[..n].fill(0);
        self.total += n as u64;
        self.window_bytes += n as u64;
        self.stats.bytes.store(self.total, Ordering::SeqCst);
        let win = self.window_start.elapsed();
        if win >= EMIT_EVERY {
            let mbps = self.window_bytes as f64 * 8.0 / 1_000_000.0 / win.as_secs_f64();
            if mbps > f64::from_bits(self.stats.peak_bits.load(Ordering::SeqCst)) {
                self.stats.peak_bits.store(mbps.to_bits(), Ordering::SeqCst);
            }
            (*self.cb)(mbps, self.total);
            self.window_start = Instant::now();
            self.window_bytes = 0;
        }
        self.stats
            .millis
            .store(elapsed.as_millis() as u64, Ordering::SeqCst);
        Ok(n)
    }
}

/// 单节点上传测速。与下载对称:复用隔离测试核中该节点端口,经代理向免鉴权 sink 流式 POST 一段
/// 数据并计时。源回退:Cloudflare 专用 `__up`(无上限,通时首选)→ 门户 `generate_204`(读大
/// POST 返 204,机场封测速域名时仍可达,延迟探测即走此域)。两者均接受 chunked body。
pub fn speed_up<F: Fn(f64, u64) + Send + Sync + 'static>(
    id: String,
    tag: String,
    on_progress: F,
) -> Result<SpeedResult, String> {
    const SOURCES: &[&str] = &[
        "https://speed.cloudflare.com/__up",
        "https://cp.cloudflare.com/generate_204",
    ];
    const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

    let session = SESSION
        .lock()
        .unwrap()
        .as_ref()
        .filter(|s| s.id == id)
        .cloned()
        .ok_or("native.probeEnded")?;
    if session.cancelled.load(Ordering::SeqCst) {
        return Err("native.probeStopped".into());
    }
    let port = session.ports.get(&tag).ok_or("native.probeNodeNotFound")?;
    let client = reqwest::blocking::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).map_err(|e| e.to_string())?)
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent(UA)
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    // on_progress 需跨多次源尝试复用,包进 Arc 以便每次新建 body 时克隆。
    let cb: Arc<dyn Fn(f64, u64) + Send + Sync> = Arc::new(on_progress);

    let mut last_error: Option<String> = None;
    let mut measured: Option<Arc<UploadStats>> = None;
    for url in SOURCES {
        if session.cancelled.load(Ordering::SeqCst) {
            return Err("native.probeStopped".into());
        }
        let stats = Arc::new(UploadStats::default());
        let body = UploadBody {
            session: session.clone(),
            stats: stats.clone(),
            cb: cb.clone(),
            start: None,
            window_start: Instant::now(),
            window_bytes: 0,
            total: 0,
            cap: MAX_BYTES,
        };
        match client
            .post(*url)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(reqwest::blocking::Body::new(body))
            .send()
        {
            Ok(resp) if resp.status().is_success() => {
                measured = Some(stats);
                break;
            }
            Ok(resp) => {
                last_error = Some(msg(
                    "native.probeHttpStatus",
                    &[("status", text(resp.status()))],
                ));
            }
            Err(error) => {
                // 取消引发的 body 中断会以请求错误冒出,优先按取消处理。
                if session.cancelled.load(Ordering::SeqCst) {
                    return Err("native.probeStopped".into());
                }
                last_error = Some(describe_request_error(&error));
            }
        }
    }
    let stats = match measured {
        Some(stats) => stats,
        None => return Err(last_error.unwrap_or_else(|| "native.probeTimedOut".into())),
    };
    let bytes = stats.bytes.load(Ordering::SeqCst);
    let millis = stats.millis.load(Ordering::SeqCst).max(1);
    let mbps = bytes as f64 * 8.0 / 1_000_000.0 / (millis as f64 / 1000.0);
    let peak = f64::from_bits(stats.peak_bits.load(Ordering::SeqCst)).max(mbps);
    Ok(SpeedResult {
        mbps,
        peak_mbps: peak,
        bytes,
        millis,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probes_refuse_subscriptions_that_reference_local_files() {
        let raw=r#"{"outbounds":[{"type":"trojan","tag":"qa","server":"example.invalid","server_port":443,"tls":{"key_path":"/tmp/private.key"}}]}"#;
        assert!(isolated_config(raw, &[("qa".into(), 2080)]).is_err());
    }
    #[test]
    fn head_probe_uses_second_response_and_keeps_cold_measurement() {
        use std::io::{Read, Write};
        for second_status in [204, 503] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                for status in [204, second_status] {
                    let mut request = Vec::new();
                    while !request.ends_with(b"\r\n\r\n") {
                        let mut byte = [0];
                        stream.read_exact(&mut byte).unwrap();
                        request.push(byte[0]);
                    }
                    assert!(request.starts_with(b"HEAD "));
                    write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n").unwrap();
                }
            });
            let client = reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap();
            let result = measure_delay(
                &client,
                &format!("http://{address}/generate_204"),
                Duration::from_secs(3),
                Duration::from_secs(3),
            )
            .unwrap();
            assert_eq!(result.warmed, second_status == 204);
            assert!(result.delay > 0 && result.cold_delay > 0);
            if second_status != 204 {
                assert_eq!(result.delay, result.cold_delay);
            }
            server.join().unwrap();
        }
    }

    // A node that accepts but never answers is the one failure that is honestly a timeout.
    #[test]
    fn a_slow_node_is_reported_as_a_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let _held = listener.accept();
            std::thread::sleep(Duration::from_millis(400));
        });
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let error = measure_delay(
            &client,
            &format!("http://{address}/generate_204"),
            Duration::from_millis(150),
            Duration::from_millis(150),
        )
        .unwrap_err();
        assert_eq!(error, "native.probeTimedOut");
        server.join().unwrap();
    }

    // A refused connection keeps its own cause instead of masquerading as a timeout.
    #[test]
    fn a_refused_connection_keeps_its_cause() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener); // nothing is listening on the port now
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let error = measure_delay(
            &client,
            &format!("http://{address}/generate_204"),
            // Windows may retry a refused SYN for several seconds. Give the
            // OS time to return its connection error; production probe budgets
            // and the separate unresponsive-node timeout test are unchanged.
            Duration::from_secs(10),
            Duration::from_secs(10),
        )
        .unwrap_err();
        assert_ne!(error, "native.probeTimedOut");
        assert!(
            error.starts_with(r#"{"key":"native.probeFailed""#),
            "unexpected error: {error}"
        );
    }

    // A TLS handshake against a plaintext endpoint fails as a TLS error, never a timeout, and the
    // fix is to report it — not to disable certificate verification so the node "passes".
    #[test]
    fn a_tls_failure_keeps_its_cause() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                drop(stream); // speak no TLS; close the connection immediately
            }
        });
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let error = measure_delay(
            &client,
            &format!("https://{address}/generate_204"),
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert_ne!(error, "native.probeTimedOut");
        assert!(
            error.starts_with(r#"{"key":"native.probeFailed""#),
            "unexpected error: {error}"
        );
        server.join().unwrap();
    }

    // An unresolvable host fails with its DNS error, not a timeout. `.invalid` never resolves
    // (RFC 6761), so the lookup returns rather than hanging.
    #[test]
    fn a_dns_failure_keeps_its_cause() {
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let error = measure_delay(
            &client,
            "http://clashnova-probe-does-not-exist.invalid/generate_204",
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_ne!(error, "native.probeTimedOut");
        assert!(
            error.starts_with(r#"{"key":"native.probeFailed""#),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn routes_each_inbound_only_to_its_node() {
        let cfg=isolated_config(r#"{"outbounds":[{"type":"socks","tag":"one","server":"example.com"},{"type":"socks","tag":"two","server":"example.org"}],"route":{"final":"direct"},"inbounds":[{"type":"tun"}],"experimental":{"clash_api":{"external_controller":"0.0.0.0:9090"}}}"#, &[("one".into(),12345),("two".into(),12346)]).unwrap();
        assert_eq!(cfg["route"]["rules"][0]["outbound"], "one");
        assert_eq!(cfg["route"]["rules"][1]["outbound"], "two");
        assert_eq!(cfg["inbounds"][1]["listen_port"], 12346);
        assert_eq!(cfg["inbounds"][0]["listen"], "127.0.0.1");
        assert!(cfg.get("experimental").is_none());
        assert_eq!(cfg["dns"]["servers"][0]["type"], "local");
        assert_eq!(cfg["route"]["default_domain_resolver"]["server"], "probe-dns");
        assert_eq!(cfg["route"]["default_domain_resolver"]["strategy"], "prefer_ipv4");
    }
    #[test]
    fn rejects_unknown_and_empty_targets() {
        assert!(isolated_config(r#"{"outbounds":[]}"#, &[("unknown".into(), 12345)]).is_err());
        assert!(isolated_config(r#"{"outbounds":[]}"#, &[]).is_err());
    }

    // A chain has to be walked before the budget is known: one hop gets the old 4 s, and every
    // extra hop gets room for another handshake.
    #[test]
    fn a_chain_budgets_for_every_hop() {
        let outbounds = json!([
            {"type":"trojan","tag":"entry","server":"a.example"},
            {"type":"vmess","tag":"exit","server":"b.example","detour":"entry"},
            {"type":"trojan","tag":"third","server":"c.example","detour":"exit"},
            {"type":"direct","tag":"direct"}
        ]);
        assert_eq!(hop_count(outbounds.as_array().unwrap(), "entry"), 1);
        assert_eq!(hop_count(outbounds.as_array().unwrap(), "exit"), 2);
        assert_eq!(hop_count(outbounds.as_array().unwrap(), "third"), 3);
        assert_eq!(hop_count(outbounds.as_array().unwrap(), "direct"), 1);
        // A detour loop is counted once instead of never ending.
        let looped = json!([
            {"type":"trojan","tag":"a","server":"a.example","detour":"b"},
            {"type":"trojan","tag":"b","server":"b.example","detour":"a"}
        ]);
        assert_eq!(hop_count(looped.as_array().unwrap(), "a"), 2);
        // A node in a batch gets seconds; a chain the user is waiting for gets a minute.
        assert_eq!(
            probe_budget(1),
            (Duration::from_secs(6), Duration::from_secs(2))
        );
        for hops in [2, 3, 8] {
            assert_eq!(
                probe_budget(hops),
                (Duration::from_secs(60), Duration::from_secs(10)),
                "{hops} hops"
            );
        }
    }

    // Opt-in network integration test. Supply a private local sing-box profile;
    // output is aggregate timing only, never subscription credentials or node URLs.
    #[test]
    #[ignore = "requires AIRPORT_TEST_PROFILE and real network access"]
    fn real_subscription_batch() {
        let path = std::env::var("AIRPORT_TEST_PROFILE").expect("AIRPORT_TEST_PROFILE");
        let raw = std::fs::read_to_string(path).unwrap();
        let config: Value = serde_json::from_str(&raw).unwrap();
        let tags: Vec<String> = config["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| node["server"].is_string())
            .map(|node| node["tag"].as_str().unwrap().into())
            .collect();
        let started = Instant::now();
        let session = begin(raw, tags.clone()).expect("start shared test core");
        let cursor = AtomicUsize::new(0);
        let passed = AtomicUsize::new(0);
        let first = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| loop {
                    let index = cursor.fetch_add(1, Ordering::SeqCst);
                    let Some(tag) = tags.get(index) else {
                        break;
                    };
                    if delay(session.clone(), tag.clone()).is_ok() {
                        passed.fetch_add(1, Ordering::SeqCst);
                    }
                    let _ = first.compare_exchange(
                        0,
                        started.elapsed().as_millis() as usize,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    );
                });
            }
        });
        end(&session);
        assert!(SESSION.lock().unwrap().is_none());
        assert_eq!(ACTIVE.load(Ordering::SeqCst), 0);
        println!(
            "nodes={} passed={} first_result_ms={} total_ms={}",
            tags.len(),
            passed.load(Ordering::SeqCst),
            first.load(Ordering::SeqCst),
            started.elapsed().as_millis()
        );
        assert!(passed.load(Ordering::SeqCst) > 0, "no real node passed");
    }
}
