//! Unprivileged release-build supervisor. Normal exits never restart; crashes have a budget.
use airport_tun_service::runtime_log::RuntimeLog;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const WATCHDOG: &str = "--airport-watchdog";
const RECOVERED: &str = "--airport-recovered";
const WINDOW_SECONDS: u64 = 600;
const STABLE_AFTER: Duration = Duration::from_secs(120);

#[derive(Default, Serialize, Deserialize)]
struct Budget {
    crashes: Vec<u64>,
}
impl Budget {
    fn reserve(&mut self, now: u64) -> Option<Duration> {
        self.crashes
            .retain(|at| now.saturating_sub(*at) < WINDOW_SECONDS);
        if self.crashes.len() >= 3 {
            return None;
        }
        let delay = Duration::from_secs(1 << self.crashes.len());
        self.crashes.push(now);
        Some(delay)
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn budget_path(config: &Path) -> PathBuf {
    config.join("crash-restarts.json")
}
fn save_budget(config: &Path, budget: &Budget) -> Result<(), String> {
    crate::storage::write_private(
        &budget_path(config),
        &serde_json::to_vec(budget).map_err(|e| e.to_string())?,
    )
}
fn reserve_restart(config: &Path) -> Result<Option<Duration>, String> {
    let mut budget: Budget = match fs::read(budget_path(config)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Budget::default(),
        Err(e) => return Err(e.to_string()),
    };
    let delay = budget.reserve(now());
    save_budget(config, &budget)?;
    Ok(delay)
}
fn crash_log(config: &Path, message: &str) {
    if let Ok(log) = RuntimeLog::open(config.join("crash-logs")) {
        if let Ok(mut log) = log.lock() {
            let _ = log.append(message);
        }
    }
}
fn valid_directory(dir: &Path, config: &Path) -> bool {
    let Ok(config) = config.canonicalize() else {
        return false;
    };
    let Ok(actual) = dir.canonicalize() else {
        return false;
    };
    actual.parent() == Some(config.as_path())
        && actual
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("watchdog-"))
        && fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}
fn cleanup(dir: &Path) {
    let _ = fs::remove_file(dir.join("normal-exit"));
    // Only our empty directory, never recursively remove an argument supplied on the command line.
    let _ = fs::remove_dir(dir);
}
fn supervise(
    pid: u32,
    dir: &Path,
    config: &Path,
    restart: impl FnOnce() -> io::Result<()>,
) -> Result<(), String> {
    if pid <= 1 || !valid_directory(dir, config) {
        return Err("invalid watchdog arguments".into());
    }
    let started = Instant::now();
    let mut stable = false;
    while process_alive(pid) {
        if dir.join("normal-exit").is_file() {
            cleanup(dir);
            return Ok(());
        }
        if !stable && started.elapsed() >= STABLE_AFTER {
            save_budget(config, &Budget::default())?;
            stable = true;
        }
        thread::sleep(Duration::from_millis(500));
    }
    if dir.join("normal-exit").is_file() {
        cleanup(dir);
        return Ok(());
    }
    cleanup(dir);
    let Some(delay) = reserve_restart(config)? else {
        crash_log(config, "Client crash recovery stopped: three restarts within ten minutes. Start the app manually after checking the logs.");
        return Ok(());
    };
    crash_log(
        config,
        &format!(
            "Client process {pid} exited unexpectedly; restarting in {} seconds.",
            delay.as_secs()
        ),
    );
    thread::sleep(delay);
    restart().map_err(|e| e.to_string())
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    if pid > i32::MAX as u32 {
        return false;
    }
    // A permission error or another uncertain status is not proof of exit.
    unsafe { kill(pid as i32, 0) == 0 || io::Error::last_os_error().raw_os_error() != Some(3) }
}
#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return GetLastError() != ERROR_INVALID_PARAMETER;
        }
        let mut code = 259;
        let queried = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        queried == 0 || code == 259
    }
}

/// Called before Tauri initialization. The helper never opens a second webview or changes networking.
pub fn run_if_requested() -> bool {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_none_or(|a| a != WATCHDOG) {
        return false;
    }
    if args.len() != 4 {
        return true;
    }
    let Some(pid) = args[1].to_str().and_then(|s| s.parse::<u32>().ok()) else {
        return true;
    };
    let dir = PathBuf::from(&args[2]);
    let config = PathBuf::from(&args[3]);
    if !valid_directory(&dir, &config) {
        return true;
    }
    let result = supervise(pid, &dir, &config, || {
        let mut command = Command::new(std::env::current_exe()?);
        crate::hide_console(&mut command)
            .arg(RECOVERED)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    });
    if let Err(error) = result {
        crash_log(&config, &format!("Client recovery failed: {error}"));
    }
    true
}

pub struct Guard {
    directory: Option<PathBuf>,
    reconnect: AtomicBool,
    stopped: AtomicBool,
}
impl Guard {
    pub fn disabled() -> Self {
        Self {
            directory: None,
            reconnect: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        }
    }
    pub fn start(config: &Path) -> Result<Self, String> {
        let recovered = std::env::args_os().any(|a| a == RECOVERED);
        let intent = config.join("connection-intent.json");
        let reconnect = recovered && fs::read(&intent).is_ok_and(|bytes| bytes == b"true");
        // Consume once. A normal launch never inherits an earlier connection intent.
        crate::storage::write_private(&intent, b"false")?;
        if !recovered {
            save_budget(config, &Budget::default())?;
        }
        let mut guard = Self {
            directory: None,
            reconnect: AtomicBool::new(reconnect),
            stopped: AtomicBool::new(false),
        };
        if !cfg!(debug_assertions) {
            let dir = tempfile::Builder::new()
                .prefix("watchdog-")
                .tempdir_in(config)
                .map_err(|e| e.to_string())?;
            let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
            crate::hide_console(&mut command)
                .arg(WATCHDOG)
                .arg(std::process::id().to_string())
                .arg(dir.path())
                .arg(config)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            guard.directory = Some(dir.keep());
            install_panic_log(config.to_owned());
        }
        Ok(guard)
    }
    pub fn stop(&self) -> Result<(), String> {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        if let Some(dir) = &self.directory {
            if let Err(error) = crate::storage::write_private(&dir.join("normal-exit"), b"true") {
                self.stopped.store(false, Ordering::SeqCst);
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn take_reconnect(&self) -> bool {
        self.reconnect.swap(false, Ordering::SeqCst)
    }
}
fn install_panic_log(config: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // No Core/state lock here: the panic could have happened while holding it.
        // Do not record panic payloads, which may contain a subscription or token.
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        crash_log(&config, &format!("Client panic at {location}"));
        previous(info);
    }));
}
impl Drop for Guard {
    fn drop(&mut self) {
        if !thread::panicking() {
            let _ = self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_budget_is_bounded_and_expires() {
        let mut budget = Budget::default();
        for secs in [1, 2, 4] {
            assert_eq!(budget.reserve(100), Some(Duration::from_secs(secs)));
        }
        assert_eq!(budget.reserve(101), None);
        assert_eq!(budget.reserve(700), Some(Duration::from_secs(1)));
        // A backwards clock must not accidentally reset the budget.
        for _ in 0..3 {
            budget.reserve(1);
        }
        assert_eq!(budget.reserve(1), None);
    }
    #[test]
    fn corrupt_budget_fails_closed_and_manual_reset_recovers() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(budget_path(dir.path()), b"broken").unwrap();
        assert!(reserve_restart(dir.path()).is_err());
        save_budget(dir.path(), &Budget::default()).unwrap();
        assert_eq!(
            reserve_restart(dir.path()).unwrap(),
            Some(Duration::from_secs(1))
        );
    }
    #[test]
    fn only_direct_private_watchdog_directories_are_accepted() {
        let config = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let dir = tempfile::Builder::new()
            .prefix("watchdog-")
            .tempdir_in(config.path())
            .unwrap();
        assert!(valid_directory(dir.path(), config.path()));
        assert!(!valid_directory(other.path(), config.path()));
        assert!(!valid_directory(config.path(), config.path()));
        #[cfg(unix)]
        {
            let link = config.path().join("watchdog-link");
            std::os::unix::fs::symlink(dir.path(), &link).unwrap();
            assert!(!valid_directory(&link, config.path()));
        }
    }
    #[test]
    fn normal_exit_never_relaunches_even_while_parent_is_alive() {
        let config = tempfile::tempdir().unwrap();
        let dir = tempfile::Builder::new()
            .prefix("watchdog-")
            .tempdir_in(config.path())
            .unwrap();
        fs::write(dir.path().join("normal-exit"), b"true").unwrap();
        supervise(std::process::id(), dir.path(), config.path(), || {
            panic!("normal exit restarted")
        })
        .unwrap();
        assert!(!dir.path().exists());
    }
    #[test]
    fn repeated_stop_does_not_recreate_a_cleaned_directory() {
        let config = tempfile::tempdir().unwrap();
        let dir = tempfile::Builder::new()
            .prefix("watchdog-")
            .tempdir_in(config.path())
            .unwrap();
        let guard = Guard {
            directory: Some(dir.path().to_owned()),
            reconnect: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        };
        guard.stop().unwrap();
        cleanup(dir.path());
        guard.stop().unwrap();
        drop(guard);
        assert!(!dir.path().exists());
    }
    #[test]
    fn reconnect_request_is_one_shot() {
        let guard = Guard {
            directory: None,
            reconnect: AtomicBool::new(true),
            stopped: AtomicBool::new(false),
        };
        assert!(guard.take_reconnect());
        assert!(!guard.take_reconnect());
    }
    #[cfg(unix)]
    #[test]
    fn isolated_child_crash_relaunches_once_then_stops_at_budget() {
        let config = tempfile::tempdir().unwrap();
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 7"])
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(!process_alive(pid));
        assert!(process_alive(std::process::id()));
        let dir = tempfile::Builder::new()
            .prefix("watchdog-")
            .tempdir_in(config.path())
            .unwrap();
        let called = AtomicBool::new(false);
        supervise(pid, dir.path(), config.path(), || {
            called.store(true, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        assert!(called.load(Ordering::SeqCst));
        let budget = Budget {
            crashes: vec![now(); 3],
        };
        save_budget(config.path(), &budget).unwrap();
        let dir = tempfile::Builder::new()
            .prefix("watchdog-")
            .tempdir_in(config.path())
            .unwrap();
        supervise(pid, dir.path(), config.path(), || {
            panic!("restart budget ignored")
        })
        .unwrap();
    }
}
