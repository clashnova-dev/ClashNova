use crate::{Result, STOP};
#[cfg(target_os = "macos")]
use std::process::Stdio;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};
pub fn core_name() -> &'static str {
    if cfg!(windows) {
        "airport-core.exe"
    } else {
        "airport-core"
    }
}
/// Left in the service directory by releases that installed the upstream sing-box build
/// and its Windows library instead of the core built from this repository.
pub const LEGACY_FILES: [&str; 3] = ["sing-box", "sing-box.exe", "libcronet.dll"];
pub fn helper_name() -> &'static str {
    if cfg!(windows) {
        "airport-tun-service.exe"
    } else {
        "airport-tun-service"
    }
}
#[cfg(target_os = "macos")]
pub fn root() -> PathBuf {
    PathBuf::from("/Library/PrivilegedHelperTools/dev.airport.tun")
}
#[cfg(target_os = "linux")]
pub fn root() -> PathBuf {
    PathBuf::from("/opt/airport-tun")
}
#[cfg(windows)]
pub fn root() -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath},
    };
    let mut pointer = std::ptr::null_mut();
    unsafe {
        let result = SHGetKnownFolderPath(
            &FOLDERID_ProgramFiles,
            0,
            std::ptr::null_mut(),
            &mut pointer,
        );
        assert!(
            result >= 0 && !pointer.is_null(),
            "Program Files is unavailable"
        );
        let mut length = 0;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        let directory = std::ffi::OsString::from_wide(std::slice::from_raw_parts(pointer, length));
        CoTaskMemFree(pointer.cast());
        PathBuf::from(directory).join("AirportTun")
    }
}
fn run(command: &mut Command) -> Result<()> {
    let output = command.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "System service operation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// Opens the proxy port to the local subnet so LAN devices can use this host as a proxy while
/// "allow LAN" is on. Windows blocks inbound by default; the service runs elevated, so netsh
/// needs no prompt. Scoped to the local subnet, and removed again on stop. A no-op where the
/// platform does not block inbound (macOS/Linux). The rule name has no spaces so it needs no
/// argument quoting.
#[cfg(windows)]
pub fn open_lan_port(port: u16) -> Result<()> {
    close_lan_port(port);
    let name = format!("name=ClashNova-LAN-{port}");
    let localport = format!("localport={port}");
    for protocol in ["protocol=tcp", "protocol=udp"] {
        run(Command::new("netsh.exe").args([
            "advfirewall", "firewall", "add", "rule", name.as_str(), "dir=in", "action=allow",
            protocol, localport.as_str(), "remoteip=localsubnet", "profile=any", "enable=yes",
        ]))?;
    }
    Ok(())
}
#[cfg(windows)]
pub fn close_lan_port(port: u16) {
    let name = format!("name=ClashNova-LAN-{port}");
    let _ = Command::new("netsh.exe")
        .args(["advfirewall", "firewall", "delete", "rule", name.as_str()])
        .output();
}
#[cfg(not(windows))]
pub fn open_lan_port(_port: u16) -> Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn close_lan_port(_port: u16) {}
#[cfg(unix)]
pub fn require_admin() -> Result<()> {
    if unsafe { libc::geteuid() } == 0 {
        Ok(())
    } else {
        Err("System administrator authorization is required".into())
    }
}
#[cfg(windows)]
pub fn require_admin() -> Result<()> {
    if unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() } != 0 {
        Ok(())
    } else {
        Err("Administrator authorization is required".into())
    }
}
#[cfg(unix)]
pub fn prepare_root() -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let root = root();
    // Validate every existing ancestor before creating or writing privileged files.
    let mut current = PathBuf::from("/");
    for part in root.components().skip(1) {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) => {
                if !meta.is_dir()
                    || meta.file_type().is_symlink()
                    || meta.uid() != 0
                    || meta.mode() & 0o022 != 0
                {
                    return Err("Service directory permissions are unsafe".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current).map_err(|e| e.to_string())?;
                std::fs::set_permissions(&current, std::fs::Permissions::from_mode(0o700))
                    .map_err(|e| e.to_string())?;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
#[cfg(windows)]
pub fn prepare_root() -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    let root = root();
    if root.exists()
        && std::fs::symlink_metadata(&root)
            .map_err(|e| e.to_string())?
            .file_attributes()
            & 0x400
            != 0
    {
        return Err("Service directory must not be a reparse point".into());
    }
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    run(Command::new("icacls.exe").arg(&root).arg("/reset"))?;
    run(Command::new("icacls.exe").arg(root).args([
        "/inheritance:r",
        "/grant:r",
        "*S-1-5-18:(OI)(CI)F",
        "*S-1-5-32-544:(OI)(CI)F",
    ]))
}
#[cfg(target_os = "macos")]
const PLIST: &str = "/Library/LaunchDaemons/dev.airport.tun.plist";
#[cfg(target_os = "macos")]
pub fn stop_existing() -> Result<()> {
    let exists = Command::new("/bin/launchctl")
        .args(["print", "system/dev.airport.tun"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?
        .success();
    if exists {
        run(Command::new("/bin/launchctl").args(["bootout", "system/dev.airport.tun"]))?;
        thread::sleep(Duration::from_millis(300));
    }
    Ok(())
}
#[cfg(target_os = "macos")]
pub fn register() -> Result<()> {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>Label</key><string>dev.airport.tun</string><key>ProgramArguments</key><array><string>/Library/PrivilegedHelperTools/dev.airport.tun/airport-tun-service</string><string>--serve</string></array><key>RunAtLoad</key><true/><key>KeepAlive</key><true/><key>WorkingDirectory</key><string>/Library/PrivilegedHelperTools/dev.airport.tun</string><key>ExitTimeOut</key><integer>8</integer><key>ThrottleInterval</key><integer>5</integer></dict></plist>"#;
    crate::secure_write(Path::new(PLIST), xml.as_bytes(), 0o644)?;
    run(Command::new("/bin/launchctl").args(["enable", "system/dev.airport.tun"]))?;
    run(Command::new("/bin/launchctl").args(["bootstrap", "system", PLIST]))
}
#[cfg(target_os = "macos")]
pub fn uninstall() -> Result<()> {
    prepare_root()?;
    stop_existing()?;
    if Path::new(PLIST).exists() {
        std::fs::remove_file(PLIST).map_err(|e| e.to_string())?;
    }
    std::fs::remove_dir_all(root()).map_err(|e| e.to_string())
}
#[cfg(target_os = "linux")]
enum Init {
    Systemd,
    OpenRc,
}
#[cfg(target_os = "linux")]
fn init_system() -> Result<Init> {
    if Path::new("/run/systemd/system").exists() {
        Ok(Init::Systemd)
    } else if Path::new("/run/openrc").exists()
        || ["/sbin/openrc-run", "/usr/sbin/openrc-run"]
            .iter()
            .any(|p| Path::new(p).exists())
    {
        Ok(Init::OpenRc)
    } else {
        Err("No supported init system found (expected systemd or OpenRC).".into())
    }
}
#[cfg(target_os = "linux")]
fn tool(candidates: &[&'static str]) -> Result<&'static str> {
    candidates
        .iter()
        .copied()
        .find(|p| Path::new(p).exists())
        .ok_or_else(|| format!("Required tool not found (looked for {candidates:?})"))
}
#[cfg(target_os = "linux")]
pub fn stop_existing() -> Result<()> {
    match init_system()? {
        Init::Systemd => {
            if Path::new("/etc/systemd/system/airport-tun.service").exists() {
                run(Command::new("/usr/bin/systemctl").args(["stop", "airport-tun.service"]))?;
            }
        }
        Init::OpenRc => {
            if Path::new("/etc/init.d/airport-tun").exists() {
                let _ = run(Command::new(tool(&["/sbin/rc-service", "/usr/sbin/rc-service"])?)
                    .args(["airport-tun", "stop"]));
            }
        }
    }
    Ok(())
}
#[cfg(target_os = "linux")]
pub fn register() -> Result<()> {
    if !Path::new("/dev/net/tun").exists() {
        let modprobe = if Path::new("/usr/sbin/modprobe").exists() {
            "/usr/sbin/modprobe"
        } else {
            "/sbin/modprobe"
        };
        run(Command::new(modprobe).arg("tun"))?;
        if !Path::new("/dev/net/tun").exists() {
            return Err(
                "The kernel provides no TUN device. Check the kernel or container permissions."
                    .into(),
            );
        }
    }
    match init_system()? {
        Init::Systemd => {
            let unit="[Unit]\nDescription=Airport TUN service\nAfter=network.target\n[Service]\nExecStart=/opt/airport-tun/airport-tun-service --serve\nWorkingDirectory=/opt/airport-tun\nRestart=on-failure\nRestartSec=3\nKillMode=mixed\nTimeoutStopSec=8\nUMask=0077\nNoNewPrivileges=true\n[Install]\nWantedBy=multi-user.target\n";
            crate::secure_write(
                Path::new("/etc/systemd/system/airport-tun.service"),
                unit.as_bytes(),
                0o644,
            )?;
            run(Command::new("/usr/bin/systemctl").arg("daemon-reload"))?;
            run(Command::new("/usr/bin/systemctl").args(["enable", "--now", "airport-tun.service"]))
        }
        Init::OpenRc => {
            let script = r#"#!/sbin/openrc-run
name="airport-tun"
description="Airport TUN service"
supervisor=supervise-daemon
command="/opt/airport-tun/airport-tun-service"
command_args="--serve"
respawn_delay=3

depend() {
	need net
}
"#;
            crate::secure_write(Path::new("/etc/init.d/airport-tun"), script.as_bytes(), 0o755)?;
            run(Command::new(tool(&["/sbin/rc-update", "/usr/sbin/rc-update"])?)
                .args(["add", "airport-tun", "default"]))?;
            run(Command::new(tool(&["/sbin/rc-service", "/usr/sbin/rc-service"])?)
                .args(["airport-tun", "restart"]))
        }
    }
}
#[cfg(target_os = "linux")]
pub fn uninstall() -> Result<()> {
    prepare_root()?;
    stop_existing()?;
    match init_system()? {
        Init::Systemd => {
            run(Command::new("/usr/bin/systemctl").args(["disable", "airport-tun.service"]))?;
            if Path::new("/etc/systemd/system/airport-tun.service").exists() {
                std::fs::remove_file("/etc/systemd/system/airport-tun.service")
                    .map_err(|e| e.to_string())?;
            }
            run(Command::new("/usr/bin/systemctl").arg("daemon-reload"))?;
        }
        Init::OpenRc => {
            let _ = run(Command::new(tool(&["/sbin/rc-update", "/usr/sbin/rc-update"])?)
                .args(["del", "airport-tun", "default"]));
            if Path::new("/etc/init.d/airport-tun").exists() {
                std::fs::remove_file("/etc/init.d/airport-tun").map_err(|e| e.to_string())?;
            }
        }
    }
    std::fs::remove_dir_all(root()).map_err(|e| e.to_string())
}
#[cfg(unix)]
pub fn serve() -> Result<()> {
    extern "C" fn stop(_: i32) {
        STOP.store(true, Ordering::Relaxed);
    }
    unsafe {
        libc::signal(libc::SIGTERM, stop as *const () as usize);
        libc::signal(libc::SIGINT, stop as *const () as usize);
    }
    crate::daemon()
}
pub fn core_command(root: &Path) -> Command {
    let mut cmd = Command::new(root.join(core_name()));
    cmd.current_dir(root).env_clear();
    // The kill switch tells the core's own sockets from everything else by their group.
    // Only root may change group; an unprivileged test run keeps its own.
    #[cfg(unix)]
    if unsafe { libc::geteuid() } == 0 {
        use std::os::unix::process::CommandExt;
        cmd.gid(airport_tun_service::CORE_GID);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x00000200);
        if let Some(windir) = std::env::var_os("SystemRoot") {
            cmd.env("SystemRoot", windir);
        }
    }
    cmd
}
pub fn terminate(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent(
            windows_sys::Win32::System::Console::CTRL_BREAK_EVENT,
            child.id(),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(30));
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };
    define_windows_service!(entry, main);
    pub fn serve() -> Result<()> {
        service_dispatcher::start("AirportTun", entry).map_err(|e| e.to_string())
    }
    fn main(_: Vec<std::ffi::OsString>) {
        let _ = run_service();
    }
    fn run_service() -> Result<()> {
        // Keep this handle alive until process exit. Windows closes it on a crash,
        // terminating the inherited child job instead of leaving a privileged core orphaned.
        unsafe {
            use windows_sys::Win32::System::{JobObjects::*, Threading::GetCurrentProcess};
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err("Could not create the service process job".into());
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(job, GetCurrentProcess()) == 0
            {
                windows_sys::Win32::Foundation::CloseHandle(job);
                return Err("Could not enable the service process job".into());
            }
        }

        let handle = service_control_handler::register("AirportTun", |event| match event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                STOP.store(true, Ordering::Relaxed);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })
        .map_err(|e| e.to_string())?;
        // A private console lets CTRL_BREAK reach the child process group for route cleanup.
        unsafe {
            windows_sys::Win32::System::Console::AllocConsole();
            let window = windows_sys::Win32::System::Console::GetConsoleWindow();
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(window, 0);
        }
        let status = |state, code| ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::Win32(code),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        };
        handle
            .set_service_status(status(ServiceState::Running, 0))
            .map_err(|e| e.to_string())?;
        let result = crate::daemon();
        let _ = handle.set_service_status(status(
            ServiceState::Stopped,
            if result.is_ok() { 0 } else { 1 },
        ));
        result
    }
}
#[cfg(windows)]
pub use win::serve;
#[cfg(windows)]
pub fn stop_existing() -> Result<()> {
    use windows_service::{
        service::{ServiceAccess, ServiceState},
        service_manager::{ServiceManager, ServiceManagerAccess},
    };
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| e.to_string())?;
    match manager.open_service(
        "AirportTun",
        ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
    ) {
        Ok(service) => {
            let status = service.query_status().map_err(|e| e.to_string())?;
            if status.current_state != ServiceState::Stopped {
                service.stop().map_err(|e| e.to_string())?;
                let until = Instant::now() + Duration::from_secs(10);
                while service
                    .query_status()
                    .map_err(|e| e.to_string())?
                    .current_state
                    != ServiceState::Stopped
                {
                    if Instant::now() > until {
                        return Err("Timed out stopping the TUN service".into());
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(1060) => {}
        Err(e) => return Err(e.to_string()),
    }
    Ok(())
}
#[cfg(windows)]
pub fn register() -> Result<()> {
    use windows_service::{
        service::{
            ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
            ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
            ServiceType,
        },
        service_manager::{ServiceManager, ServiceManagerAccess},
    };
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(|e| e.to_string())?;
    let info = ServiceInfo {
        name: "AirportTun".into(),
        display_name: "Airport TUN Service".into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: root().join(helper_name()),
        launch_arguments: vec!["--serve".into()],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let service = match manager.open_service(
        "AirportTun",
        ServiceAccess::CHANGE_CONFIG | ServiceAccess::START,
    ) {
        Ok(s) => {
            s.change_config(&info).map_err(|e| e.to_string())?;
            s
        }
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(1060) => manager
            .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
            .map_err(|e| e.to_string())?,
        Err(e) => return Err(e.to_string()),
    };
    // launchd and systemd bring the service back after a crash; Windows only does when told to.
    // Without this a crashed service stays down until the next boot and TUN cannot be started.
    let restart = || ServiceAction {
        action_type: ServiceActionType::Restart,
        delay: Duration::from_secs(3),
    };
    service
        .update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(3600)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![restart(), restart(), restart()]),
        })
        .map_err(|e| e.to_string())?;
    service
        .set_failure_actions_on_non_crash_failures(true)
        .map_err(|e| e.to_string())?;
    service.start::<&str>(&[]).map_err(|e| e.to_string())
}
#[cfg(windows)]
pub fn uninstall() -> Result<()> {
    use windows_service::{
        service::ServiceAccess,
        service_manager::{ServiceManager, ServiceManagerAccess},
    };
    prepare_root()?;
    stop_existing()?;
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| e.to_string())?;
    let service = manager
        .open_service("AirportTun", ServiceAccess::DELETE)
        .map_err(|e| e.to_string())?;
    service.delete().map_err(|e| e.to_string())?;
    // The running uninstaller cannot delete its own executable on Windows. Remove secrets now.
    for name in ["credentials.json", "runtime.json", core_name()]
        .into_iter()
        .chain(LEGACY_FILES)
    {
        let path = root().join(name);
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ---- Kill switch -------------------------------------------------------------------------
//
// When armed and the core exits unexpectedly, everything leaving the machine is blocked except:
//   - loopback;
//   - traffic already inside the tunnel, recognised by the device's fixed source addresses,
//     so it flows again the moment a new core brings the device up;
//   - the core itself (its group on Unix, its executable on Windows), so a reconnect can
//     reach the proxy servers through the block;
//   - address assignment (DHCP, IPv6 neighbour discovery), so the machine keeps its lease;
//   - destinations the user keeps off the tunnel.
// The rules live in one named anchor/table/session, so releasing is removing that one object,
// and none of them survives a reboot.

#[cfg(any(unix, test))]
fn split_families(bypass: &[String]) -> (Vec<&str>, Vec<&str>) {
    bypass
        .iter()
        .map(String::as_str)
        .filter(|prefix| airport_tun_service::valid_cidr(prefix))
        .partition(|prefix| !prefix.contains(':'))
}

/// pf rules for macOS. First match wins (`quick`), the block comes last.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn pf_rules(bypass: &[String]) -> String {
    use airport_tun_service::{CORE_GID, TUN_ADDRESS_V4, TUN_ADDRESS_V6};
    let (v4, v6) = split_families(bypass);
    let mut rules = vec![
        "pass out quick on lo0 all".to_owned(),
        format!("pass out quick inet from {TUN_ADDRESS_V4} to any"),
        format!("pass out quick inet6 from {TUN_ADDRESS_V6} to any"),
        format!("pass out quick proto tcp all group {CORE_GID}"),
        format!("pass out quick proto udp all group {CORE_GID}"),
        "pass out quick inet proto udp from any port 68 to any port 67".to_owned(),
        "pass out quick inet6 proto udp from any port 546 to any port 547".to_owned(),
        "pass out quick inet6 proto icmp6 all".to_owned(),
    ];
    rules.extend(
        v4.iter()
            .map(|prefix| format!("pass out quick inet from any to {prefix}")),
    );
    rules.extend(
        v6.iter()
            .map(|prefix| format!("pass out quick inet6 from any to {prefix}")),
    );
    rules.push("block drop out quick all".to_owned());
    rules.join("\n") + "\n"
}

/// nftables rules for Linux. Declaring, deleting and redefining the table in one file makes
/// loading it atomic and repeatable.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn nft_rules(bypass: &[String]) -> String {
    use airport_tun_service::{CORE_GID, TUN_ADDRESS_V4, TUN_ADDRESS_V6};
    let (v4, v6) = split_families(bypass);
    let mut rules = vec![
        "oifname \"lo\" accept".to_owned(),
        format!("ip saddr {TUN_ADDRESS_V4} accept"),
        format!("ip6 saddr {TUN_ADDRESS_V6} accept"),
        format!("meta skgid {CORE_GID} accept"),
        "udp sport 68 udp dport 67 accept".to_owned(),
        "udp sport 546 udp dport 547 accept".to_owned(),
        "meta l4proto ipv6-icmp accept".to_owned(),
    ];
    if !v4.is_empty() {
        rules.push(format!("ip daddr {{ {} }} accept", v4.join(", ")));
    }
    if !v6.is_empty() {
        rules.push(format!("ip6 daddr {{ {} }} accept", v6.join(", ")));
    }
    let body = rules
        .iter()
        .map(|rule| format!("\t\t{rule}\n"))
        .collect::<String>();
    format!(
        "table inet {NFT_TABLE}\ndelete table inet {NFT_TABLE}\ntable inet {NFT_TABLE} {{\n\tchain output {{\n\t\ttype filter hook output priority 0; policy drop;\n{body}\t}}\n}}\n"
    )
}
#[cfg(any(target_os = "linux", test))]
const NFT_TABLE: &str = "airport_tun_guard";

#[cfg(target_os = "macos")]
mod guard {
    use super::*;
    // Apple's stock ruleset evaluates every anchor below `com.apple`, so the rules take effect
    // without replacing the main ruleset, which other software may own.
    const ANCHOR: &str = "com.apple/dev.airport.tun";
    const PFCTL: &str = "/sbin/pfctl";
    pub struct SystemGuard {
        rules: PathBuf,
        /// pf is enabled by reference; this is our reference, returned on release.
        token: Option<String>,
    }
    pub fn new(root: &Path) -> SystemGuard {
        SystemGuard {
            rules: root.join("guard.pf"),
            token: None,
        }
    }
    // pfctl reports warnings on stderr even when it succeeds; the exit status decides.
    fn pfctl(args: &[&str]) -> Result<String> {
        let output = Command::new(PFCTL)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| e.to_string())?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if output.status.success() {
            Ok(text)
        } else {
            Err(format!("pfctl {}: {}", args.join(" "), text.trim()))
        }
    }
    impl crate::Guard for SystemGuard {
        fn engage(&mut self, bypass: &[String]) -> Result<()> {
            // A main ruleset without Apple's anchor would never consult ours: blocking would
            // silently do nothing, which is worse than saying it cannot be done.
            if !pfctl(&["-sr"])?.contains("anchor \"com.apple/*\"") {
                return Err("the system packet filter does not evaluate com.apple anchors".into());
            }
            crate::secure_write(&self.rules, pf_rules(bypass).as_bytes(), 0o600)?;
            let rules = self.rules.to_string_lossy().into_owned();
            pfctl(&["-a", ANCHOR, "-f", &rules])?;
            if self.token.is_none() {
                let enabled = pfctl(&["-E"])?;
                self.token = enabled
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("Token : "))
                    .map(|token| token.trim().to_owned())
                    .filter(|token| !token.is_empty() && token.bytes().all(|c| c.is_ascii_digit()));
            }
            Ok(())
        }
        fn release(&mut self) -> Result<()> {
            // Flushing an anchor that was never loaded is not an error worth reporting.
            let flushed = pfctl(&["-a", ANCHOR, "-F", "all"]);
            if let Some(token) = self.token.take() {
                let _ = pfctl(&["-X", &token]);
            }
            let _ = std::fs::remove_file(&self.rules);
            match flushed {
                Err(error)
                    if !error.contains("Anchor or Ruleset does not exist")
                        && !error.contains("No such") =>
                {
                    Err(error)
                }
                _ => Ok(()),
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod guard {
    use super::*;
    pub struct SystemGuard {
        rules: PathBuf,
    }
    pub fn new(root: &Path) -> SystemGuard {
        SystemGuard {
            rules: root.join("guard.nft"),
        }
    }
    fn nft() -> Result<&'static str> {
        ["/usr/sbin/nft", "/sbin/nft", "/usr/bin/nft"]
            .into_iter()
            .find(|path| Path::new(path).exists())
            .ok_or_else(|| "nftables (nft) is not installed".to_owned())
    }
    impl crate::Guard for SystemGuard {
        fn engage(&mut self, bypass: &[String]) -> Result<()> {
            crate::secure_write(&self.rules, nft_rules(bypass).as_bytes(), 0o600)?;
            run(Command::new(nft()?).arg("-f").arg(&self.rules))
        }
        fn release(&mut self) -> Result<()> {
            let _ = std::fs::remove_file(&self.rules);
            // Without nft there is nothing that could have been loaded.
            let Ok(nft) = nft() else { return Ok(()) };
            let output = Command::new(nft)
                .args(["delete", "table", "inet", NFT_TABLE])
                .output()
                .map_err(|e| e.to_string())?;
            let error = String::from_utf8_lossy(&output.stderr);
            if output.status.success() || error.contains("No such file or directory") {
                Ok(())
            } else {
                Err(format!("nft delete table: {}", error.trim()))
            }
        }
    }
}

#[cfg(windows)]
mod guard {
    use super::*;
    use std::{
        net::{IpAddr, Ipv4Addr, Ipv6Addr},
        os::windows::ffi::OsStrExt,
        ptr,
    };
    use windows_sys::core::GUID;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::*;
    use windows_sys::Win32::System::Rpc::RPC_C_AUTHN_WINNT;

    const SUBLAYER: GUID = GUID::from_u128(0x5b6f3a52_3f0e_4c62_9a41_7d1e2c8f47a1);
    const PERMIT_WEIGHT: u8 = 15;
    const BLOCK_WEIGHT: u8 = 0;
    const UDP: u8 = 17;

    /// The filters belong to a dynamic session: closing the engine handle removes them, and so
    /// does the end of this process, however it ends.
    pub struct SystemGuard {
        core: PathBuf,
        engine: HANDLE,
    }
    pub fn new(root: &Path) -> SystemGuard {
        SystemGuard {
            core: root.join(core_name()),
            engine: ptr::null_mut(),
        }
    }
    fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }
    fn check(code: u32, what: &str) -> Result<()> {
        if code == 0 {
            Ok(())
        } else {
            Err(format!("{what} failed with code {code:#x}"))
        }
    }
    fn condition(field: GUID, match_type: FWP_MATCH_TYPE) -> FWPM_FILTER_CONDITION0 {
        let mut condition: FWPM_FILTER_CONDITION0 = unsafe { std::mem::zeroed() };
        condition.fieldKey = field;
        condition.matchType = match_type;
        condition
    }
    fn equals_u8(field: GUID, value: u8) -> FWPM_FILTER_CONDITION0 {
        let mut c = condition(field, FWP_MATCH_EQUAL);
        c.conditionValue.r#type = FWP_UINT8;
        c.conditionValue.Anonymous.uint8 = value;
        c
    }
    fn equals_u16(field: GUID, value: u16) -> FWPM_FILTER_CONDITION0 {
        let mut c = condition(field, FWP_MATCH_EQUAL);
        c.conditionValue.r#type = FWP_UINT16;
        c.conditionValue.Anonymous.uint16 = value;
        c
    }
    fn equals_u32(field: GUID, match_type: FWP_MATCH_TYPE, value: u32) -> FWPM_FILTER_CONDITION0 {
        let mut c = condition(field, match_type);
        c.conditionValue.r#type = FWP_UINT32;
        c.conditionValue.Anonymous.uint32 = value;
        c
    }
    unsafe fn add(
        engine: HANDLE,
        layer: GUID,
        conditions: &mut [FWPM_FILTER_CONDITION0],
        permit: bool,
    ) -> Result<()> {
        let mut name = wide(std::ffi::OsStr::new("Airport TUN kill switch"));
        let mut filter: FWPM_FILTER0 = std::mem::zeroed();
        filter.displayData.name = name.as_mut_ptr();
        filter.layerKey = layer;
        filter.subLayerKey = SUBLAYER;
        filter.weight.r#type = FWP_UINT8;
        filter.weight.Anonymous.uint8 = if permit { PERMIT_WEIGHT } else { BLOCK_WEIGHT };
        filter.numFilterConditions = conditions.len() as u32;
        filter.filterCondition = if conditions.is_empty() {
            ptr::null_mut()
        } else {
            conditions.as_mut_ptr()
        };
        filter.action.r#type = if permit {
            FWP_ACTION_PERMIT
        } else {
            FWP_ACTION_BLOCK
        };
        check(
            FwpmFilterAdd0(engine, &filter, ptr::null_mut(), ptr::null_mut()),
            "Adding a firewall filter",
        )
    }
    /// Everything that may leave while blocking, then the block, on the layers where outbound
    /// connections are authorised.
    unsafe fn install(engine: HANDLE, core: &Path, bypass: &[String]) -> Result<()> {
        let mut name = wide(std::ffi::OsStr::new("Airport TUN kill switch"));
        let mut sublayer: FWPM_SUBLAYER0 = std::mem::zeroed();
        sublayer.subLayerKey = SUBLAYER;
        sublayer.displayData.name = name.as_mut_ptr();
        sublayer.weight = u16::MAX;
        check(
            FwpmSubLayerAdd0(engine, &sublayer, ptr::null_mut()),
            "Adding the firewall sublayer",
        )?;
        let v4 = FWPM_LAYER_ALE_AUTH_CONNECT_V4;
        let v6 = FWPM_LAYER_ALE_AUTH_CONNECT_V6;
        for layer in [v4, v6] {
            add(
                engine,
                layer,
                &mut [equals_u32(
                    FWPM_CONDITION_FLAGS,
                    FWP_MATCH_FLAGS_ALL_SET,
                    FWP_CONDITION_FLAG_IS_LOOPBACK,
                )],
                true,
            )?;
        }
        // Traffic already inside the tunnel.
        let tunnel_v4: Ipv4Addr = airport_tun_service::TUN_ADDRESS_V4
            .parse()
            .map_err(|_| "Invalid tunnel address")?;
        add(
            engine,
            v4,
            &mut [equals_u32(
                FWPM_CONDITION_IP_LOCAL_ADDRESS,
                FWP_MATCH_EQUAL,
                u32::from(tunnel_v4),
            )],
            true,
        )?;
        let tunnel_v6: Ipv6Addr = airport_tun_service::TUN_ADDRESS_V6
            .parse()
            .map_err(|_| "Invalid tunnel address")?;
        let mut tunnel_v6 = FWP_BYTE_ARRAY16 {
            byteArray16: tunnel_v6.octets(),
        };
        let mut local_v6 = condition(FWPM_CONDITION_IP_LOCAL_ADDRESS, FWP_MATCH_EQUAL);
        local_v6.conditionValue.r#type = FWP_BYTE_ARRAY16_TYPE;
        local_v6.conditionValue.Anonymous.byteArray16 = &mut tunnel_v6;
        add(engine, v6, &mut [local_v6], true)?;
        // The core, so a reconnect reaches the proxy servers.
        let path = wide(core.as_os_str());
        let mut application: *mut FWP_BYTE_BLOB = ptr::null_mut();
        check(
            FwpmGetAppIdFromFileName0(path.as_ptr(), &mut application),
            "Identifying the core for the firewall",
        )?;
        let mut by_application = condition(FWPM_CONDITION_ALE_APP_ID, FWP_MATCH_EQUAL);
        by_application.conditionValue.r#type = FWP_BYTE_BLOB_TYPE;
        by_application.conditionValue.Anonymous.byteBlob = application;
        let permitted = add(engine, v4, &mut [by_application], true)
            .and_then(|_| add(engine, v6, &mut [by_application], true));
        FwpmFreeMemory0((&mut application as *mut *mut FWP_BYTE_BLOB).cast());
        permitted?;
        // Address assignment.
        add(
            engine,
            v4,
            &mut [
                equals_u8(FWPM_CONDITION_IP_PROTOCOL, UDP),
                equals_u16(FWPM_CONDITION_IP_LOCAL_PORT, 68),
                equals_u16(FWPM_CONDITION_IP_REMOTE_PORT, 67),
            ],
            true,
        )?;
        add(
            engine,
            v6,
            &mut [
                equals_u8(FWPM_CONDITION_IP_PROTOCOL, UDP),
                equals_u16(FWPM_CONDITION_IP_LOCAL_PORT, 546),
                equals_u16(FWPM_CONDITION_IP_REMOTE_PORT, 547),
            ],
            true,
        )?;
        // Destinations the user keeps off the tunnel.
        for prefix in bypass {
            let Some((address, length)) = prefix.split_once('/') else {
                continue;
            };
            let (Ok(address), Ok(length)) = (address.parse::<IpAddr>(), length.parse::<u8>())
            else {
                continue;
            };
            match address {
                IpAddr::V4(address) if length <= 32 => {
                    let mut range = FWP_V4_ADDR_AND_MASK {
                        addr: u32::from(address),
                        mask: if length == 0 {
                            0
                        } else {
                            u32::MAX << (32 - u32::from(length))
                        },
                    };
                    let mut to = condition(FWPM_CONDITION_IP_REMOTE_ADDRESS, FWP_MATCH_EQUAL);
                    to.conditionValue.r#type = FWP_V4_ADDR_MASK;
                    to.conditionValue.Anonymous.v4AddrMask = &mut range;
                    add(engine, v4, &mut [to], true)?;
                }
                IpAddr::V6(address) if length <= 128 => {
                    let mut range = FWP_V6_ADDR_AND_MASK {
                        addr: address.octets(),
                        prefixLength: length,
                    };
                    let mut to = condition(FWPM_CONDITION_IP_REMOTE_ADDRESS, FWP_MATCH_EQUAL);
                    to.conditionValue.r#type = FWP_V6_ADDR_MASK;
                    to.conditionValue.Anonymous.v6AddrMask = &mut range;
                    add(engine, v6, &mut [to], true)?;
                }
                _ => {}
            }
        }
        add(engine, v4, &mut [], false)?;
        add(engine, v6, &mut [], false)
    }
    impl crate::Guard for SystemGuard {
        fn engage(&mut self, bypass: &[String]) -> Result<()> {
            self.release()?;
            unsafe {
                let mut session: FWPM_SESSION0 = std::mem::zeroed();
                session.flags = FWPM_SESSION_FLAG_DYNAMIC;
                let mut engine: HANDLE = ptr::null_mut();
                check(
                    FwpmEngineOpen0(
                        ptr::null(),
                        RPC_C_AUTHN_WINNT,
                        ptr::null(),
                        &session,
                        &mut engine,
                    ),
                    "Opening the filtering engine",
                )?;
                if let Err(error) = install(engine, &self.core, bypass) {
                    FwpmEngineClose0(engine);
                    return Err(error);
                }
                self.engine = engine;
            }
            Ok(())
        }
        fn release(&mut self) -> Result<()> {
            if self.engine.is_null() {
                return Ok(());
            }
            let code = unsafe { FwpmEngineClose0(self.engine) };
            self.engine = ptr::null_mut();
            check(code, "Closing the filtering engine")
        }
    }
}

pub fn system_guard(root: &Path) -> Box<dyn crate::Guard> {
    Box::new(guard::new(root))
}
