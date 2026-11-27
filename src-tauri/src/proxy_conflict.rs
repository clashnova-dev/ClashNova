//! Before connect, see whether a system proxy or a TUN interface is already on.
//! This reads the network configuration only. It does not look at running programs,
//! and it does not change the system proxy.
//!
//! `own_port` is this app's local proxy, so a leftover `127.0.0.1:<own_port>` is ignored.

use std::process::Command;
use std::thread;

/// Endpoints that are already on, or nothing when both the system proxy and TUN are off.
/// The two reads do not depend on each other, so they run together.
pub fn detail(own_port: u16) -> Option<String> {
    thread::scope(|scope| {
        let system = scope.spawn(move || system_proxy(own_port));
        let tun = scope.spawn(tun_interfaces);
        let mut items = system.join().unwrap_or_default();
        items.extend(tun.join().unwrap_or_default());
        items.sort();
        items.dedup();
        items.truncate(4);
        if items.is_empty() {
            None
        } else {
            Some(items.join("; "))
        }
    })
}

fn system_proxy(own_port: u16) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        macos_system_proxies(own_port)
    }
    #[cfg(windows)]
    {
        windows_system_proxy(own_port)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux_system_proxy(own_port)
    }
}

fn tun_interfaces() -> Vec<String> {
    #[cfg(windows)]
    let args = ["ipconfig"];
    #[cfg(target_os = "macos")]
    let args = ["ifconfig", "-a"];
    #[cfg(all(unix, not(target_os = "macos")))]
    let args = ["ip", "-4", "-o", "addr", "show"];
    command_output(&args)
        .map(|text| parse_tun(&text))
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn macos_system_proxies(own_port: u16) -> Vec<String> {
    // One snapshot of the active proxy. Per-service `networksetup` calls are what made connect slow.
    let Ok(text) = command_output(&["/usr/sbin/scutil", "--proxy"]) else {
        return Vec::new();
    };
    parse_scutil_proxy(&text, own_port)
}

#[cfg(windows)]
fn windows_system_proxy(own_port: u16) -> Vec<String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
    else {
        return Vec::new();
    };
    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    if enabled == 0 {
        return Vec::new();
    }
    let server: String = key.get_value("ProxyServer").unwrap_or_default();
    let own = format!("127.0.0.1:{own_port}");
    if server.is_empty() || server == own || server == format!("localhost:{own_port}") {
        Vec::new()
    } else {
        vec![server]
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_system_proxy(own_port: u16) -> Vec<String> {
    let Ok(mode) = command_output(&["gsettings", "get", "org.gnome.system.proxy", "mode"]) else {
        return Vec::new();
    };
    if mode.trim() == "'none'" {
        return Vec::new();
    }
    let Ok(host) = command_output(&["gsettings", "get", "org.gnome.system.proxy.http", "host"])
    else {
        return vec!["system proxy".into()];
    };
    let port = command_output(&["gsettings", "get", "org.gnome.system.proxy.http", "port"])
        .unwrap_or_default();
    let host = host.trim().trim_matches('\'');
    let port = port.trim().trim_matches('\'');
    if host == "127.0.0.1" && port == own_port.to_string() {
        Vec::new()
    } else if host.is_empty() {
        vec!["system proxy".into()]
    } else {
        vec![format!("{host}:{port}")]
    }
}

fn command_output(args: &[&str]) -> Result<String, ()> {
    let output = Command::new(args[0])
        .args(&args[1..])
        .output()
        .map_err(|_| ())?;
    if !output.status.success() {
        return Err(());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// `scutil --proxy`. Enabled HTTP/HTTPS/SOCKS endpoints, skipping this app's own port.
pub fn parse_scutil_proxy(text: &str, own_port: u16) -> Vec<String> {
    let value = |key: &str| {
        text.lines()
            .find_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix(key)?;
                rest.trim().strip_prefix(':').map(str::trim)
            })
            .unwrap_or("")
            .to_string()
    };
    let mut found = Vec::new();
    for (enable, proxy, port) in [
        ("HTTPEnable", "HTTPProxy", "HTTPPort"),
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        ("SOCKSEnable", "SOCKSProxy", "SOCKSPort"),
    ] {
        if value(enable) != "1" {
            continue;
        }
        let server = value(proxy);
        let port = value(port);
        if server.is_empty() || port == "0" {
            continue;
        }
        let own = (server == "127.0.0.1" || server == "localhost") && port == own_port.to_string();
        if !own {
            found.push(format!("{server}:{port}"));
        }
    }
    if value("ProxyAutoConfigEnable") == "1" {
        found.push("PAC".into());
    }
    found.sort();
    found.dedup();
    found
}

/// TUN that is already up.
///
/// macOS `ifconfig`: a `utun`/`tun` interface with an IPv4 address. System tunnels only have
/// link-local IPv6, so an IPv4 address means a TUN proxy (or another tunnel) is on.
/// Linux `ip -4 addr`: the same, limited to `tun*` / `utun*`.
/// Windows `ipconfig`: the addresses this app and other TUN proxies assign (`172.19.0.0/24`, `198.18.0.0/15`).
pub fn parse_tun(text: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        return parse_windows_tun(text);
    }
    #[cfg(not(windows))]
    {
        parse_unix_tun(text)
    }
}

#[cfg(any(not(windows), test))]
fn parse_unix_tun(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut iface: Option<String> = None;
    for line in text.lines() {
        if line.starts_with(|c: char| c.is_ascii_whitespace()) {
            let Some(name) = iface.as_deref() else {
                continue;
            };
            let trimmed = line.trim();
            let address = trimmed
                .strip_prefix("inet ")
                .and_then(|rest| rest.split_whitespace().next())
                .filter(|ip| *ip != "127.0.0.1" && !ip.is_empty());
            if let Some(ip) = address {
                let ip = ip.split('/').next().unwrap_or(ip);
                found.push(format!("TUN {name} {ip}"));
            }
            continue;
        }
        // `ip -4 -o addr`: "5: tun0 inet 172.19.0.1/30 ..."
        let mut parts = line.split_whitespace();
        let first = parts.next().unwrap_or("");
        let name = if first.ends_with(':')
            && first[..first.len() - 1].chars().all(|c| c.is_ascii_digit())
        {
            parts.next().unwrap_or("").trim_end_matches(':')
        } else {
            first.trim_end_matches(':')
        };
        iface = if is_tun_name(name) {
            Some(name.to_string())
        } else {
            None
        };
        if iface.is_some() {
            let rest: Vec<&str> = parts.collect();
            if let Some(pos) = rest.iter().position(|part| *part == "inet") {
                if let Some(ip) = rest.get(pos + 1) {
                    let ip = ip.split('/').next().unwrap_or(ip);
                    if !ip.is_empty() && ip != "127.0.0.1" {
                        found.push(format!("TUN {name} {ip}"));
                    }
                }
            }
        }
    }
    found
}

#[cfg(any(windows, test))]
fn parse_windows_tun(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        let Some((_, value)) = line.split_once(':') else {
            continue;
        };
        let ip = value.trim();
        if is_proxy_tun_address(ip) {
            found.push(format!("TUN {ip}"));
        }
    }
    found
}

fn is_tun_name(name: &str) -> bool {
    let name = name.trim();
    name.starts_with("tun") || name.starts_with("utun")
}

#[cfg(any(windows, test))]
fn is_proxy_tun_address(ip: &str) -> bool {
    let Some((a, rest)) = ip.split_once('.') else {
        return false;
    };
    let Some((b, _)) = rest.split_once('.') else {
        return false;
    };
    let Ok(a) = a.parse::<u8>() else {
        return false;
    };
    let Ok(b) = b.parse::<u8>() else {
        return false;
    };
    // sing-box default in this app, and the range other TUN proxies use for the tunnel address.
    (a == 172 && b == 19) || (a == 198 && (b == 18 || b == 19))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scutil_reports_an_enabled_proxy_and_skips_our_port() {
        let sample = "\
<dictionary> {\n\
  HTTPEnable : 1\n\
  HTTPPort : 7890\n\
  HTTPProxy : 127.0.0.1\n\
  HTTPSEnable : 1\n\
  HTTPSPort : 7890\n\
  HTTPSProxy : 127.0.0.1\n\
  SOCKSEnable : 0\n\
  ProxyAutoConfigEnable : 0\n\
}\n";
        assert_eq!(
            parse_scutil_proxy(sample, 2080),
            vec!["127.0.0.1:7890".to_string()]
        );
        let ours = sample.replace("7890", "2080");
        assert_eq!(parse_scutil_proxy(&ours, 2080), Vec::<String>::new());
    }

    #[test]
    fn a_tun_with_an_ipv4_address_is_on_and_a_link_local_utun_is_not() {
        let sample = "utun0: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1380\n\tinet6 fe80::1%utun0 prefixlen 64\nutun8: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 9000\n\tinet 198.18.0.1 --> 198.18.0.1 netmask 0xfffffffc\nen0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\tinet 192.168.1.20 netmask 0xffffff00\n";
        assert_eq!(
            parse_unix_tun(sample),
            vec!["TUN utun8 198.18.0.1".to_string()]
        );
    }

    #[test]
    fn linux_ip_addr_reports_only_tun_devices() {
        let sample = "\
2: enp0s1    inet 192.168.1.20/24 brd 192.168.1.255 scope global\n\
5: tun0    inet 172.19.0.1/30 scope global tun0\n";
        assert_eq!(
            parse_unix_tun(sample),
            vec!["TUN tun0 172.19.0.1".to_string()]
        );
    }

    #[test]
    fn windows_ipconfig_reports_proxy_tunnel_ranges_only() {
        let sample = "Ethernet adapter TUN:\n   IPv4 Address: 172.19.0.1\nEthernet adapter Ethernet:\n   IPv4 Address: 192.168.1.20\nEthernet adapter Other tunnel:\n   IPv4 Address: 198.19.0.1\n";
        let expected = vec!["TUN 172.19.0.1".to_string(), "TUN 198.19.0.1".to_string()];
        assert_eq!(parse_windows_tun(sample), expected);
        #[cfg(windows)]
        assert_eq!(parse_tun(sample), expected);
    }
}
