// Portable command-based system proxy integration. Keep an on-disk rollback
// before the first mutation so a partial failure or crash can be recovered.
use super::message::{msg, text};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command};
#[derive(Serialize, Deserialize)]
pub struct Backup {
    #[serde(default = "super::network_settings::default_proxy_port")]
    owned_port: u16,
    restore: Vec<Vec<String>>,
}
fn run(args: &[String]) -> Result<String, String> {
    let output = Command::new(&args[0])
        .args(&args[1..])
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.contains("Error:") || value.contains("requires admin") {
        return Err(value);
    }
    Ok(value)
}
fn command(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}
pub fn restore(backup: Backup) -> Result<(), String> {
    // Recovery files are data, never arbitrary commands supplied to the shell/OS.
    if !backup.restore.iter().all(|args| valid_restore(args)) {
        return Err("native.proxyBackupInvalid".into());
    }
    let mut failures = Vec::new();
    #[cfg(target_os = "macos")]
    let superseded = superseded_services(&backup)?;
    #[cfg(target_os = "linux")]
    if linux_proxy_superseded(&backup)? {
        return Ok(());
    }
    for args in backup.restore {
        #[cfg(target_os = "macos")]
        if superseded.contains(&args[2]) {
            continue;
        }
        // One bounded retry covers transient network-service failures.
        if let Err(e) = run(&args).or_else(|_| run(&args)) {
            failures.push(e);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}
#[cfg(target_os = "macos")]
fn superseded_services(backup: &Backup) -> Result<std::collections::HashSet<String>, String> {
    let mut services = std::collections::HashSet::new();
    for args in &backup.restore {
        if args.len() != 5 {
            continue;
        }
        let get = args[1].replacen("-set", "-get", 1);
        let current = run(&command(&["/usr/sbin/networksetup", &get, &args[2]]))?;
        let field = |name| {
            current
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .unwrap_or("")
                .trim()
        };
        let owned =
            field("Server:") == "127.0.0.1" && field("Port:") == backup.owned_port.to_string();
        let original = field("Server:") == args[3] && field("Port:") == args[4];
        // Do not undo a different proxy client's changes made while Airport ran.
        if !owned && !original {
            services.insert(args[2].clone());
        }
    }
    Ok(services)
}
#[cfg(target_os = "linux")]
fn linux_proxy_superseded(backup: &Backup) -> Result<bool, String> {
    let port = backup.owned_port.to_string();
    for args in &backup.restore {
        let current = run(&command(&["gsettings", "get", &args[2], &args[3]]))?;
        let ours = match args[3].as_str() {
            "host" => "'127.0.0.1'",
            "port" => &port,
            _ => "'manual'",
        };
        if current != ours && current != args[4] {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Read the current proxy so it can be restored. Does not change anything, so it can run
/// while the node connectivity check is still in flight.
pub fn prepare_enable(port: u16) -> Result<(Backup, Vec<Vec<String>>), String> {
    prepare(port)
}

/// Write the rollback file, then apply a snapshot from [prepare_enable].
pub fn commit_enable(
    path: &Path,
    backup: Backup,
    changes: Vec<Vec<String>>,
) -> Result<Backup, String> {
    let port = backup.owned_port;
    super::storage::write_private(
        path,
        &serde_json::to_vec(&backup).map_err(|e| e.to_string())?,
    )?;
    let mut last_error = String::new();
    for _ in 0..2 {
        let result = changes
            .iter()
            .try_for_each(|args| run(args).map(|_| ()))
            .and_then(|_| verify(port));
        match result {
            Ok(()) => return Ok(backup),
            Err(error) => last_error = error,
        }
    }
    Err(msg(
        "native.systemProxyApplyFailed",
        &[("error", text(last_error))],
    ))
}

pub fn enable(path: &Path, port: u16) -> Result<Backup, String> {
    let (backup, changes) = prepare_enable(port)?;
    commit_enable(path, backup, changes)
}

/// Inspect through the same platform adapter used to create rollback snapshots.
/// Reading never mutates the user's current proxy configuration.
pub fn diagnose() -> Result<String, String> {
    let (backup, _) = prepare(super::network_settings::DEFAULT_PROXY_PORT)?;
    #[cfg(target_os = "macos")]
    let enabled = backup
        .restore
        .iter()
        .any(|args| args.len() == 4 && args[3] == "on");
    #[cfg(target_os = "linux")]
    let enabled = backup
        .restore
        .iter()
        .any(|args| args[3] == "mode" && args[4] != "'none'");
    Ok(if enabled {
        "native.systemProxyEnabled"
    } else {
        "native.systemProxyDisabled"
    }
    .into())
}

fn verify(port: u16) -> Result<(), String> {
    let (backup, _) = prepare(port)?;
    if installed_values_match(&backup, port) {
        Ok(())
    } else {
        Err("native.systemProxyMismatch".into())
    }
}

fn installed_values_match(backup: &Backup, port: u16) -> bool {
    let port_text = port.to_string();
    backup.restore.iter().all(|args| {
        #[cfg(target_os = "macos")]
        {
            if args.len() == 5 {
                args[3] == "127.0.0.1" && args[4] == port_text
            } else if matches!(
                args[1].as_str(),
                "-setautoproxystate" | "-setproxyautodiscovery"
            ) {
                args[3] == "off"
            } else {
                args[3] == "on"
            }
        }
        #[cfg(target_os = "linux")]
        {
            args[4]
                == match args[3].as_str() {
                    "host" => "'127.0.0.1'",
                    "port" => &port_text,
                    _ => "'manual'",
                }
        }
    })
}

#[cfg(target_os = "macos")]
fn valid_restore(args: &[String]) -> bool {
    if args.first().map(String::as_str) != Some("/usr/sbin/networksetup") || args.len() < 4 {
        return false;
    }
    match args[1].as_str() {
        "-setwebproxy" | "-setsecurewebproxy" | "-setsocksfirewallproxy" => {
            args.len() == 5 && args[4].parse::<u16>().is_ok()
        }
        "-setwebproxystate"
        | "-setsecurewebproxystate"
        | "-setsocksfirewallproxystate"
        | "-setautoproxystate"
        | "-setproxyautodiscovery" => args.len() == 4 && matches!(args[3].as_str(), "on" | "off"),
        _ => false,
    }
}
#[cfg(target_os = "linux")]
fn valid_restore(args: &[String]) -> bool {
    args.len() == 5
        && args[0] == "gsettings"
        && args[1] == "set"
        && match args[2].as_str() {
            "org.gnome.system.proxy" => {
                args[3] == "mode" && matches!(args[4].as_str(), "'none'" | "'manual'" | "'auto'")
            }
            "org.gnome.system.proxy.http"
            | "org.gnome.system.proxy.https"
            | "org.gnome.system.proxy.socks" => {
                args[3] == "host" || (args[3] == "port" && args[4].parse::<u16>().is_ok())
            }
            _ => false,
        }
}
#[cfg(target_os = "macos")]
fn prepare(port: u16) -> Result<(Backup, Vec<Vec<String>>), String> {
    let port_text = port.to_string();
    let tool = "/usr/sbin/networksetup";
    let services = run(&command(&[tool, "-listallnetworkservices"]))?;
    let mut restore = Vec::new();
    let mut changes = Vec::new();
    for service in services
        .lines()
        .skip(1)
        .filter(|s| !s.starts_with('*') && !s.is_empty())
    {
        for kind in ["autoproxy", "proxyautodiscovery"] {
            let get = if kind == "autoproxy" {
                "-getautoproxyurl"
            } else {
                "-getproxyautodiscovery"
            };
            let current = run(&command(&[tool, get, service]))?;
            let enabled = current.lines().any(|line| {
                line.trim() == "Enabled: Yes" || line.trim() == "Auto Proxy Discovery: On"
            });
            let set = if kind == "autoproxy" {
                "-setautoproxystate"
            } else {
                "-setproxyautodiscovery"
            };
            restore.push(command(&[
                tool,
                set,
                service,
                if enabled { "on" } else { "off" },
            ]));
            changes.push(command(&[tool, set, service, "off"]));
        }
        for kind in ["webproxy", "securewebproxy", "socksfirewallproxy"] {
            let current = run(&command(&[tool, &format!("-get{kind}"), service]))?;
            let field = |key: &str| {
                current
                    .lines()
                    .find_map(|l| l.strip_prefix(key))
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            // Authenticated proxy passwords are not retrievable; do not overwrite them.
            if field("Authenticated Proxy Enabled:") == "1" {
                return Err(msg(
                    "native.authenticatedProxyInUse",
                    &[("service", text(service))],
                ));
            }
            let server = field("Server:");
            let port = field("Port:");
            let set = format!("-set{kind}");
            let state = format!("-set{kind}state");
            restore.push(command(&[
                tool,
                &set,
                service,
                &server,
                if port.is_empty() { "0" } else { &port },
            ]));
            restore.push(command(&[
                tool,
                &state,
                service,
                if field("Enabled:") == "Yes" {
                    "on"
                } else {
                    "off"
                },
            ]));
            changes.push(command(&[tool, &set, service, "127.0.0.1", &port_text]));
            changes.push(command(&[tool, &state, service, "on"]));
        }
    }
    if changes.is_empty() {
        return Err("native.noNetworkServices".into());
    }
    Ok((
        Backup {
            owned_port: port,
            restore,
        },
        changes,
    ))
}
#[cfg(target_os = "linux")]
fn prepare(port: u16) -> Result<(Backup, Vec<Vec<String>>), String> {
    let port_text = port.to_string();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_uppercase();
    if !desktop.contains("GNOME") && !desktop.contains("UNITY") {
        return Err("native.desktopNeedsManualProxy".into());
    }
    let mut restore = Vec::new();
    let mut changes = Vec::new();
    for (schema, key, value) in [
        ("org.gnome.system.proxy.http", "host", "'127.0.0.1'"),
        ("org.gnome.system.proxy.http", "port", port_text.as_str()),
        ("org.gnome.system.proxy.https", "host", "'127.0.0.1'"),
        ("org.gnome.system.proxy.https", "port", port_text.as_str()),
        ("org.gnome.system.proxy.socks", "host", "'127.0.0.1'"),
        ("org.gnome.system.proxy.socks", "port", port_text.as_str()),
        ("org.gnome.system.proxy", "mode", "'manual'"),
    ] {
        let old = run(&command(&["gsettings", "get", schema, key]))?;
        restore.push(command(&["gsettings", "set", schema, key, &old]));
        changes.push(command(&["gsettings", "set", schema, key, value]));
    }
    Ok((
        Backup {
            owned_port: port,
            restore,
        },
        changes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_recovery_defaults_to_original_port() {
        let backup: Backup = serde_json::from_str(r#"{"restore":[]}"#).unwrap();
        assert_eq!(backup.owned_port, 2080);
    }

    #[test]
    fn recovery_rejects_arbitrary_programs() {
        assert!(!valid_restore(&command(&[
            "/bin/sh",
            "-c",
            "echo unsafe",
            "extra"
        ])));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn verifies_custom_port_and_proxy_state() {
        let mut backup = Backup {
            owned_port: 3456,
            restore: vec![
                command(&[
                    "/usr/sbin/networksetup",
                    "-setwebproxy",
                    "Wi-Fi",
                    "127.0.0.1",
                    "3456",
                ]),
                command(&["/usr/sbin/networksetup", "-setwebproxystate", "Wi-Fi", "on"]),
            ],
        };
        assert!(installed_values_match(&backup, 3456));
        assert!(!installed_values_match(&backup, 2080));
        backup.restore[1][3] = "off".into();
        assert!(!installed_values_match(&backup, 3456));
    }
    #[test]
    #[ignore = "temporarily changes OS proxy; explicit local QA only"]
    fn system_proxy_roundtrip_and_startup_recovery() {
        assert_eq!(
            std::env::var("AIRPORT_ALLOW_SYSTEM_PROXY_TEST").as_deref(),
            Ok("1")
        );
        let port = 34567;
        let (before, _) = prepare(port).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("proxy-backup.json");
        struct Recovery(std::path::PathBuf);
        impl Drop for Recovery {
            fn drop(&mut self) {
                if let Ok(raw) = std::fs::read(&self.0) {
                    if let Ok(backup) = serde_json::from_slice::<Backup>(&raw) {
                        let _ = restore(backup);
                    }
                }
            }
        }
        let _recovery = Recovery(path.clone());
        enable(&path, port).unwrap();
        verify(port).unwrap();
        // A fresh Core represents startup after an interrupted process.
        let mut fresh = crate::Core::new();
        fresh.proxy_backup_path = Some(path.clone());
        crate::restore_saved_proxy(&mut fresh).unwrap();
        assert!(!path.exists());
        let (after, _) = prepare(port).unwrap();
        assert_eq!(before.restore, after.restore);
        // Repeated cleanup must be a no-op and preserve the restored values.
        crate::restore_saved_proxy(&mut fresh).unwrap();
    }
}
