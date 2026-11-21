//! Read connected LAN IPv4 addresses without routing a probe through a VPN.
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    net::Ipv4Addr,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
const READ_FAILED: &str = "native.networkInfoReadFailed";
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct LanAddress {
    pub interface: String,
    pub ip: String,
}

fn usable_interface(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !name.is_empty()
        && name != "lo"
        && ![
            "lo0",
            "loopback",
            "utun",
            "tun",
            "tap",
            "wg",
            "tailscale",
            "docker",
            "veth",
            "vethernet",
            "virbr",
            "vmnet",
            "vbox",
            "bridge",
            "br-",
            "awdl",
            "llw",
            "gif",
            "stf",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}
fn normalize(mut addresses: Vec<LanAddress>) -> Vec<LanAddress> {
    addresses.retain(|entry| {
        usable_interface(&entry.interface)
            && entry.ip.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_private())
    });
    addresses.sort_by(|a, b| (&a.interface, &a.ip).cmp(&(&b.interface, &b.ip)));
    addresses.dedup_by(|a, b| a.ip == b.ip && a.interface == b.interface);
    addresses.truncate(16);
    addresses
}
fn command(program: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(|_| READ_FAILED)?;
    let stdout = child.stdout.take().ok_or(READ_FAILED)?;
    let (send, recv) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(128 * 1024)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = send.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(READ_FAILED.into());
                }
                let bytes = recv
                    .recv_timeout(Duration::from_millis(500))
                    .map_err(|_| READ_FAILED)?
                    .map_err(|_| READ_FAILED)?;
                return String::from_utf8(bytes).map_err(|_| READ_FAILED.into());
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(READ_FAILED.into());
            }
        }
    }
}
#[cfg(any(target_os = "macos", test))]
fn parse_macos(output: &str) -> Vec<LanAddress> {
    let (mut name, mut up, mut ethernet, mut active) = ("", false, false, false);
    let mut ips: Vec<String> = Vec::new();
    let mut entries = Vec::new();
    for line in output.lines().chain(std::iter::once("")) {
        if !line.starts_with(char::is_whitespace) {
            if up && ethernet && active {
                entries.extend(ips.drain(..).map(|ip| LanAddress {
                    interface: name.to_owned(),
                    ip,
                }));
            }
            ips.clear();
            name = line.split(':').next().unwrap_or("");
            up = line.split('<').nth(1).is_some_and(|flags| {
                flags.split([',', '>']).any(|flag| flag == "UP") && !flags.contains("POINTOPOINT")
            });
            ethernet = false;
            active = false;
        } else {
            let line = line.trim();
            ethernet |= line.starts_with("ether ");
            active |= line == "status: active";
            if let Some(ip) = line
                .strip_prefix("inet ")
                .and_then(|s| s.split_whitespace().next())
            {
                ips.push(ip.into());
            }
        }
    }
    normalize(entries)
}
#[cfg(any(target_os = "linux", test))]
fn parse_linux(output: &str) -> Result<Vec<LanAddress>, String> {
    let data: serde_json::Value = serde_json::from_str(output).map_err(|_| READ_FAILED)?;
    let rows = data.as_array().ok_or(READ_FAILED)?;
    let mut entries = Vec::new();
    for row in rows {
        let Some(name) = row["ifname"].as_str() else {
            continue;
        };
        if row["link_type"] != "ether"
            || !row["flags"].as_array().is_some_and(|flags| {
                flags.iter().any(|f| f == "UP") && !flags.iter().any(|f| f == "POINTOPOINT")
            })
        {
            continue;
        }
        for address in row["addr_info"].as_array().into_iter().flatten() {
            if address["family"] == "inet" && address["scope"] == "global" {
                if let Some(ip) = address["local"].as_str() {
                    entries.push(LanAddress {
                        interface: name.into(),
                        ip: ip.into(),
                    });
                }
            }
        }
    }
    Ok(normalize(entries))
}
#[cfg(any(windows, test))]
fn parse_windows(output: &str) -> Result<Vec<LanAddress>, String> {
    if output.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value =
        serde_json::from_str(output.trim_start_matches('\u{feff}')).map_err(|_| READ_FAILED)?;
    let entries = match value {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::Array(values) => values,
        value => vec![value],
    };
    let entries = entries
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<LanAddress>, _>>()
        .map_err(|_| READ_FAILED)?;
    Ok(normalize(entries))
}
pub fn discover() -> Result<Vec<LanAddress>, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(parse_macos(&command("/sbin/ifconfig", &["-a"])?))
    }
    #[cfg(target_os = "linux")]
    {
        parse_linux(&command(
            "ip",
            &["-j", "-4", "address", "show", "up", "scope", "global"],
        )?)
    }
    #[cfg(windows)]
    {
        // Without -All, this cmdlet returns connected, non-virtual adapters only.
        let script = "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); $ErrorActionPreference='Stop'; @(Get-NetIPConfiguration | ForEach-Object { $name=$_.InterfaceAlias; $_.IPv4Address | ForEach-Object { [pscustomobject]@{interface=$name;ip=$_.IPAddress} } }) | ConvertTo-Json -Compress";
        parse_windows(&command(
            "powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command", script],
        )?)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Err(READ_FAILED.into())
    }
}
#[tauri::command]
pub async fn get_lan_addresses() -> Result<Vec<LanAddress>, String> {
    tauri::async_runtime::spawn_blocking(discover)
        .await
        .map_err(|_| READ_FAILED.to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macos_filters_disconnected_and_virtual_interfaces() {
        let raw = "en0: flags=8863<UP,BROADCAST,RUNNING,MULTICAST>\n\tinet 192.168.1.10 netmask 0xffffff00\n\tether aa:bb:cc:dd:ee:ff\n\tstatus: active\nen1: flags=8863<UP,BROADCAST,RUNNING>\n\tinet 10.0.0.2 netmask 0xffffff00\n\tether aa:bb:cc:dd:ee:ff\n\tstatus: inactive\nutun0: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST>\n\tinet 172.19.0.1\n\tstatus: active\nbridge0: flags=8863<UP,BROADCAST,RUNNING>\n\tinet 192.168.64.1\n\tether aa:bb:cc:dd:ee:ff\n\tstatus: active";
        assert_eq!(
            parse_macos(raw),
            vec![LanAddress {
                interface: "en0".into(),
                ip: "192.168.1.10".into()
            }]
        );
    }
    #[test]
    fn linux_only_returns_up_ethernet_lan_addresses() {
        let raw = r#"[{"ifname":"eth0","flags":["UP","BROADCAST"],"link_type":"ether","addr_info":[{"family":"inet","scope":"global","local":"10.0.0.2"},{"family":"inet","scope":"global","local":"10.0.0.2"}]},{"ifname":"docker0","flags":["UP"],"link_type":"ether","addr_info":[{"family":"inet","scope":"global","local":"172.17.0.1"}]},{"ifname":"tun0","flags":["UP","POINTOPOINT"],"link_type":"none","addr_info":[{"family":"inet","scope":"global","local":"172.19.0.1"}]},{"ifname":"wlan0","flags":["BROADCAST"],"link_type":"ether","addr_info":[{"family":"inet","scope":"global","local":"192.168.1.2"}]}]"#;
        assert_eq!(
            parse_linux(raw).unwrap(),
            vec![LanAddress {
                interface: "eth0".into(),
                ip: "10.0.0.2".into()
            }]
        );
        assert!(parse_linux("bad").is_err());
    }
    #[test]
    fn windows_handles_single_multiple_empty_and_non_lan_results() {
        assert_eq!(
            parse_windows(r#"{"interface":"Local Area Connection","ip":"10.0.0.2"}"#)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            parse_windows(r#"{"interface":"Wi-Fi","ip":"192.168.31.38"}"#)
                .unwrap()
                .len(),
            1
        );
        let raw = r#"[{"interface":"Wi-Fi","ip":"192.168.31.38"},{"interface":"vEthernet (WSL)","ip":"172.29.1.1"},{"interface":"Loopback","ip":"127.0.0.1"},{"interface":"Ethernet","ip":"169.254.1.2"},{"interface":"Ethernet","ip":"8.8.8.8"}]"#;
        assert_eq!(parse_windows(raw).unwrap().len(), 1);
        assert!(parse_windows("").unwrap().is_empty());
        assert!(parse_windows("null").unwrap().is_empty());
        assert!(parse_windows("bad").is_err());
    }
    #[test]
    #[cfg(target_os = "macos")]
    fn live_macos_discovery_reads_only_interface_configuration() {
        let addresses = discover().unwrap();
        assert!(addresses
            .iter()
            .all(|entry| entry.ip.parse::<Ipv4Addr>().unwrap().is_private()));
        eprintln!("LAN interfaces: {addresses:?}");
    }
}
