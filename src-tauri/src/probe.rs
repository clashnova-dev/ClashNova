use crate::message::{msg, text};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    error::Error as StdError,
    io::Write,
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

static SESSION: Mutex<Option<Arc<Session>>> = Mutex::new(None);
static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static STOPPING: AtomicBool = AtomicBool::new(false);
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
    if raw.len() > 2 * 1024 * 1024 {
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
    if ACTIVE.fetch_add(1, Ordering::SeqCst) >= 12 {
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
