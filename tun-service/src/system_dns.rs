//! Points the machine's resolver at the tunnel while it carries traffic, and gives it back.
//!
//! Why this exists: `dns_mode: hijack` only takes over DNS packets that enter the device, and
//! macOS keeps asking the resolver its DHCP lease named — a router address on the local
//! segment, which never enters the device. Left alone, every name is resolved by the ISP
//! resolver: blocked names come back poisoned (an address that belongs to someone else), the
//! core dials that address through the node, and the site cannot answer. Pointing the system
//! resolver at the device makes every lookup land in the core, where fake-IP answers and the
//! node resolves the real name on its own side. This is the model Clash Verge / mihomo use.
//!
//! The record of what the resolver was before lives in `dns-backup.json` (mode 0600) so a
//! service that dies at the wrong moment can still restore it on its next start.
use crate::{secure_write, Result, SystemDns};
use airport_tun_service::TUN_ADDRESS_V4;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// How long the device may take to appear before the override is refused: writing a resolver
/// that points at a device which is not up yet would black out every lookup.
#[cfg(target_os = "macos")]
const READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    /// Network service name -> the resolvers it had before the tunnel took them.
    services: BTreeMap<String, Vec<String>>,
}

pub struct Platform {
    root: PathBuf,
}

impl Platform {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn path(&self) -> PathBuf {
        self.root.join("dns-backup.json")
    }

    fn saved(&self) -> Result<Saved> {
        match std::fs::read(self.path()) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| "native.tunDnsRecordCorrupt".to_owned())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Saved::default()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn write_saved(&self, saved: &Saved) -> Result<()> {
        let bytes = serde_json::to_vec(saved).map_err(|e| e.to_string())?;
        secure_write(&self.path(), &bytes, 0o600)
    }

    /// Records every enabled service that does not already point at the tunnel. Names that
    /// appeared since the tunnel came up (a cable plugged in) are picked up here too.
    fn snapshot(&self, saved: &mut Saved) -> Result<()> {
        for service in self.services()? {
            if saved.services.contains_key(&service) {
                continue;
            }
            let before = self.current(&service)?;
            if before.as_slice() != [TUN_ADDRESS_V4] {
                saved.services.insert(service, before);
            }
        }
        Ok(())
    }

    fn point(&self, saved: &Saved) -> Result<()> {
        for service in saved.services.keys() {
            self.set(service, &[TUN_ADDRESS_V4.to_owned()])?;
        }
        Ok(())
    }
}

impl SystemDns for Platform {
    fn apply(&mut self) -> Result<()> {
        self.wait_ready()?;
        // A record left by a crash describes resolvers a reboot may already have replaced.
        self.restore().ok();
        let mut saved = Saved::default();
        self.snapshot(&mut saved)?;
        if saved.services.is_empty() {
            return Ok(());
        }
        // Persisted before the first change: from here on, any exit can give the resolvers back.
        self.write_saved(&saved)?;
        if let Err(error) = self.point(&saved) {
            let _ = self.restore();
            return Err(error);
        }
        flush_cache();
        Ok(())
    }

    fn refresh(&mut self) -> Result<()> {
        let mut saved = self.saved()?;
        if saved.services.is_empty() {
            return self.apply();
        }
        let known = saved.services.len();
        self.snapshot(&mut saved)?;
        if saved.services.len() != known {
            self.write_saved(&saved)?;
        }
        let mut changed = false;
        for (service, before) in &saved.services {
            let current = match self.current(service) {
                Ok(current) => current,
                Err(_) => continue,
            };
            // Already ours, or taken by someone else since: nothing to put in place.
            if current.as_slice() == [TUN_ADDRESS_V4] || current != *before {
                continue;
            }
            self.set(service, &[TUN_ADDRESS_V4.to_owned()])?;
            changed = true;
        }
        if changed {
            flush_cache();
        }
        Ok(())
    }

    fn restore(&mut self) -> Result<()> {
        let saved = self.saved()?;
        if saved.services.is_empty() {
            return Ok(());
        }
        for (service, before) in &saved.services {
            // Only a resolver that still points at the tunnel is ours to take back: a guest
            // VPN that changed it since owns it now.
            let ours =
                matches!(self.current(service), Ok(now) if now.as_slice() == [TUN_ADDRESS_V4]);
            if !ours {
                continue;
            }
            self.set(service, before)?;
        }
        flush_cache();
        match std::fs::remove_file(self.path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(target_os = "macos")]
impl Platform {
    /// The device answers only once its address is assigned; DNS pointing at it before then
    /// fails every lookup.
    fn wait_ready(&self) -> Result<()> {
        let deadline = std::time::Instant::now() + READY_TIMEOUT;
        loop {
            if output("/sbin/ifconfig")
                .is_ok_and(|text| text.contains(&format!("inet {TUN_ADDRESS_V4} ")))
            {
                return Ok(());
            }
            if std::time::Instant::now() > deadline {
                return Err("native.tunDnsDeviceNotReady".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    fn services(&self) -> Result<Vec<String>> {
        Ok(parse_services(&run(&["-listallnetworkservices"])?))
    }

    fn current(&self, service: &str) -> Result<Vec<String>> {
        Ok(parse_servers(&run(&["-getdnsservers", service])?))
    }

    fn set(&self, service: &str, servers: &[String]) -> Result<()> {
        let mut args = vec!["-setdnsservers", service];
        if servers.is_empty() {
            args.push("Empty");
        } else {
            args.extend(servers.iter().map(String::as_str));
        }
        run(&args).map(|_| ())
    }
}

#[cfg(target_os = "macos")]
const NETWORKSETUP: &str = "/usr/sbin/networksetup";

/// Runs `networksetup` with the given arguments.
#[cfg(target_os = "macos")]
fn run(args: &[&str]) -> Result<String> {
    let mut command = std::process::Command::new(NETWORKSETUP);
    command.args(args);
    command_with(&mut command, args)
}

#[cfg(target_os = "macos")]
fn output(program: &str) -> Result<String> {
    command_with(&mut std::process::Command::new(program), &[])
}

/// Runs a prepared command and returns its standard output, or an error naming it.
#[cfg(target_os = "macos")]
fn command_with(command: &mut std::process::Command, args: &[&str]) -> Result<String> {
    let output = command.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "{}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `networksetup -listallnetworkservices`: a header, then one service per line. A leading `*`
/// marks a disabled service, which has no resolver to take.
#[cfg(any(target_os = "macos", test))]
fn parse_services(text: &str) -> Vec<String> {
    text.lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('*'))
        .map(str::to_owned)
        .collect()
}

/// `networksetup -getdnsservers`: either "There aren't any DNS Servers set on X." or one
/// address per line. Anything that is not an address is noise, whatever language it is in.
#[cfg(any(target_os = "macos", test))]
fn parse_servers(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.parse::<std::net::IpAddr>().is_ok())
        .map(str::to_owned)
        .collect()
}

/// Both caches must go: the resolver daemon keeps answers that predate the change.
#[cfg(target_os = "macos")]
fn flush_cache() {
    let _ = std::process::Command::new("/usr/bin/dscacheutil")
        .arg("-flushcache")
        .output();
    let _ = std::process::Command::new("/usr/bin/killall")
        .args(["-HUP", "mDNSResponder"])
        .output();
}

#[cfg(not(target_os = "macos"))]
fn flush_cache() {}

/// Linux and Windows are not covered yet: their resolvers are configured separately
/// (`resolvectl` / `netsh`) and neither is exercised by this client's test machines. Until
/// they are, the tunnel behaves as it did before — no resolver is taken from the system.
#[cfg(not(target_os = "macos"))]
impl Platform {
    fn wait_ready(&self) -> Result<()> {
        Ok(())
    }

    fn services(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn current(&self, _service: &str) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn set(&self, _service: &str, _servers: &[String]) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_skip_the_header_and_disabled_entries() {
        let text = "An asterisk (*) denotes that a network service is disabled.\nHUAWEI_MOBILE\nThunderbolt Bridge\nWi-Fi\niPhone USB\n*Bluetooth PAN\n";
        assert_eq!(
            parse_services(text),
            vec![
                "HUAWEI_MOBILE".to_owned(),
                "Thunderbolt Bridge".to_owned(),
                "Wi-Fi".to_owned(),
                "iPhone USB".to_owned(),
            ]
        );
    }

    #[test]
    fn resolvers_parse_in_any_language() {
        // The "none set" sentence is localised, so only addresses are taken from it.
        assert!(parse_servers("There aren't any DNS Servers set on Wi-Fi.\n").is_empty());
        assert!(parse_servers("Auf Wi-Fi sind keine DNS-Server eingestellt.\n").is_empty());
        assert_eq!(
            parse_servers("192.168.31.1\n223.5.5.5\n"),
            vec!["192.168.31.1".to_owned(), "223.5.5.5".to_owned()]
        );
        assert_eq!(
            parse_servers("192.168.31.1\n2001:4860:4860::8888\n"),
            vec!["192.168.31.1".to_owned(), "2001:4860:4860::8888".to_owned()]
        );
    }

    #[test]
    fn a_missing_record_restores_nothing() {
        let dir = std::env::temp_dir().join(format!(
            "airport-dns-test-{}",
            airport_tun_service::random_id().unwrap()
        ));
        std::fs::create_dir(&dir).unwrap();
        let mut platform = Platform::new(&dir);
        assert!(platform.saved().unwrap().services.is_empty());
        assert!(platform.restore().is_ok());
        assert!(!platform.path().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
