//! Normal-user client for the narrowly scoped, installed network service.
use crate::message::{msg, text};
use airport_tun_service::{Action, Credentials, Response, Result, Timeouts, LEASE_SECONDS};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Condvar, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};
static INSTALL_LOCK: Mutex<()> = Mutex::new(());
pub fn connection_guard() -> Result<std::sync::MutexGuard<'static, ()>> {
    INSTALL_LOCK
        .try_lock()
        .map_err(|_| "native.tunServiceBusyConnectLater".into())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    ready: bool,
    running: bool,
    /// The installed service still runs the core or helper of an earlier app version.
    /// It works, but only a repair replaces the privileged copies with the ones shipped now.
    outdated: bool,
    message: String,
}
fn path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("tun-service.json"))
}
fn credentials(path: &Path) -> Result<Credentials> {
    serde_json::from_slice(&fs::read(path).map_err(|_| "native.tunServiceNotInstalled")?)
        .map_err(|_| "native.tunCredentialsCorrupt".into())
}
/// Digests of the core and helper this app ships, computed once: they are what a repair
/// would install, and do not change while the app runs.
fn shipped_digests() -> Option<&'static (String, String)> {
    static SHIPPED: OnceLock<Option<(String, String)>> = OnceLock::new();
    SHIPPED
        .get_or_init(|| {
            let core = digest(&crate::resolve_binary().ok()?).ok()?;
            let helper = digest(&crate::resolve_sidecar("airport-tun-service").ok()?).ok()?;
            Some((core, helper))
        })
        .as_ref()
}
/// Whether the service runs other binaries than the app ships. Unknown (binaries not found,
/// as in some development setups) counts as up to date: a false alarm would ask for an
/// administrator password for nothing.
fn outdated(response: &Response) -> bool {
    shipped_digests().is_some_and(|(core, helper)| {
        response.core_sha256.as_deref() != Some(core.as_str())
            || response.helper_sha256.as_deref() != Some(helper.as_str())
    })
}
fn status(path: &Path) -> ServiceStatus {
    match credentials(path).and_then(|c| c.call(Action::Status)) {
        Ok(response) => {
            let outdated = outdated(&response);
            ServiceStatus {
                ready: true,
                running: response.running,
                outdated,
                message: if outdated {
                    "native.tunServiceOutdated"
                } else {
                    "native.tunServiceReady"
                }
                .into(),
            }
        }
        Err(error) => ServiceStatus {
            ready: false,
            running: false,
            outdated: false,
            message: error,
        },
    }
}
#[tauri::command]
pub async fn tun_service_status(app: AppHandle) -> Result<ServiceStatus> {
    let path = path(&app)?;
    tauri::async_runtime::spawn_blocking(move || status(&path))
        .await
        .map_err(|e| e.to_string())
}
fn digest(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
#[cfg(any(target_os = "macos", test))]
fn shell_quote(value: &str) -> Result<String> {
    if value.chars().any(|c| c.is_control()) {
        return Err("native.installPathControlChars".into());
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}
fn elevated(helper: &Path, action: &str, manifest: Option<&Path>) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut shell = format!(
            "{} {}",
            shell_quote(&helper.to_string_lossy())?,
            shell_quote(action)?
        );
        if let Some(path) = manifest {
            shell.push(' ');
            shell.push_str(&shell_quote(&path.to_string_lossy())?);
        }
        let script = format!(
            "do shell script {} with administrator privileges",
            serde_json::to_string(&shell).map_err(|e| e.to_string())?
        );
        let mut cmd = Command::new("/usr/bin/osascript");
        cmd.args(["-e", &script]);
        cmd
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut cmd = Command::new("pkexec");
        cmd.arg(helper).arg(action);
        if let Some(path) = manifest {
            cmd.arg(path);
        }
        cmd
    };
    #[cfg(windows)]
    let mut command = {
        fn ps(value: &str) -> String {
            format!("'{}'", value.replace('\'', "''"))
        }
        let mut arguments = vec![action.to_owned()];
        if let Some(path) = manifest {
            let path = path.to_string_lossy();
            if path.contains('"') || path.chars().any(|c| c.is_control()) {
                return Err("native.installPathInvalid".into());
            }
            arguments.push(format!("\"{path}\""));
        }
        let script=format!("$ErrorActionPreference='Stop'; try {{ $p=Start-Process -FilePath {} -ArgumentList {} -Verb RunAs -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1 }}",ps(&helper.to_string_lossy()),ps(&arguments.join(" ")));
        let root = std::env::var_os("SystemRoot").ok_or("native.windowsDirNotFound")?;
        let mut cmd = Command::new(
            PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        );
        crate::hide_console(&mut cmd).args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ]);
        cmd
    };
    let result = command
        .output()
        .map_err(|e| msg("native.elevationFailed", &[("error", text(e))]))?;
    if !result.status.success() {
        return Err("native.tunServiceOperationIncomplete".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn install_tun_service(
    app: AppHandle,
    state: State<'_, Arc<Mutex<crate::Core>>>,
) -> Result<ServiceStatus> {
    let path = path(&app)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move||{
  let _install=INSTALL_LOCK.try_lock().map_err(|_|"native.tunInstallInProgress")?;
  let core=state.lock().map_err(|e|e.to_string())?;
  if core.child.is_some(){return Err("native.disconnectToInstallTun".into());}
  drop(core);
  let helper=crate::resolve_sidecar("airport-tun-service")?;let binary=crate::resolve_binary()?;
  // Keep an existing key on repair so failed installation never discards the only client credential.
  let credential=credentials(&path).or_else(|_|Credentials::generate())?;
  let staging=tempfile::tempdir().map_err(|e|e.to_string())?;let manifest=staging.path().join("install.json");
  let data=serde_json::json!({"credentials":credential,"core":binary,"core_sha256":digest(&binary)?});
  crate::storage::write_private(&manifest,&serde_json::to_vec(&data).map_err(|e|e.to_string())?)?;
  // Persist before elevation; cancellation leaves only a private random credential, no service.
  crate::storage::write_private(&path,&serde_json::to_vec(&credential).map_err(|e|e.to_string())?)?;
  elevated(&helper,"--install",Some(&manifest))?;
  let deadline=Instant::now()+Duration::from_secs(10);
  loop {
    let result = status(&path);
    if result.ready {
      return Ok(result);
    } else if Instant::now() > deadline {
      return Err("native.tunServiceInstalledNotStarted".into());
    }
    thread::sleep(Duration::from_millis(200));
  }
 }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn uninstall_tun_service(
    app: AppHandle,
    state: State<'_, Arc<Mutex<crate::Core>>>,
) -> Result<ServiceStatus> {
    let path = path(&app)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _install = INSTALL_LOCK.try_lock().map_err(|_| "tun.busy")?;
        let core = state.lock().map_err(|e| e.to_string())?;
        if core.child.is_some() {
            return Err("native.disconnectToUninstallTun".into());
        }
        drop(core);
        elevated(
            &crate::resolve_sidecar("airport-tun-service")?,
            "--uninstall",
            None,
        )?;
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(ServiceStatus {
            ready: false,
            running: false,
            outdated: false,
            message: "tun.tunServiceUninstalled".into(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// How often the app renews its lease. The service stops the core when no heartbeat arrives
/// for `LEASE_SECONDS`, so this leaves several attempts before that happens.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
/// A heartbeat that cannot be exchanged says nothing about the tunnel: the service may be busy
/// or the machine may just have woken. The core is treated as running until heartbeats have
/// failed for this long, which still ends before the service gives up the lease on its own.
const HEARTBEAT_GRACE: Duration = Duration::from_secs(LEASE_SECONDS - 5);

pub struct ServiceProcess {
    logs: Arc<Mutex<Vec<String>>>,
    credentials: Credentials,
    session: String,
    /// When a heartbeat was last attempted, and when one was last answered.
    polled: Option<Instant>,
    answered: Instant,
}
impl ServiceProcess {
    pub fn start(
        path: &Path,
        config: serde_json::Value,
        logs: Arc<Mutex<Vec<String>>>,
    ) -> Result<Self> {
        let credentials = credentials(path)?;
        let session = airport_tun_service::random_id()?;
        let mut process = Self {
            logs,
            credentials,
            session,
            polled: None,
            answered: Instant::now(),
        };
        let start = |process: &Self, config| {
            process.credentials.call(Action::Start {
                config,
                session: process.session.clone(),
            })
        };
        let response = match start(&process, config.clone()) {
            // A session this app no longer knows: it quit or crashed while connected and came
            // back before the service gave the old session up. Nobody renews that lease any more,
            // so it ends by itself within the lease time; wait for that instead of failing.
            Err(error) if error == "native.tunSessionExists" => {
                process.await_idle()?;
                start(&process, config)?
            }
            other => other?,
        };
        if !response.running {
            return Err("native.tunCoreNotStarted".into());
        }
        process.poll()?;
        Ok(process)
    }
    /// Waits for the service to drop a session left behind by a previous run of the app.
    fn await_idle(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(LEASE_SECONDS + 3);
        loop {
            if !self.credentials.call(Action::Status)?.running {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err("native.tunSessionExists".into());
            }
            thread::sleep(Duration::from_millis(500));
        }
    }
    /// Renews the lease and collects the core's log lines. `Ok(None)` while the core runs,
    /// `Ok(Some(reason))` once the service says it does not.
    pub fn poll(&mut self) -> Result<Option<String>> {
        if self
            .polled
            .is_some_and(|at| at.elapsed() < HEARTBEAT_INTERVAL)
        {
            return Ok(None);
        }
        self.polled = Some(Instant::now());
        let response = match self.credentials.exchange(
            Action::Heartbeat {
                session: self.session.clone(),
            },
            Timeouts::HEARTBEAT,
        ) {
            Ok(response) => response,
            Err(_) if self.answered.elapsed() < HEARTBEAT_GRACE => return Ok(None),
            Err(error) => return Err(error),
        };
        self.answered = Instant::now();
        // The service answered, and its answer is final: the session is gone or was replaced.
        if let Some(error) = response.error {
            return Ok(Some(error));
        }
        for line in &response.logs {
            crate::remember(&self.logs, &format!("[TUN] {line}"));
        }
        Ok(
            if response.running && response.session.as_deref() == Some(&self.session) {
                None
            } else {
                Some("native.tunCoreStopped".into())
            },
        )
    }
    pub fn stop(&mut self) -> Result<()> {
        self.credentials
            .call(Action::Stop {
                session: self.session.clone(),
            })
            .map(|_| ())
    }
    /// Tells the service the tunnel has been verified: a block left by an earlier drop is lifted,
    /// and from here on an unexpected exit of the core blocks the network.
    pub fn arm(&self) -> Result<()> {
        self.credentials
            .call(Action::Arm {
                session: self.session.clone(),
            })
            .map(|_| ())
    }
}
/// Lifts the kill switch and stops protecting. Needs no running session: it is how the
/// network is restored after a drop the app did not recover from.
pub fn release_guard(path: &Path) -> Result<()> {
    credentials(path)?.call(Action::Release).map(|_| ())
}

/// Opened after the boot-time Release has been applied. `Action::Release` disarms whatever
/// session is current, so a TUN connect has to wait: otherwise a slow boot Release can land
/// after this process has already armed the kill switch and silently turn protection off.
#[derive(Clone)]
pub struct StartupGate {
    done: Arc<(Mutex<bool>, Condvar)>,
}
struct GateSignal(Arc<(Mutex<bool>, Condvar)>);
impl Drop for GateSignal {
    fn drop(&mut self) {
        let (lock, cv) = &*self.0;
        let mut done = lock.lock().unwrap_or_else(|poison| poison.into_inner());
        *done = true;
        cv.notify_all();
    }
}
impl StartupGate {
    fn pair() -> (Self, GateSignal) {
        let done = Arc::new((Mutex::new(false), Condvar::new()));
        (Self { done: done.clone() }, GateSignal(done))
    }
    pub fn start(path: PathBuf) -> Self {
        let (gate, signal) = Self::pair();
        // If the thread cannot be spawned, dropping the closure drops the signal and the gate
        // opens. Connecting must not wait forever for a release that never started.
        let _ = thread::Builder::new()
            .name("tun-startup-release".into())
            .spawn(move || {
                let _signal = signal;
                let _ = release_guard(&path);
            });
        gate
    }
    pub fn wait(&self) {
        let (lock, cv) = &*self.done;
        let mut done = lock.lock().unwrap_or_else(|poison| poison.into_inner());
        while !*done {
            done = cv.wait(done).unwrap_or_else(|poison| poison.into_inner());
        }
    }
}
/// Before restoring a connection after a GUI crash, let the orphaned session's lease expire.
/// Missing/offline helpers cannot own a session; the normal connect path still validates them.
pub fn await_previous_session(path: &Path) -> Result<()> {
    let credentials = credentials(path)?;
    let deadline = Instant::now() + Duration::from_secs(LEASE_SECONDS + 3);
    loop {
        match credentials.exchange(Action::Status, Timeouts::HEARTBEAT) {
            Ok(response) if response.running => {}
            Ok(_) | Err(_) => return Ok(()),
        }
        if Instant::now() >= deadline {
            return Err("native.tunSessionExists".into());
        }
        thread::sleep(Duration::from_millis(500));
    }
}
/// Renews the lease on a block while no core runs, and reports whether the service is still
/// blocking. Without this the service would lift the block as if the app had gone away.
pub fn guard_keepalive(path: &Path) -> Result<bool> {
    credentials(path)?
        .exchange(Action::Status, Timeouts::HEARTBEAT)
        .map(|response| response.blocking)
}
impl Drop for ServiceProcess {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[test]
    fn installer_quotes_paths() {
        assert_eq!(shell_quote("a'b $(whoami)").unwrap(), "'a'\\''b $(whoami)'");
        assert!(shell_quote("x\ny").is_err());
    }
    #[test]
    fn startup_release_gate_blocks_until_the_release_finishes_and_then_stays_open() {
        let (gate, signal) = StartupGate::pair();
        let waiting = gate.clone();
        let entered = Arc::new(AtomicBool::new(false));
        let flag = entered.clone();
        let blocked = thread::spawn(move || {
            flag.store(true, Ordering::SeqCst);
            waiting.wait();
        });
        while !entered.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(1));
        }
        thread::sleep(Duration::from_millis(20));
        assert!(!blocked.is_finished());
        drop(signal);
        blocked.join().unwrap();
        let started = Instant::now();
        gate.wait();
        assert!(started.elapsed() < Duration::from_millis(50));
    }
}
