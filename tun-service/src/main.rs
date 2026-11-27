#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;
mod system_dns;
use airport_tun_service::{
    message::{msg, text},
    now, read_message, safe_config, valid_id, write_message, Action, Credentials, Request,
    Response, Result, LEASE_SECONDS, PORT, PROTOCOL,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};
static STOP: AtomicBool = AtomicBool::new(false);
/// A whole request must arrive within this long. Requests come from the same machine; even a
/// multi-megabyte config crosses loopback in milliseconds.
const REQUEST_DEADLINE: Duration = Duration::from_secs(2);
/// Connections being read at once. Any local process can open a connection, so the number is
/// bounded and the excess is dropped rather than queued.
const MAX_PENDING_READS: usize = 16;
const MAX_BATCH: usize = 32;
fn request_channel() -> (
    mpsc::SyncSender<(Request, TcpStream)>,
    mpsc::Receiver<(Request, TcpStream)>,
) {
    mpsc::sync_channel(MAX_PENDING_READS)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Install {
    credentials: Credentials,
    core: PathBuf,
    core_sha256: String,
}
fn main() {
    if let Err(error) = entry() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn entry() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    match args.get(1).and_then(|s| s.to_str()) {
        Some("--install") if args.len() == 3 => install(Path::new(&args[2])),
        Some("--uninstall") if args.len() == 2 => {
            platform::require_admin()?;
            platform::uninstall()
        }
        Some("--serve") if args.len() == 2 => {
            platform::require_admin()?;
            platform::serve()
        }
        _ => Err("Usage: airport-tun-service --install manifest | --serve | --uninstall".into()),
    }
}
fn checked_bytes(path: &Path, expected: &str) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Install source is not a regular file".into());
    }
    let mut bytes = vec![];
    file.take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 * 1024 || format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err("Install file verification failed".into());
    }
    Ok(bytes)
}
fn install(manifest: &Path) -> Result<()> {
    platform::require_admin()?;
    let mut raw = vec![];
    fs::File::open(manifest)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    if raw.len() > 65536 {
        return Err("Install manifest is too large".into());
    }
    let install: Install =
        serde_json::from_slice(&raw).map_err(|_| "Install manifest is invalid")?;
    let core = checked_bytes(&install.core, &install.core_sha256)?;
    let own =
        fs::read(std::env::current_exe().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    platform::prepare_root()?;
    platform::stop_existing()?;
    let root = platform::root();
    secure_write(&root.join(platform::helper_name()), &own, 0o700)?;
    secure_write(&root.join(platform::core_name()), &core, 0o700)?;
    for name in platform::LEGACY_FILES {
        let _ = fs::remove_file(root.join(name));
    }
    secure_write(
        &root.join("credentials.json"),
        &serde_json::to_vec(&install.credentials).map_err(|e| e.to_string())?,
        0o600,
    )?;
    platform::register()?;
    Ok(())
}
pub(crate) fn secure_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension(format!("{}.tmp", airport_tun_service::random_id()?));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let mut file = opts.open(&tmp).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}
struct Running {
    child: Child,
    session: String,
    heartbeat: Instant,
    /// The app confirmed the tunnel works and asked for the kill switch (`Action::Arm`).
    armed: bool,
    /// Destinations the tunnel leaves alone; they stay reachable while the kill switch blocks.
    bypass: Vec<String>,
    /// The LAN-exposed proxy port a firewall rule was opened for (allow LAN); closed on stop.
    lan_port: Option<u16>,
}
impl Drop for Running {
    fn drop(&mut self) {
        platform::terminate(&mut self.child);
        if let Some(port) = self.lan_port {
            platform::close_lan_port(port);
        }
    }
}
/// Blocks the machine's network except for the tunnel and the core, and restores it.
/// The real one drives the system firewall (see `platform::system_guard`).
pub trait Guard {
    fn engage(&mut self, bypass: &[String]) -> Result<()>;
    /// Must succeed when nothing is engaged: it also clears what a crashed service left behind.
    fn release(&mut self) -> Result<()>;
}
/// How long to wait before trying again to lift a block the firewall refused to lift.
const UNBLOCK_RETRY: Duration = Duration::from_secs(2);
/// How often the resolvers are checked while the tunnel runs. A cable plugged in after the
/// tunnel came up brings a network service whose resolver still points at the ISP; this is how
/// it gets taken too.
const RESOLVER_REFRESH: Duration = Duration::from_secs(10);
/// The machine's resolver, taken for as long as the tunnel needs it. The real one drives the
/// system settings (see `system_dns`).
pub trait SystemDns {
    /// Points every enabled network service at the tunnel, remembering what was there.
    fn apply(&mut self) -> Result<()>;
    /// Re-applies to services that appeared or were put back; leaves foreign changes alone.
    fn refresh(&mut self) -> Result<()>;
    /// Gives every service back the resolver it had. Must succeed when nothing was applied:
    /// it also clears a record a crashed service left behind.
    fn restore(&mut self) -> Result<()>;
}
struct Engine {
    journal: Option<airport_tun_service::runtime_log::SharedLog>,
    running: Option<Running>,
    root: PathBuf,
    guard: Box<dyn Guard>,
    dns: Box<dyn SystemDns>,
    /// When the resolvers were last checked.
    dns_checked: Instant,
    /// The kill switch is engaged: the network is blocked until the app arms a working
    /// tunnel again, releases it, or stops answering.
    blocking: bool,
    /// When the app was last heard from. A block is only held for an app that is there to
    /// lift it; like the core, it does not outlive the lease.
    seen: Instant,
    unblock_after: Option<Instant>,
}
impl Engine {
    fn note(&self, line: &str) {
        match &self.journal {
            Some(log) => {
                if let Err(error) = log.lock().unwrap().append(line) {
                    eprintln!("Log write failed: {error}");
                }
            }
            None => eprintln!("{line}"),
        }
    }
    fn block(&mut self, bypass: &[String]) {
        match self.guard.engage(bypass) {
            Ok(()) => {
                self.blocking = true;
                self.unblock_after = None;
                self.note("Kill switch: the core exited unexpectedly, the network is blocked until the tunnel is back");
            }
            // Nothing can be done about a firewall that refuses: say so where the user reads it.
            Err(error) => self.note(&format!("Kill switch could not block the network: {error}")),
        }
    }
    fn unblock(&mut self) -> Result<()> {
        if !self.blocking {
            return Ok(());
        }
        match self.guard.release() {
            Ok(()) => {
                self.blocking = false;
                self.unblock_after = None;
                self.note("Kill switch: the network is no longer blocked");
                Ok(())
            }
            Err(error) => {
                self.unblock_after = Some(Instant::now() + UNBLOCK_RETRY);
                self.note(&format!(
                    "Kill switch could not restore the network, retrying: {error}"
                ));
                Err("native.tunGuardReleaseFailed".into())
            }
        }
    }
    /// Every authenticated request renews the lease on a block.
    fn touch(&mut self) {
        self.seen = Instant::now();
    }
    fn tick(&mut self) {
        if let Some(log) = &self.journal {
            if let Err(error) = log.lock().unwrap().maintain() {
                eprintln!("Log maintenance failed: {error}");
            }
        }
        let lease = Duration::from_secs(LEASE_SECONDS);
        let mut exposed = None;
        let mut ended = false;
        if let Some(running) = self.running.as_mut() {
            let expired = running.heartbeat.elapsed() > lease;
            let exited = !matches!(running.child.try_wait(), Ok(None));
            if expired || exited {
                // Only an exit nobody asked for leaves traffic exposed. An expired lease means
                // the app is gone: with nobody left to lift a block, none is raised.
                if exited && !expired && running.armed {
                    exposed = Some(std::mem::take(&mut running.bypass));
                }
                self.running.take();
                ended = true;
                let _ = fs::remove_file(self.root.join("runtime.json"));
            }
        }
        if ended {
            // The tunnel is gone, so the resolver it answered for goes back with it.
            let _ = self.dns.restore();
        } else if self.running.is_some() && self.dns_checked.elapsed() > RESOLVER_REFRESH {
            self.dns_checked = Instant::now();
            if let Err(error) = self.dns.refresh() {
                self.note(&format!("Resolver refresh failed: {error}"));
            }
        }
        if let Some(bypass) = exposed {
            self.block(&bypass);
        }
        if self.blocking
            && self.seen.elapsed() > lease
            && self.unblock_after.is_none_or(|at| Instant::now() >= at)
        {
            let _ = self.unblock();
        }
    }
    fn apply(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Status => {}
            Action::Start { config, session } => {
                if !valid_id(&session) {
                    return Err("native.tunSessionIdInvalid".into());
                }
                if let Some(current) = &self.running {
                    if current.session == session {
                        return Ok(());
                    }
                    return Err("native.tunSessionExists".into());
                }
                let config = safe_config(config)?;
                let path = self.root.join("runtime.json");
                secure_write(
                    &path,
                    &serde_json::to_vec(&config).map_err(|e| e.to_string())?,
                    0o600,
                )?;
                let mut check = platform::core_command(&self.root);
                check
                    .args(["check", "-c"])
                    .arg(&path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let mut child = check.spawn().map_err(|e| e.to_string())?;
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) if status.success() => break,
                        Ok(Some(_)) => return Err("native.tunConfigRejected".into()),
                        Err(e) => return Err(e.to_string()),
                        _ => {}
                    }
                    if Instant::now() > deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("native.tunConfigCheckTimeout".into());
                    }
                    thread::sleep(Duration::from_millis(30));
                }
                let log =
                    airport_tun_service::runtime_log::RuntimeLog::open(self.root.join("logs"))
                        .map_err(|e| e.to_string())?;
                log.lock().unwrap().drain();
                self.journal = Some(log.clone());
                let mut child = platform::core_command(&self.root)
                    .args(["run", "-c"])
                    .arg(&path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                if let Some(out) = child.stdout.take() {
                    airport_tun_service::runtime_log::capture(out, log.clone());
                }
                if let Some(err) = child.stderr.take() {
                    airport_tun_service::runtime_log::capture(err, log);
                }
                // Allow LAN: when the proxy inbound binds a non-loopback address, open that port in
                // the firewall for the local subnet so LAN devices can reach it (Windows blocks
                // inbound by default). Best effort — a refused rule only means no LAN access, the
                // tunnel itself is unaffected.
                let lan_port = config["inbounds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find_map(|inbound| {
                        let listen = inbound["listen"].as_str()?;
                        if matches!(listen, "127.0.0.1" | "::1" | "localhost") {
                            return None;
                        }
                        u16::try_from(inbound["listen_port"].as_u64()?).ok()
                    });
                if let Some(port) = lan_port {
                    let _ = platform::open_lan_port(port);
                }
                self.running = Some(Running {
                    child,
                    session,
                    heartbeat: Instant::now(),
                    armed: false,
                    bypass: airport_tun_service::route_exclusions(&config),
                    lan_port,
                });
                // The core has the device; the machine must now resolve through it. Failing here
                // is a failed connect, not a tunnel without DNS: the core is taken down first.
                if let Err(error) = self.dns.apply() {
                    self.running.take();
                    let _ = fs::remove_file(self.root.join("runtime.json"));
                    let _ = self.dns.restore();
                    return Err(msg(
                        "native.tunDnsOverrideFailed",
                        &[("error", text(error))],
                    ));
                }
                self.dns_checked = Instant::now();
            }
            Action::Heartbeat { session } => {
                let running = self.running.as_mut().ok_or("native.tunCoreStopped")?;
                if running.session != session {
                    return Err("native.tunSessionChanged".into());
                }
                running.heartbeat = Instant::now();
            }
            Action::Stop { session } => {
                if let Some(running) = &self.running {
                    if running.session != session {
                        return Err("native.tunSessionForeign".into());
                    }
                }
                // Stopping a core is deliberate: it raises no block. One already raised stays,
                // because a reconnect stops the session it replaces.
                self.running.take();
                let _ = self.dns.restore();
                let _ = fs::remove_file(self.root.join("runtime.json"));
            }
            Action::Arm { session } => {
                let running = self.running.as_mut().ok_or("native.tunCoreStopped")?;
                if running.session != session {
                    return Err("native.tunSessionChanged".into());
                }
                running.armed = true;
                // The tunnel is carrying traffic again: what was blocked now has somewhere to go.
                self.unblock()?;
            }
            Action::Release => {
                if let Some(running) = self.running.as_mut() {
                    running.armed = false;
                }
                self.unblock()?;
            }
        }
        Ok(())
    }
}
/// The service is going away with the tunnel it carried: the resolver goes back with it, even
/// on the exit paths that never reach `Stop`.
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.dns.restore();
    }
}
#[derive(Default)]
struct ReplayGuard {
    seen: HashMap<String, Instant>,
}
impl ReplayGuard {
    fn accept(&mut self, request: &Request) -> bool {
        self.seen
            .retain(|_, time| time.elapsed() < Duration::from_secs(65));
        if !valid_id(&request.id)
            || request.time.abs_diff(now()) > 30
            || self.seen.contains_key(&request.id)
            || self.seen.len() >= 4096
        {
            return false;
        }
        self.seen.insert(request.id.clone(), Instant::now());
        true
    }
}
/// Bounds the time to read one request as a whole, not each `read` call: a peer that
/// trickles a byte at a time must not be able to hold a connection open indefinitely.
struct Deadline<'a> {
    stream: &'a TcpStream,
    until: Instant,
}
impl Read for Deadline<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let left = self.until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left))?;
        let mut stream = self.stream;
        stream.read(buffer)
    }
}
fn read_request(stream: &TcpStream, key: &[u8; 32]) -> Result<Request> {
    // The listener is non-blocking, and on BSD-derived systems (macOS included) an accepted
    // socket inherits that: a read racing the client's write would fail instead of waiting.
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    let mut reader = Deadline {
        stream,
        until: Instant::now() + REQUEST_DEADLINE,
    };
    read_message::<Request>(&mut reader, key, b"airport/request/v1")
}
fn file_digest(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Some(format!("{:x}", hash.finalize()))
}
fn daemon() -> Result<()> {
    let root = platform::root();
    let credentials: Credentials = serde_json::from_slice(
        &fs::read(root.join("credentials.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|_| "Service credentials are corrupt")?;
    let listener = TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], PORT)))
        .map_err(|e| format!("Service port is unavailable: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    // Installed files are root-owned and replaced only by a repair, which restarts the service,
    // so the digests are those of the binaries in use for as long as this process lives.
    let core_sha256 = file_digest(&root.join(platform::core_name()));
    let helper_sha256 = std::env::current_exe()
        .ok()
        .and_then(|path| file_digest(&path));
    let mut engine = Engine {
        journal: Some(
            airport_tun_service::runtime_log::RuntimeLog::open(root.join("logs"))
                .map_err(|e| e.to_string())?,
        ),
        running: None,
        guard: platform::system_guard(&root),
        dns: Box::new(system_dns::Platform::new(&root)),
        dns_checked: Instant::now(),
        blocking: false,
        seen: Instant::now(),
        unblock_after: None,
        root,
    };
    // A service that was killed while blocking leaves the block behind. Whatever the reason
    // this process started, it starts with the network open.
    if let Err(error) = engine.guard.release() {
        engine.note(&format!("Kill switch cleanup failed: {error}"));
    }
    // The same holds for the resolver: a tunnel that died with the service cannot answer for
    // the resolver it took, so the machine gets the one it had back before anything else.
    if let Err(error) = engine.dns.restore() {
        engine.note(&format!("Resolver restore failed: {error}"));
    }
    let mut replay = ReplayGuard::default();
    // Requests are read off the main loop. The loop owns the lease: if reading a slow or
    // hostile connection could stall it, an expired session would never be stopped and the
    // app could no longer take the tunnel down.
    let (requests, incoming) = request_channel();
    let reading = Arc::new(AtomicUsize::new(0));
    while !STOP.load(Ordering::Relaxed) {
        engine.tick();
        let mut idle = true;
        for _ in 0..MAX_BATCH {
            match listener.accept() {
                Ok((stream, _)) => {
                    idle = false;
                    if reading.load(Ordering::Relaxed) >= MAX_PENDING_READS {
                        continue;
                    }
                    reading.fetch_add(1, Ordering::Relaxed);
                    let (requests, reading, key) =
                        (requests.clone(), reading.clone(), credentials.key);
                    thread::spawn(move || {
                        if let Ok(request) = read_request(&stream, &key) {
                            // Drop overflow instead of retaining unbounded authenticated frames/sockets.
                            let _ = requests.try_send((request, stream));
                        }
                        reading.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        for _ in 0..MAX_BATCH {
            let Ok((request, mut stream)) = incoming.try_recv() else {
                break;
            };
            idle = false;
            if !replay.accept(&request) {
                continue;
            }
            engine.touch();
            let fetch_logs = matches!(&request.action, Action::Heartbeat { .. });
            let error = engine.apply(request.action).err();
            engine.tick();
            let response = Response {
                id: request.id,
                protocol: PROTOCOL,
                running: engine.running.is_some(),
                session: engine.running.as_ref().map(|r| r.session.clone()),
                logs: if fetch_logs && error.is_none() {
                    engine
                        .journal
                        .as_ref()
                        .map(|log| log.lock().unwrap().drain())
                        .unwrap_or_default()
                } else {
                    vec![]
                },
                error,
                core_sha256: core_sha256.clone(),
                helper_sha256: helper_sha256.clone(),
                blocking: engine.blocking,
            };
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            let _ = write_message(
                &mut stream,
                &credentials.key,
                b"airport/response/v1",
                &response,
            );
        }
        if idle {
            thread::sleep(Duration::from_millis(40));
        }
    }
    // The service is stopping (shutdown, repair, uninstall): nothing would be left to lift a block.
    let _ = engine.unblock();
    drop(engine);
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::Mutex;

    /// Records what the kill switch was asked to do instead of touching the firewall.
    #[derive(Default)]
    struct Recorder {
        calls: Arc<Mutex<Vec<String>>>,
        refuse_release: Arc<AtomicBool>,
    }
    impl Guard for Recorder {
        fn engage(&mut self, bypass: &[String]) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("engage {}", bypass.join(",")));
            Ok(())
        }
        fn release(&mut self) -> Result<()> {
            if self.refuse_release.load(Ordering::Relaxed) {
                return Err("firewall busy".into());
            }
            self.calls.lock().unwrap().push("release".into());
            Ok(())
        }
    }
    fn engine(
        root: PathBuf,
        running: Option<Running>,
    ) -> (Engine, Arc<Mutex<Vec<String>>>, Arc<AtomicBool>) {
        let recorder = Recorder::default();
        let (calls, refuse) = (recorder.calls.clone(), recorder.refuse_release.clone());
        (
            Engine {
                journal: None,
                running,
                root,
                guard: Box::new(recorder),
                dns: Box::new(DnsRecorder::default()),
                dns_checked: Instant::now(),
                blocking: false,
                seen: Instant::now(),
                unblock_after: None,
            },
            calls,
            refuse,
        )
    }
    /// Records the resolver calls instead of touching the system settings.
    #[derive(Default)]
    struct DnsRecorder {
        calls: Arc<Mutex<Vec<String>>>,
        refuse_apply: Arc<AtomicBool>,
    }
    impl SystemDns for DnsRecorder {
        fn apply(&mut self) -> Result<()> {
            if self.refuse_apply.load(Ordering::Relaxed) {
                return Err("no network service".into());
            }
            self.calls.lock().unwrap().push("apply".into());
            Ok(())
        }
        fn refresh(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("refresh".into());
            Ok(())
        }
        fn restore(&mut self) -> Result<()> {
            self.calls.lock().unwrap().push("restore".into());
            Ok(())
        }
    }
    fn engine_dns(
        root: PathBuf,
        running: Option<Running>,
    ) -> (Engine, Arc<Mutex<Vec<String>>>, Arc<AtomicBool>) {
        let recorder = DnsRecorder::default();
        let (calls, refuse) = (recorder.calls.clone(), recorder.refuse_apply.clone());
        let mut engine = engine(root, running).0;
        engine.dns = Box::new(recorder);
        (engine, calls, refuse)
    }
    fn session(command: &str, name: &str, armed: bool) -> Running {
        Running {
            child: Command::new("sh").args(["-c", command]).spawn().unwrap(),
            session: name.repeat(32),
            heartbeat: Instant::now(),
            armed,
            bypass: vec!["192.168.0.0/16".into()],
            lan_port: None,
        }
    }
    fn wait_exit(engine: &mut Engine) {
        for _ in 0..200 {
            engine.tick();
            if engine.running.is_none() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("the core did not exit");
    }
    #[test]
    fn request_queue_bounds_memory_and_drops_overflow_sockets() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let (sender, receiver) = request_channel();
        let mut clients = Vec::new();
        for _ in 0..MAX_PENDING_READS {
            clients.push(TcpStream::connect(listener.local_addr().unwrap()).unwrap());
            let (stream, _) = listener.accept().unwrap();
            sender
                .try_send((
                    Request {
                        id: airport_tun_service::random_id().unwrap(),
                        time: now(),
                        action: Action::Status,
                    },
                    stream,
                ))
                .unwrap();
        }
        let mut overflow = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        overflow
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let (stream, _) = listener.accept().unwrap();
        let result = sender.try_send((
            Request {
                id: airport_tun_service::random_id().unwrap(),
                time: now(),
                action: Action::Status,
            },
            stream,
        ));
        assert!(matches!(&result, Err(mpsc::TrySendError::Full(_))));
        drop(result);
        let mut byte = [0];
        assert_eq!(overflow.read(&mut byte).unwrap(), 0);
        assert_eq!(receiver.try_iter().count(), MAX_PENDING_READS);
    }
    #[test]
    fn replay_guard_rejects_duplicates_and_expired_requests() {
        let mut guard = ReplayGuard::default();
        let request = Request {
            id: "a".repeat(32),
            time: now(),
            action: Action::Status,
        };
        assert!(guard.accept(&request));
        assert!(!guard.accept(&request));
        let stale = Request {
            id: "b".repeat(32),
            time: now() - 31,
            action: Action::Status,
        };
        assert!(!guard.accept(&stale));
        let future = Request {
            id: "c".repeat(32),
            time: now() + 31,
            action: Action::Status,
        };
        assert!(!guard.accept(&future));
    }
    // A peer trickling bytes must not hold a connection for longer than the request deadline,
    // and a well-formed request must still be read from a socket accepted off a non-blocking listener.
    #[test]
    fn request_reads_are_bounded_as_a_whole() {
        use std::io::Write;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let accept = |listener: &TcpListener| loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("{e}"),
            }
        };
        let key = [7u8; 32];
        // A slow writer: the 4-byte length arrives, then one byte every 100 ms, forever.
        let slow = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream.write_all(&1024u32.to_be_bytes()).unwrap();
            while stream.write_all(&[0]).is_ok() {
                thread::sleep(Duration::from_millis(100));
            }
        });
        let stream = accept(&listener);
        let started = Instant::now();
        assert!(read_request(&stream, &key).is_err());
        let took = started.elapsed();
        assert!(
            took >= REQUEST_DEADLINE - Duration::from_millis(200)
                && took < REQUEST_DEADLINE + Duration::from_secs(1),
            "{took:?}"
        );
        drop(stream);
        slow.join().unwrap();
        // A real request written a moment after connecting is waited for, not dropped.
        let writer = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            thread::sleep(Duration::from_millis(150));
            let request = Request {
                id: "a".repeat(32),
                time: now(),
                action: Action::Status,
            };
            write_message(&mut stream, &key, b"airport/request/v1", &request).unwrap();
            stream
        });
        let stream = accept(&listener);
        let request = read_request(&stream, &key).unwrap();
        assert_eq!(request.id, "a".repeat(32));
        drop(writer.join().unwrap());
    }
    #[test]
    fn start_is_idempotent_and_conflicting_start_is_rejected() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "airport-service-test-{}",
            airport_tun_service::random_id().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        let binary = root.join(platform::core_name());
        fs::write(
            &binary,
            b"#!/bin/sh\nif [ \"$1\" = check ]; then exit 0; fi\necho core-test-output\nexec /bin/sleep 30\n",
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let (mut engine, _, _) = engine(root.clone(), None);
        let config = serde_json::json!({"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":2080},{"type":"tun","address":[airport_tun_service::TUN_PREFIX_V4,airport_tun_service::TUN_PREFIX_V6],"auto_route":true,"dns_mode":"hijack","route_exclude_address":["10.0.0.0/8"]}],"outbounds":[{"type":"direct","tag":"direct"}],"experimental":{"clash_api":{"external_controller":"127.0.0.1:29999","secret":"a".repeat(64)}}});
        engine
            .apply(Action::Start {
                config: config.clone(),
                session: "a".repeat(32),
            })
            .unwrap();
        let log = engine.journal.as_ref().unwrap();
        for _ in 0..100 {
            if log
                .lock()
                .unwrap()
                .recent()
                .iter()
                .any(|line| line.contains("core-test-output"))
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(log
            .lock()
            .unwrap()
            .recent()
            .iter()
            .any(|line| line.contains("core-test-output")));
        let pid = engine.running.as_ref().unwrap().child.id();
        // A started core is not armed until the app says the tunnel works, and it remembers
        // which destinations the tunnel leaves alone.
        assert!(!engine.running.as_ref().unwrap().armed);
        assert_eq!(engine.running.as_ref().unwrap().bypass, ["10.0.0.0/8"]);
        engine
            .apply(Action::Start {
                config: config.clone(),
                session: "a".repeat(32),
            })
            .unwrap();
        assert_eq!(engine.running.as_ref().unwrap().child.id(), pid);
        assert!(engine
            .apply(Action::Start {
                config,
                session: "b".repeat(32)
            })
            .is_err());
        engine
            .apply(Action::Stop {
                session: "a".repeat(32),
            })
            .unwrap();
        assert!(!root.join("runtime.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_stop_from_another_session() {
        let (mut e, _, _) = engine(
            std::env::temp_dir().join("airport-no-runtime"),
            Some(session("exec sleep 30", "a", false)),
        );
        assert!(e
            .apply(Action::Stop {
                session: "b".repeat(32)
            })
            .is_err());
        assert!(e.running.is_some());
        e.apply(Action::Stop {
            session: "a".repeat(32),
        })
        .unwrap();
        assert!(e.running.is_none());
    }
    #[test]
    fn expired_lease_terminates_child() {
        let mut stale = session("exec sleep 30", "a", true);
        stale.heartbeat = Instant::now() - Duration::from_secs(LEASE_SECONDS + 1);
        let (mut e, calls, _) =
            engine(std::env::temp_dir().join("airport-no-runtime"), Some(stale));
        e.tick();
        assert!(e.running.is_none());
        // The app is gone: nobody could lift a block, so none is raised even though it was armed.
        assert!(!e.blocking && calls.lock().unwrap().is_empty());
    }
    fn root() -> PathBuf {
        std::env::temp_dir().join("airport-no-runtime")
    }
    // The kill switch: an armed core that exits on its own blocks the network; arming the
    // next working tunnel or an explicit release lifts the block; nothing else does.
    #[test]
    fn unexpected_exit_of_an_armed_core_blocks_until_armed_again() {
        let (mut e, calls, _) = engine(root(), Some(session("exit 3", "a", true)));
        wait_exit(&mut e);
        assert!(e.blocking);
        assert_eq!(*calls.lock().unwrap(), ["engage 192.168.0.0/16"]);
        // Reconnecting: the old session is stopped, a new core starts. Neither lifts the block.
        e.apply(Action::Stop {
            session: "a".repeat(32),
        })
        .unwrap();
        e.running = Some(session("exec sleep 30", "b", false));
        e.apply(Action::Heartbeat {
            session: "b".repeat(32),
        })
        .unwrap();
        e.tick();
        assert!(e.blocking);
        // Arming needs the session that is actually running.
        assert!(e
            .apply(Action::Arm {
                session: "c".repeat(32)
            })
            .is_err());
        assert!(e.blocking);
        // The app verified the new tunnel: traffic flows again and the new session is protected.
        e.apply(Action::Arm {
            session: "b".repeat(32),
        })
        .unwrap();
        assert!(!e.blocking && e.running.as_ref().unwrap().armed);
        assert_eq!(*calls.lock().unwrap(), ["engage 192.168.0.0/16", "release"]);
    }
    #[test]
    fn deliberate_stops_and_unarmed_exits_do_not_block() {
        // Not armed: the user did not ask for the kill switch, or the tunnel never came up.
        let (mut e, calls, _) = engine(root(), Some(session("exit 1", "a", false)));
        wait_exit(&mut e);
        assert!(!e.blocking && calls.lock().unwrap().is_empty());
        // Armed, but stopped on request.
        let (mut e, calls, _) = engine(root(), Some(session("exec sleep 30", "a", true)));
        e.apply(Action::Stop {
            session: "a".repeat(32),
        })
        .unwrap();
        e.tick();
        assert!(!e.blocking && calls.lock().unwrap().is_empty());
        // Arming without a running core is refused rather than silently ignored.
        assert!(e
            .apply(Action::Arm {
                session: "a".repeat(32)
            })
            .is_err());
    }
    #[test]
    fn release_lifts_the_block_and_disarms() {
        let (mut e, calls, _) = engine(root(), Some(session("exit 0", "a", true)));
        wait_exit(&mut e);
        assert!(e.blocking);
        e.apply(Action::Release).unwrap();
        assert!(!e.blocking);
        // Releasing with a live armed session disarms it: its later exit no longer blocks.
        let (mut e, calls2, _) = engine(root(), Some(session("exec sleep 30", "a", true)));
        e.apply(Action::Release).unwrap();
        assert!(!e.running.as_ref().unwrap().armed);
        e.running.as_mut().unwrap().child.kill().unwrap();
        wait_exit(&mut e);
        assert!(!e.blocking && calls2.lock().unwrap().is_empty());
        assert_eq!(*calls.lock().unwrap(), ["engage 192.168.0.0/16", "release"]);
    }
    // A block is only held for an app that is there to lift it.
    #[test]
    fn block_is_lifted_when_the_app_stops_answering() {
        let (mut e, calls, refuse) = engine(root(), Some(session("exit 0", "a", true)));
        wait_exit(&mut e);
        assert!(e.blocking);
        // The app keeps asking for status: the block stays.
        e.touch();
        e.tick();
        assert!(e.blocking);
        // Silence for longer than the lease.
        e.seen = Instant::now() - Duration::from_secs(LEASE_SECONDS + 1);
        // A firewall that refuses is retried, not given up on, and not hammered.
        refuse.store(true, Ordering::Relaxed);
        e.tick();
        e.tick();
        assert!(e.blocking && e.unblock_after.is_some());
        refuse.store(false, Ordering::Relaxed);
        e.tick();
        assert!(e.blocking, "retried before the retry delay");
        e.unblock_after = Some(Instant::now());
        e.tick();
        assert!(!e.blocking);
        assert_eq!(*calls.lock().unwrap(), ["engage 192.168.0.0/16", "release"]);
    }
    #[test]
    fn firewall_rules_allow_only_the_tunnel_the_core_and_the_exclusions() {
        let bypass = vec![
            "192.168.0.0/16".to_owned(),
            "fc00::/7".to_owned(),
            "not a prefix; block drop".to_owned(),
        ];
        let pf = platform::pf_rules(&bypass);
        let lines: Vec<&str> = pf.lines().collect();
        // The block is the last rule and every rule before it is a `quick` pass.
        assert_eq!(*lines.last().unwrap(), "block drop out quick all");
        assert!(lines[..lines.len() - 1]
            .iter()
            .all(|line| line.starts_with("pass out quick ")));
        for expected in [
            "pass out quick on lo0 all",
            "pass out quick inet from 172.19.0.1 to any",
            "pass out quick inet6 from fdfe:dcba:9876::1 to any",
            "pass out quick proto tcp all group 47653",
            "pass out quick proto udp all group 47653",
            "pass out quick inet from any to 192.168.0.0/16",
            "pass out quick inet6 from any to fc00::/7",
        ] {
            assert!(lines.contains(&expected), "missing: {expected}\n{pf}");
        }
        // Only validated prefixes ever reach a rule file.
        assert!(!pf.contains("not a prefix"));
        let nft = platform::nft_rules(&bypass);
        assert!(nft.starts_with("table inet airport_tun_guard\ndelete table inet airport_tun_guard\ntable inet airport_tun_guard {"));
        for expected in [
            "type filter hook output priority 0; policy drop;",
            "oifname \"lo\" accept",
            "ip saddr 172.19.0.1 accept",
            "ip6 saddr fdfe:dcba:9876::1 accept",
            "meta skgid 47653 accept",
            "ip daddr { 192.168.0.0/16 } accept",
            "ip6 daddr { fc00::/7 } accept",
        ] {
            assert!(nft.contains(expected), "missing: {expected}\n{nft}");
        }
        assert!(!nft.contains("not a prefix"));
        // Without exclusions there are no empty sets, which nft would reject.
        assert!(!platform::nft_rules(&[]).contains("daddr"));
    }
    /// A tunnel the machine cannot resolve through is not a tunnel: the connect fails and the
    /// core is taken down, so nobody browses through a device their lookups never reach.
    #[test]
    fn a_resolver_that_cannot_be_taken_fails_the_connect() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "airport-service-dns-{}",
            airport_tun_service::random_id().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        let binary = root.join(platform::core_name());
        fs::write(
            &binary,
            b"#!/bin/sh\nif [ \"$1\" = check ]; then exit 0; fi\nexec /bin/sleep 30\n",
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let (mut engine, calls, refuse) = engine_dns(root.clone(), None);
        refuse.store(true, Ordering::Relaxed);
        let error = engine
            .apply(Action::Start {
                config: start_config(),
                session: "a".repeat(32),
            })
            .unwrap_err();
        assert!(error.contains("native.tunDnsOverrideFailed"), "{error}");
        assert!(
            engine.running.is_none(),
            "the core must not outlive a failed override"
        );
        assert_eq!(*calls.lock().unwrap(), ["restore"]);
        let _ = fs::remove_dir_all(&root);
    }
    /// However the tunnel ends — a stop the app asked for, or a core that died — the machine
    /// gets the resolver it had back.
    #[test]
    fn the_resolver_goes_back_when_the_tunnel_does() {
        let (mut engine, calls, _) = engine_dns(root(), Some(session("exec sleep 30", "a", false)));
        engine
            .apply(Action::Stop {
                session: "a".repeat(32),
            })
            .unwrap();
        assert_eq!(*calls.lock().unwrap(), ["restore"]);
        let (mut engine, calls, _) = engine_dns(root(), Some(session("exit 0", "a", false)));
        wait_exit(&mut engine);
        assert!(engine.running.is_none());
        assert_eq!(*calls.lock().unwrap(), ["restore"]);
        // While the tunnel runs, nothing is put back: only a periodic re-apply may happen.
        let (mut engine, calls, _) = engine_dns(root(), Some(session("exec sleep 30", "a", false)));
        engine.dns_checked = Instant::now() - RESOLVER_REFRESH - Duration::from_secs(1);
        engine.tick();
        assert_eq!(*calls.lock().unwrap(), ["refresh"]);
    }
    fn start_config() -> serde_json::Value {
        serde_json::json!({"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":2080},{"type":"tun","address":[airport_tun_service::TUN_PREFIX_V4,airport_tun_service::TUN_PREFIX_V6],"auto_route":true,"dns_mode":"hijack"}],"outbounds":[{"type":"direct","tag":"direct"}],"experimental":{"clash_api":{"external_controller":"127.0.0.1:29999","secret":"a".repeat(64)}}})
    }
}
