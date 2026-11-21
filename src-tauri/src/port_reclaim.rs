//! Frees this client's proxy port from a core left behind by a previous connection.
//! Only a process whose executable is this app's own proxy core is stopped.
//! Anything else listening on the port is left alone, and the usual port error follows.

#[cfg(target_os = "macos")]
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct Listener {
    pid: u32,
    name: String,
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
pub fn reclaim(port: u16) -> Vec<u32> {
    if port < 1024 {
        return Vec::new();
    }
    // The normal path has no listener. A bind is cheaper than lsof/netstat/PowerShell.
    // Use loopback so this check cannot expose a LAN listener or prompt the firewall.
    // Still inspect occupied ports, and let the actual core bind recheck any later race.
    if let Ok(free) = std::net::TcpListener::bind(("127.0.0.1", port)) {
        drop(free);
        return Vec::new();
    }
    let own: Vec<u32> = listeners(port)
        .into_iter()
        .filter(|listener| {
            listener.pid > 1 && listener.pid != std::process::id() && is_own_core(&listener.name)
        })
        .map(|listener| listener.pid)
        .collect();
    if own.is_empty() {
        return own;
    }
    for pid in &own {
        signal(*pid, "-TERM");
    }
    wait_until_gone(port, &own, Duration::from_millis(1200));
    for pid in listeners(port)
        .into_iter()
        .filter(|listener| own.contains(&listener.pid))
        .map(|listener| listener.pid)
    {
        signal(pid, "-KILL");
    }
    wait_until_gone(port, &own, Duration::from_millis(400));
    own
}

fn wait_until_gone(port: u16, pids: &[u32], budget: Duration) {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if listeners(port)
            .iter()
            .all(|listener| !pids.contains(&listener.pid))
        {
            return;
        }
        thread::sleep(Duration::from_millis(40));
    }
}

fn signal(pid: u32, sig: &str) -> bool {
    #[cfg(windows)]
    {
        let _ = sig;
        let mut cmd = Command::new("taskkill");
        crate::hide_console(&mut cmd)
            .args(["/PID", &pid.to_string(), "/F"])
            .stdout(Stdio::null())
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
            found.push(Listener { pid, name });
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
    let token = text.split_whitespace().next()?;
    Path::new(token)
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
            found.push(Listener { pid, name });
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
            found.push(Listener { pid, name });
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
    }

    #[test]
    fn a_free_port_stays_available_after_the_fast_path() {
        let listener = TcpListener::bind("0.0.0.0:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(reclaim(port).is_empty());
        assert!(TcpListener::bind(("0.0.0.0", port)).is_ok());
    }

    #[test]
    fn leaves_an_unrelated_listener_in_place() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(reclaim(port).is_empty());
        assert!(TcpListener::bind(("127.0.0.1", port)).is_err());
    }
}
