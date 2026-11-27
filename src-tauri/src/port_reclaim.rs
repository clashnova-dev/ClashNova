//! Reclaims leftover cores automatically; foreign listeners require a short-lived, one-shot confirmation.
//! Only the configured proxy port is eligible. Recheck process identity before every signal.

use crate::{connection::Phase, message::msg, Core};
use serde::Serialize;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::State;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Listener {
    pid: u32,
    name: String,
    #[serde(skip)]
    generation: String,
}

impl Listener {
    fn new(pid: u32, name: String) -> Self {
        Self {
            pid,
            name,
            generation: process_generation(pid).unwrap_or_default(),
        }
    }
    fn eligible(&self) -> bool {
        self.pid > 4
            && self.pid != std::process::id()
            && !self.generation.is_empty()
            && !protected_name(&self.name)
    }
}

fn protected_name(name: &str) -> bool {
    let name = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    matches!(
        name.trim_end_matches(".exe"),
        "clashnova"
            | "airport-client"
            | "airport-tun-service"
            | "system"
            | "system idle process"
            | "launchd"
            | "systemd"
            | "kernel_task"
            | "windowserver"
            | "loginwindow"
            | "wininit"
            | "winlogon"
            | "csrss"
            | "lsass"
            | "svchost"
    )
}

#[derive(Serialize)]
pub struct Conflict {
    port: u16,
    token: String,
    listeners: Vec<Listener>,
}
struct Pending {
    port: u16,
    address: String,
    token: String,
    created: Instant,
    listeners: Vec<Listener>,
}
#[derive(Default)]
pub struct Reclaimer(Mutex<Option<Pending>>);

fn available(port: u16, address: &str) -> bool {
    std::net::TcpListener::bind((address, port)).is_ok()
}
fn port_error(key: &str, port: u16) -> String {
    msg(key, &[("port", port.into())])
}

impl Reclaimer {
    fn inspect(&self, port: u16, address: &str) -> Result<Option<Conflict>, String> {
        let mut pending = self.0.lock().map_err(|_| "native.connectionBusy")?;
        *pending = None;
        if port < 1024 {
            return Err("settings.portInvalid".into());
        }
        if available(port, address) {
            return Ok(None);
        }
        let foreign: Vec<_> = listeners(port)
            .into_iter()
            .filter(|p| !is_own_core(&p.name))
            .collect();
        if foreign.is_empty() {
            // Existing own cores are handled by the automatic startup reclaim.
            if listeners(port).iter().any(|p| is_own_core(&p.name)) {
                return Ok(None);
            }
            return Err(port_error("native.portOwnerUnknown", port));
        }
        if foreign.iter().any(|p| !p.eligible()) {
            return Err(port_error("native.portOwnerProtected", port));
        }
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| "native.trafficSecretFailed")?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        *pending = Some(Pending {
            port,
            address: address.into(),
            token: token.clone(),
            created: Instant::now(),
            listeners: foreign.clone(),
        });
        Ok(Some(Conflict {
            port,
            token,
            listeners: foreign,
        }))
    }

    fn release(&self, port: u16, address: &str, token: &str) -> Result<Vec<u32>, String> {
        let pending = self
            .0
            .lock()
            .map_err(|_| "native.connectionBusy")?
            .take()
            .ok_or("native.portConflictChanged")?;
        if pending.port != port
            || pending.address != address
            || pending.token != token
            || pending.created.elapsed() > Duration::from_secs(120)
        {
            return Err("native.portConflictChanged".into());
        }
        // A new/reused PID or executable is a new decision, never implicit consent.
        if listeners(port)
            .iter()
            .filter(|p| !is_own_core(&p.name))
            .any(|p| !pending.listeners.contains(p))
        {
            return Err("native.portConflictChanged".into());
        }
        let mut stopped = stop_targets(port, address, &pending.listeners);
        stopped.extend(reclaim(port, address));
        if !available(port, address) {
            return Err(port_error("native.portReleaseFailed", port));
        }
        Ok(stopped)
    }
}

// Shared precondition for the port-reclaim commands: the core must be idle (not mid-connect) and a
// subscription must be imported before touching the proxy port.
fn idle_core_guard(core: &Core) -> Result<(), String> {
    if !matches!(core.phase, Phase::Idle | Phase::Failed) {
        return Err("native.connectionBusy".into());
    }
    if core.config_path.is_none() {
        return Err("native.importSubscriptionFirst".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn inspect_proxy_port(
    core: State<'_, Arc<Mutex<Core>>>,
    reclaimer: State<'_, Arc<Reclaimer>>,
) -> Result<Option<Conflict>, String> {
    let core = core.inner().clone();
    let reclaimer = reclaimer.inner().clone();
    crate::blocking(move || {
        let core = core.lock().map_err(|_| "native.connectionBusy")?;
        idle_core_guard(&core)?;
        let conflict = reclaimer.inspect(core.network.proxy_port, core.network.listen_address())?;
        if conflict.is_some() {
            if let Some(detail) = crate::proxy_conflict::detail(core.network.proxy_port) {
                return Err(msg(
                    "native.otherProxyInUse",
                    &[("detail", crate::message::text(detail))],
                ));
            }
        }
        Ok(conflict)
    })
    .await
}

#[tauri::command]
pub async fn release_proxy_port(
    core: State<'_, Arc<Mutex<Core>>>,
    reclaimer: State<'_, Arc<Reclaimer>>,
    token: String,
) -> Result<(), String> {
    let core = core.inner().clone();
    let reclaimer = reclaimer.inner().clone();
    crate::blocking(move || {
        let mut core = core.lock().map_err(|_| "native.connectionBusy")?;
        idle_core_guard(&core)?;
        let stopped = reclaimer.release(
            core.network.proxy_port,
            core.network.listen_address(),
            &token,
        )?;
        if !stopped.is_empty() {
            let port = core.network.proxy_port;
            core.push_log(port_error("log.proxyPortReclaimed", port));
        }
        Ok(())
    })
    .await
}

pub(crate) fn is_own_core(name: &str) -> bool {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let name = name.strip_suffix(" (deleted)").unwrap_or(name);
    let name = name
        .get(..name.len().saturating_sub(4))
        .filter(|_| name.as_bytes().ends_with_ignore_ascii_case(b".exe"))
        .unwrap_or(name);
    name.eq_ignore_ascii_case("airport-core")
        || name.to_ascii_lowercase().starts_with("airport-core-")
}

trait AsciiCase {
    fn ends_with_ignore_ascii_case(&self, suffix: &[u8]) -> bool;
}
impl AsciiCase for [u8] {
    fn ends_with_ignore_ascii_case(&self, suffix: &[u8]) -> bool {
        self.len() >= suffix.len() && self[self.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
    }
}

/// Stops leftover proxy cores listening on `port`. Returns the process ids that were signaled.
pub fn reclaim(port: u16, address: &str) -> Vec<u32> {
    if port < 1024 || available(port, address) {
        return Vec::new();
    }
    let own: Vec<_> = listeners(port)
        .into_iter()
        .filter(|p| p.eligible() && is_own_core(&p.name))
        .collect();
    stop_targets(port, address, &own)
}

fn stop_targets(port: u16, address: &str, targets: &[Listener]) -> Vec<u32> {
    if targets.is_empty() {
        return Vec::new();
    }
    let mut signaled = Vec::new();
    let deadline = Instant::now() + Duration::from_millis(1200);
    loop {
        let current = listeners(port);
        for target in targets.iter().filter(|p| p.eligible()) {
            if !signaled.contains(&target.pid) && current.contains(target) {
                signal(target.pid, "-TERM");
                signaled.push(target.pid);
            }
        }
        // A process snapshot can briefly omit an exiting owner before its
        // socket is reusable. Only an actual bind proves that startup can proceed.
        if available(port, address) || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(40));
    }
    for target in targets.iter().filter(|p| p.eligible()) {
        if signaled.contains(&target.pid) && listeners(port).contains(target) {
            signal(target.pid, "-KILL");
        }
    }
    wait_available(port, address, Duration::from_millis(800));
    let remaining = listeners(port);
    signaled.retain(|pid| {
        targets
            .iter()
            .filter(|p| p.pid == *pid)
            .all(|p| !remaining.contains(p))
    });
    signaled
}

fn wait_available(port: u16, address: &str, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if available(port, address) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(40));
    }
}

#[cfg(target_os = "linux")]
fn process_generation(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    linux_generation(&stat)
}
#[cfg(any(test, target_os = "linux"))]
fn linux_generation(stat: &str) -> Option<String> {
    let start = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?;
    start.parse::<u64>().ok()?;
    Some(start.to_owned())
}
#[cfg(target_os = "macos")]
fn process_generation(pid: u32) -> Option<String> {
    let output = Command::new("/bin/ps")
        .env("LC_ALL", "C")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if output.status.success() && !text.is_empty() {
        Some(text)
    } else {
        None
    }
}
#[cfg(windows)]
fn process_generation(pid: u32) -> Option<String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0;
        CloseHandle(handle);
        ok.then(|| format!("{}:{}", created.dwHighDateTime, created.dwLowDateTime))
    }
}

fn signal(pid: u32, sig: &str) -> bool {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        crate::hide_console(&mut cmd).args(["/PID", &pid.to_string()]);
        if sig == "-KILL" {
            cmd.arg("/F");
        }
        cmd.stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("/bin/kill")
            .args([sig, &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

fn listeners(port: u16) -> Vec<Listener> {
    #[cfg(target_os = "linux")]
    {
        linux_listeners(port)
    }
    #[cfg(target_os = "macos")]
    {
        unix_lsof_listeners(port)
    }
    #[cfg(windows)]
    {
        windows_listeners(port)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = port;
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
fn unix_lsof_listeners(port: u16) -> Vec<Listener> {
    let output = command(
        &["/usr/sbin/lsof", "/usr/bin/lsof", "lsof"],
        &[
            "-nP".into(),
            format!("-iTCP:{port}"),
            "-sTCP:LISTEN".into(),
            "-t".into(),
        ],
    );
    let Some(output) = output else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut found = Vec::new();
    for pid in parse_pid_lines(&text) {
        if let Some(name) = macos_process_name(pid) {
            found.push(Listener::new(pid, name));
        }
    }
    found
}

#[cfg(target_os = "macos")]
fn macos_process_name(pid: u32) -> Option<String> {
    let output = Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-ww", "-o", "comm="])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let executable = text.trim();
    Path::new(executable)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

#[cfg(target_os = "linux")]
fn linux_listeners(port: u16) -> Vec<Listener> {
    let mut inodes = Vec::new();
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(text) = std::fs::read_to_string(path) {
            inodes.extend(tcp_listen_inodes(&text, port));
        }
    }
    if inodes.is_empty() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        let holds = fds.flatten().any(|fd| {
            std::fs::read_link(fd.path())
                .ok()
                .and_then(|target| socket_inode(&target.to_string_lossy()))
                .is_some_and(|inode| inodes.contains(&inode))
        });
        if !holds {
            continue;
        }
        if let Some(name) = linux_process_name(pid) {
            found.push(Listener::new(pid, name));
        }
    }
    found
}

#[cfg(target_os = "linux")]
fn linux_process_name(pid: u32) -> Option<String> {
    let path = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

#[cfg(windows)]
fn windows_listeners(port: u16) -> Vec<Listener> {
    let mut cmd = Command::new("netstat");
    let Some(output) = crate::hide_console(&mut cmd)
        .args(["-ano", "-p", "TCP"])
        .output()
        .ok()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut found = Vec::new();
    for pid in netstat_listen_pids(&text, port) {
        if let Some(name) = windows_image_name(pid) {
            found.push(Listener::new(pid, name));
        }
    }
    found
}

#[cfg(windows)]
fn windows_image_name(pid: u32) -> Option<String> {
    let mut cmd = Command::new("tasklist");
    let output = crate::hide_console(&mut cmd)
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    parse_tasklist_image(text.lines().next()?)
}

#[cfg(target_os = "macos")]
fn command(bins: &[&str], args: &[String]) -> Option<std::process::Output> {
    bins.iter().find_map(|bin| {
        Command::new(bin)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()
    })
}

#[cfg(any(test, target_os = "macos"))]
fn parse_pid_lines(text: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    for line in text.lines() {
        if let Ok(pid) = line.trim().parse::<u32>() {
            if pid > 1 {
                pids.push(pid);
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

#[cfg(any(test, target_os = "linux"))]
fn tcp_listen_inodes(text: &str, port: u16) -> Vec<u64> {
    let mut inodes = Vec::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 10 || cols[3] != "0A" {
            continue;
        }
        let Some((_, port_hex)) = cols[1].rsplit_once(':') else {
            continue;
        };
        let Ok(local_port) = u16::from_str_radix(port_hex, 16) else {
            continue;
        };
        if local_port != port {
            continue;
        }
        if let Ok(inode) = cols[9].parse::<u64>() {
            if inode > 0 {
                inodes.push(inode);
            }
        }
    }
    inodes
}

#[cfg(any(test, target_os = "linux"))]
fn socket_inode(target: &str) -> Option<u64> {
    target
        .strip_prefix("socket:[")
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|inode| inode.parse().ok())
}

#[cfg(any(test, windows))]
fn netstat_listen_pids(text: &str, port: u16) -> Vec<u32> {
    let mut pids = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 || !parts[3].eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let Some(local_port) = parts[1]
            .rsplit(':')
            .next()
            .and_then(|value| value.parse::<u16>().ok())
        else {
            continue;
        };
        if local_port != port {
            continue;
        }
        if let Ok(pid) = parts[4].parse::<u32>() {
            if pid > 1 {
                pids.push(pid);
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

#[cfg(any(test, windows))]
fn parse_tasklist_image(line: &str) -> Option<String> {
    let line = line.trim();
    if !line.starts_with('"') {
        return None;
    }
    let end = line[1..].find('"')?;
    Some(line[1..1 + end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn recognises_only_this_apps_proxy_core() {
        assert!(is_own_core("airport-core"));
        assert!(is_own_core("airport-core.exe"));
        assert!(is_own_core("airport-core-aarch64-apple-darwin"));
        assert!(is_own_core(
            "/Applications/ClashNova.app/Contents/MacOS/airport-core"
        ));
        assert!(is_own_core("airport-core (deleted)"));
        assert!(!is_own_core("airport-client"));
        assert!(!is_own_core("airport-tun-service"));
        assert!(!is_own_core("chrome"));
        assert!(!is_own_core(""));
    }

    #[test]
    fn parses_listen_tables() {
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:0820 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 18434 1 0000000000000000 100 0 0 10 0\n\
   1: 0100007F:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 99 1 0000000000000000 100 0 0 10 0\n";
        assert_eq!(tcp_listen_inodes(tcp, 2080), vec![18434]);
        let netstat = "  TCP    127.0.0.1:2080    0.0.0.0:0    LISTENING    4321\n\
  TCP    0.0.0.0:80    0.0.0.0:0    LISTENING    4\n\
  TCP    [::]:2080    [::]:0    LISTENING    4321\n";
        assert_eq!(netstat_listen_pids(netstat, 2080), vec![4321]);
        assert_eq!(parse_pid_lines("12\n\n34\n12\n"), vec![12, 34]);
        assert_eq!(
            parse_tasklist_image(r#""airport-core.exe","4321","Console","1","1,000 K""#).as_deref(),
            Some("airport-core.exe")
        );
        assert_eq!(socket_inode("socket:[18434]"), Some(18434));
        let fields = (4..=52)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            linux_generation(&format!("55 (name with (spaces)) S {fields}")),
            Some("22".into())
        );
        assert_eq!(linux_generation("55 (incomplete) S 1 2"), None);
    }

    #[test]
    fn a_free_port_stays_available_after_the_fast_path() {
        let listener = TcpListener::bind("0.0.0.0:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(reclaim(port, "127.0.0.1").is_empty());
        assert!(TcpListener::bind(("0.0.0.0", port)).is_ok());
    }

    #[test]
    fn leaves_an_unrelated_listener_in_place() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(reclaim(port, "127.0.0.1").is_empty());
        assert!(TcpListener::bind(("127.0.0.1", port)).is_err());
    }

    #[test]
    fn waits_for_delayed_socket_release_without_terminating_an_owner() {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            drop(socket);
        });
        assert!(wait_available(port, "127.0.0.1", Duration::from_secs(2)));
        release.join().unwrap();
    }

    #[test]
    fn occupied_socket_wait_is_bounded_and_does_not_terminate_its_owner() {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        assert!(!wait_available(
            port,
            "127.0.0.1",
            Duration::from_millis(60)
        ));
        assert!(!available(port, "127.0.0.1"));
        drop(socket);
    }

    #[test]
    #[ignore = "test-only child listener; launched explicitly by the parent test"]
    fn port_listener_fixture() {
        let ready = std::env::var_os("AIRPORT_PORT_FIXTURE_READY").expect("fixture only");
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        std::fs::write(ready, socket.local_addr().unwrap().port().to_string()).unwrap();
        thread::sleep(Duration::from_secs(120));
        drop(socket);
    }

    struct ChildListener {
        child: std::process::Child,
        _directory: tempfile::TempDir,
        port: u16,
    }
    impl Drop for ChildListener {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    fn child_listener(ignore_term: bool) -> ChildListener {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("ready");
        let executable = std::env::current_exe().unwrap();
        #[cfg(unix)]
        let mut command = if ignore_term {
            let mut cmd = Command::new("/bin/sh");
            cmd.args(["-c", "trap '' TERM; exec \"$@\"", "port-fixture"])
                .arg(&executable);
            cmd
        } else {
            Command::new(&executable)
        };
        #[cfg(windows)]
        let mut command = {
            let _ = ignore_term;
            Command::new(&executable)
        };
        let child = command
            .args([
                "--exact",
                "port_reclaim::tests::port_listener_fixture",
                "--ignored",
            ])
            .env("AIRPORT_PORT_FIXTURE_READY", &ready)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut fixture = ChildListener {
            child,
            _directory: dir,
            port: 0,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(port) = std::fs::read_to_string(&ready)
                .ok()
                .and_then(|s| s.parse().ok())
            {
                fixture.port = port;
                return fixture;
            }
            assert!(
                fixture.child.try_wait().unwrap().is_none(),
                "fixture exited before listening"
            );
            thread::sleep(Duration::from_millis(20));
        }
        panic!("fixture listener did not start");
    }

    #[test]
    fn confirmed_foreign_listener_is_stopped_and_port_becomes_available() {
        let mut child = child_listener(false);
        let reclaimer = Reclaimer::default();
        let conflict = reclaimer.inspect(child.port, "127.0.0.1").unwrap().unwrap();
        assert!(conflict.listeners.iter().any(|p| p.pid == child.child.id()));
        assert!(
            child.child.try_wait().unwrap().is_none(),
            "inspection must not terminate anything"
        );
        let stopped = reclaimer
            .release(child.port, "127.0.0.1", &conflict.token)
            .unwrap_or_else(|error| {
                panic!(
                    "{error}; child={:?}; owners={:?}",
                    child.child.try_wait(),
                    listeners(child.port)
                )
            });
        assert!(stopped.contains(&child.child.id()));
        assert!(available(child.port, "127.0.0.1"));
        assert!(
            reclaimer
                .release(child.port, "127.0.0.1", &conflict.token)
                .is_err(),
            "tokens are one-shot"
        );
    }

    #[test]
    fn changed_expired_or_unapproved_requests_leave_the_listener_running() {
        let mut child = child_listener(false);
        let reclaimer = Reclaimer::default();
        let c = reclaimer.inspect(child.port, "127.0.0.1").unwrap().unwrap();
        assert!(reclaimer
            .release(child.port, "127.0.0.1", "wrong-token")
            .is_err());
        assert!(reclaimer
            .release(child.port, "127.0.0.1", &c.token)
            .is_err());
        let c = reclaimer.inspect(child.port, "127.0.0.1").unwrap().unwrap();
        assert!(reclaimer
            .release(child.port - 1, "127.0.0.1", &c.token)
            .is_err());
        let c = reclaimer.inspect(child.port, "127.0.0.1").unwrap().unwrap();
        reclaimer.0.lock().unwrap().as_mut().unwrap().created =
            Instant::now() - Duration::from_secs(121);
        assert!(reclaimer
            .release(child.port, "127.0.0.1", &c.token)
            .is_err());
        let c = reclaimer.inspect(child.port, "127.0.0.1").unwrap().unwrap();
        reclaimer.0.lock().unwrap().as_mut().unwrap().listeners[0]
            .generation
            .push_str("changed");
        assert!(reclaimer
            .release(child.port, "127.0.0.1", &c.token)
            .is_err());
        assert!(child.child.try_wait().unwrap().is_none());
        assert!(!available(child.port, "127.0.0.1"));
    }

    #[test]
    fn protects_this_process_and_system_services() {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(Reclaimer::default()
            .inspect(socket.local_addr().unwrap().port(), "127.0.0.1")
            .is_err());
        for name in [
            "launchd",
            "systemd",
            "ClashNova.exe",
            "airport-tun-service",
            "svchost.exe",
        ] {
            assert!(protected_name(name));
        }
        assert!(!protected_name("example-server.exe"));
    }

    #[cfg(unix)]
    #[test]
    fn escalates_only_a_confirmed_listener_that_ignores_normal_termination() {
        use std::os::unix::process::ExitStatusExt;
        let mut child = child_listener(true);
        signal(child.child.id(), "-TERM");
        thread::sleep(Duration::from_millis(100));
        assert!(
            child.child.try_wait().unwrap().is_none(),
            "fixture must ignore TERM"
        );
        let reclaimer = Reclaimer::default();
        let conflict = reclaimer.inspect(child.port, "127.0.0.1").unwrap().unwrap();
        reclaimer
            .release(child.port, "127.0.0.1", &conflict.token)
            .unwrap();
        assert!(available(child.port, "127.0.0.1"));
        assert_eq!(child.child.wait().unwrap().signal(), Some(9));
    }
}
