//! TUN configuration is owned by the application, not by imported profiles.
use serde_json::{json, Value};

use crate::network_settings::TunSettings;

/// Shapes the runtime config for TUN mode, or strips everything TUN from it.
///
/// What is fixed, whatever the settings say: all traffic is routed into the device, DNS on it is
/// hijacked, and names resolve through the proxy. What the user may adjust lives in `TunSettings`.
pub fn configure(config: &mut Value, enabled: bool, settings: &TunSettings) -> Result<(), String> {
    let inbounds = config["inbounds"]
        .as_array_mut()
        .ok_or("native.configNoInbounds")?;
    inbounds.retain(|item| item["type"] != "tun");
    if let Some(rules) = config["route"]["rules"].as_array_mut() {
        rules.retain(|r| {
            !(r["inbound"] == json!(["tun-in"])
                && (r["action"] == "sniff"
                    || r["action"] == "hijack-dns"
                    || r["protocol"] == "stun"
                    || (r["ip_version"] == 6 && r["action"] == "reject")))
        });
    }
    if !enabled {
        if config["dns"]["final"] == "airport-local" || config["dns"]["final"] == "airport-remote" {
            config.as_object_mut().unwrap().remove("dns");
            if let Some(route) = config["route"].as_object_mut() {
                route.remove("default_domain_resolver");
            }
        }
        return Ok(());
    }
    settings.validate()?;
    let mut device = json!({
        "type":"tun", "tag":"tun-in",
        // Fixed: the service's kill switch recognises tunnelled traffic by these addresses.
        "address":[airport_tun_service::TUN_PREFIX_V4, airport_tun_service::TUN_PREFIX_V6],
        "mtu":settings.mtu, "stack":settings.stack, "auto_route":true,
        "strict_route":settings.strict_route, "dns_mode":"hijack"
    });
    if !settings.route_exclude.is_empty() {
        device["route_exclude_address"] = json!(settings.route_exclude);
    }
    // nftables redirection exists on Linux only; the core refuses the option elsewhere.
    if cfg!(target_os = "linux") && settings.auto_redirect {
        device["auto_redirect"] = true.into();
    }
    config["inbounds"].as_array_mut().unwrap().push(device);
    if !config["route"].is_object() {
        config["route"] = json!({});
    }
    config["route"]["auto_detect_interface"] = true.into();
    // The proxy servers' own hostnames must resolve outside the tunnel. If one of them is
    // answered by the fake-IP server, the core dials that fake address, the fake address enters
    // the device, and the device dials the same hostname again: a loop that completes nothing
    // (measured: ~40 re-entries in 5 s, zero finished connections). So nodes are named in a rule
    // ahead of every other, and the dialer resolves them with the same endpoint.
    let hosts = proxy_hosts(config);
    // When names are limited to IPv4 below, the servers themselves must stay reachable on an
    // IPv6-only network, so this lookup keeps both families and merely tries IPv4 first.
    config["route"]["default_domain_resolver"] = if settings.ipv6 {
        json!(LOCAL_TAG)
    } else {
        json!({"server":LOCAL_TAG,"strategy":"prefer_ipv4"})
    };
    let mut rules = config["route"]["rules"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    rules.insert(0, json!({"inbound":["tun-in"],"action":"sniff"}));
    // DNS is taken over by what it is as well as by where it goes: a resolver on another
    // port is still DNS, and must not slip past as ordinary traffic to a direct rule.
    rules.insert(
        1,
        json!({"inbound":["tun-in"],"protocol":"dns","action":"hijack-dns"}),
    );
    rules.insert(
        2,
        json!({"inbound":["tun-in"],"port":53,"action":"hijack-dns"}),
    );
    config["route"]["rules"] = rules.into();
    let outbound = config["route"]["final"]
        .as_str()
        .ok_or("native.tunNoFinalOutbound")?
        .to_owned();
    // WebRTC discovery must not inherit regional/private direct-routing rules.
    // Unsupported UDP fails on the selected proxy; never fall back to direct.
    config["route"]["rules"].as_array_mut().unwrap().insert(
        3,
        json!({"inbound":["tun-in"],"protocol":"stun","action":"route","outbound":outbound}),
    );
    // An IPv6 handshake succeeds inside the tunnel before the server can fail it, so the
    // system never falls back to IPv4. Reset those connections unless the user asked for
    // IPv6, which makes the OS fall back to IPv4 at once. `no_drop` keeps every reset a
    // reset: the core otherwise silences a burst of rejects into a timeout, and a probing
    // system can open far more than that burst in a second, which would stall as if offline.
    if !settings.ipv6 {
        config["route"]["rules"].as_array_mut().unwrap().insert(
            3,
            json!({"inbound":["tun-in"],"ip_version":6,"action":"reject","no_drop":true}),
        );
    }
    // Mirror direct-route domain suffixes (including the compact domestic list) into DNS so
    // those names resolve through the domestic resolver for a real address instead of a fake-IP;
    // the route's domain_suffix rules then keep that traffic off the proxy.
    let direct_domains: Vec<Value> = config["route"]["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|rule| rule["outbound"] == "direct")
        .filter_map(|rule| rule["domain_suffix"].as_array())
        .flatten()
        .cloned()
        .collect();
    // DNS 覆写 (reference: clash-verge-rev). Fake-IP (default) answers a name with a fake
    // address and lets the exit resolve it — robust, since no lookup has to reach an external
    // resolver through the node. Remote mode resolves foreign names for real via the node's DoH
    // instead, for the apps that need a real address up front.
    //
    // The tunnel owns the system resolver while it runs (the privileged service points every
    // network service at this device), so naming the system resolver here would be naming the
    // device itself: every lookup would come straight back. The domestic resolver is therefore
    // always an encrypted endpoint pinned by IP, which needs no lookup to be dialled.
    let domestic = domestic_preset(&settings.dns_domestic);
    let mut servers = vec![domestic_dns_server(domestic)];
    let mut dns_rules = vec![];
    if !direct_domains.is_empty() {
        dns_rules.push(json!({"domain_suffix":direct_domains,"server":LOCAL_TAG}));
    }
    let dns_final = if settings.dns_mode == "remote" {
        servers.push(remote_dns_server(&settings.dns_remote, &outbound));
        "airport-remote"
    } else {
        servers.push(json!({"type":"fakeip","tag":"airport-fakeip","inet4_range":"198.18.0.0/15","inet6_range":"fc00::/18"}));
        // Other record types (for example PTR) retain the domestic resolver.
        dns_rules.push(json!({"query_type":["A","AAAA"],"server":"airport-fakeip","rewrite_ttl":30}));
        // Do not mix direct HTTPS/SVCB address hints with proxy-resolved fake addresses.
        // Empty metadata records let browsers use normal TLS without waiting on direct DNS.
        dns_rules.push(json!({"query_type":["HTTPS","SVCB"],"action":"predefined"}));
        LOCAL_TAG
    };
    // With IPv6 off, an AAAA answer is a connection that cannot work: the system would try it
    // first, this device resets it, and only then does it fall back. An empty answer says "no
    // such record" once, which is what every IPv4-only network does, and the system takes IPv4
    // straight away.
    if !settings.ipv6 {
        dns_rules.insert(0, json!({"query_type":["AAAA"],"action":"predefined"}));
    }
    if !hosts.is_empty() {
        // First match wins, so this must stand ahead of the fake-IP rule: a fake address for a
        // proxy server is one the core can never dial out of.
        dns_rules.insert(0, json!({"domain":hosts,"server":LOCAL_TAG}));
    }
    config["dns"] = json!({"servers":servers,"rules":dns_rules,"final":dns_final,"timeout":"5s"});
    if !settings.ipv6 {
        config["dns"]["strategy"] = "ipv4_only".into();
    }
    Ok(())
}

const LOCAL_TAG: &str = "airport-local";

/// The domestic preset, with `local` folded into a real endpoint: the system resolver is not
/// reachable from inside the tunnel it now answers through.
fn domestic_preset(preset: &str) -> &'static str {
    match preset {
        "dnspod" => "dnspod",
        _ => "ali",
    }
}

/// The domestic/direct resolver: encrypted, pinned by IP so dialling it needs no lookup.
fn domestic_dns_server(preset: &str) -> Value {
    match preset {
        "dnspod" => {
            json!({"type":"https","tag":LOCAL_TAG,"server":"120.53.53.53","server_port":443,"tls":{"enabled":true,"server_name":"doh.pub"}})
        }
        _ => {
            json!({"type":"https","tag":LOCAL_TAG,"server":"223.5.5.5","server_port":443,"tls":{"enabled":true,"server_name":"dns.alidns.com"}})
        }
    }
}

/// Hostnames of the proxy servers this config dials by name; empty when they are all addresses.
fn proxy_hosts(config: &Value) -> Vec<String> {
    use std::net::IpAddr;
    let mut hosts: Vec<String> = config["outbounds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|outbound| outbound["server"].as_str())
        .filter(|server| !server.is_empty() && server.parse::<IpAddr>().is_err())
        .map(str::to_owned)
        .collect();
    hosts.sort_unstable();
    hosts.dedup();
    hosts
}

/// The foreign resolver for `remote` mode, tagged `airport-remote` and dialed through the exit
/// (`detour`). Each preset is a DoH endpoint reached by IP with its own TLS name, so it needs
/// no bootstrap of its own.
fn remote_dns_server(preset: &str, detour: &str) -> Value {
    let (server, sni) = match preset {
        "google" => ("8.8.8.8", "dns.google"),
        "quad9" => ("9.9.9.9", "dns.quad9.net"),
        _ => ("1.1.1.1", "cloudflare-dns.com"),
    };
    json!({"type":"https","tag":"airport-remote","server":server,"server_port":443,"tls":{"enabled":true,"server_name":sni},"detour":detour})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owns_tun_dns_and_prevents_routing_loop() {
        let mut config = json!({"inbounds":[{"type":"mixed"},{"type":"tun","interface_name":"untrusted"}],"route":{"final":"select","rules":[{"ip_is_private":true,"outbound":"direct"}]}});
        let settings = TunSettings::default();
        configure(&mut config, true, &settings).unwrap();
        assert_eq!(config["inbounds"].as_array().unwrap().len(), 2);
        assert_eq!(config["inbounds"][1]["dns_mode"], "hijack");
        assert_eq!(
            config["inbounds"][1]["address"].as_array().unwrap().len(),
            2
        );
        assert_eq!(config["route"]["auto_detect_interface"], true);
        assert_eq!(config["dns"]["servers"][1]["type"], "fakeip");
        assert_eq!(config["dns"]["final"], "airport-local");
        assert_eq!(config["route"]["rules"][1]["protocol"], "dns");
        assert_eq!(config["route"]["rules"][1]["action"], "hijack-dns");
        assert_eq!(config["route"]["rules"][2]["port"], 53);
        assert_eq!(config["route"]["rules"][2]["action"], "hijack-dns");
        // Applying twice (a reconnect) must not stack the rules.
        configure(&mut config, true, &settings).unwrap();
        assert_eq!(config["route"]["rules"][3]["ip_version"], 6);
        assert_eq!(config["route"]["rules"][3]["action"], "reject");
        assert_eq!(config["route"]["rules"][3]["no_drop"], true);
        assert_eq!(config["route"]["rules"][4]["protocol"], "stun");
        assert_eq!(config["route"]["rules"][4]["outbound"], "select");
        assert_eq!(config["route"]["rules"].as_array().unwrap().len(), 6);
        configure(&mut config, false, &settings).unwrap();
        assert_eq!(config["route"]["rules"].as_array().unwrap().len(), 1);
        assert!(config.get("dns").is_none());
        assert!(config["route"].get("default_domain_resolver").is_none());
        assert_eq!(config["inbounds"].as_array().unwrap().len(), 1);
    }
    // The defaults are the values that were fixed before the device became tunable.
    #[test]
    fn default_device_is_the_conservative_one() {
        let mut config = json!({"inbounds":[{"type":"mixed"}],"route":{"final":"select"}});
        configure(&mut config, true, &TunSettings::default()).unwrap();
        let device = &config["inbounds"][1];
        assert_eq!(device["mtu"], 1500);
        assert_eq!(device["stack"], "mixed");
        assert_eq!(device["strict_route"], true);
        assert_eq!(device["auto_route"], true);
        assert!(device.get("auto_redirect").is_none());
        assert!(device.get("route_exclude_address").is_none());
        assert_eq!(
            config["route"]["default_domain_resolver"]["strategy"],
            "prefer_ipv4"
        );
        assert_eq!(config["dns"]["strategy"], "ipv4_only");
        assert_eq!(config["route"]["rules"][3]["action"], "reject");
        assert_eq!(config["dns"]["servers"][1]["type"], "fakeip");
        assert_eq!(config["dns"]["servers"][1]["inet4_range"], "198.18.0.0/15");
        let mut ipv6 = json!({"inbounds":[{"type":"mixed"}],"route":{"final":"select"}});
        configure(
            &mut ipv6,
            true,
            &TunSettings {
                ipv6: true,
                ipv6_chosen: true,
                ..TunSettings::default()
            },
        )
        .unwrap();
        assert_eq!(ipv6["route"]["default_domain_resolver"], "airport-local");
        assert!(ipv6["dns"].get("strategy").is_none());
        assert!(ipv6["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|rule| rule["ip_version"] != 6));
    }
    #[test]
    fn settings_shape_the_device_but_cannot_stop_it_capturing() {
        let mut config = json!({"inbounds":[{"type":"mixed"}],"route":{"final":"select"}});
        let settings = TunSettings {
            stack: "gvisor".into(),
            mtu: 9000,
            strict_route: false,
            ipv6: false,
            ipv6_chosen: true,
            auto_redirect: true,
            route_exclude: vec!["192.168.0.0/16".into(), "fc00::/7".into()],
            kill_switch: true,
            ..TunSettings::default()
        };
        configure(&mut config, true, &settings).unwrap();
        let device = &config["inbounds"][1];
        assert_eq!(device["mtu"], 9000);
        assert_eq!(device["stack"], "gvisor");
        assert_eq!(device["strict_route"], false);
        assert_eq!(
            device["route_exclude_address"],
            json!(["192.168.0.0/16", "fc00::/7"])
        );
        assert_eq!(
            device.get("auto_redirect").is_some(),
            cfg!(target_os = "linux")
        );
        // Whatever the settings, traffic and its DNS are captured, and IPv6 stays on the
        // device so it cannot leave beside the tunnel.
        assert_eq!(device["auto_route"], true);
        assert_eq!(device["dns_mode"], "hijack");
        assert_eq!(device["address"].as_array().unwrap().len(), 2);
        // Names are limited to IPv4, but the proxy servers themselves still resolve both ways.
        assert_eq!(config["dns"]["strategy"], "ipv4_only");
        assert_eq!(
            config["route"]["default_domain_resolver"]["strategy"],
            "prefer_ipv4"
        );
        // The privileged service accepts exactly what the app generates.
        crate::tray_speed::Endpoint::new()
            .unwrap()
            .configure(&mut config);
        config["outbounds"] = json!([{"type":"direct","tag":"direct"},{"type":"selector","tag":"select","outbounds":["direct"]}]);
        config["inbounds"][0] = json!({"type":"mixed","listen":"127.0.0.1","listen_port":29876});
        assert!(airport_tun_service::safe_config(config).is_ok());
        // An invalid setting is refused before any config is produced.
        let mut bad = json!({"inbounds":[{"type":"mixed"}],"route":{"final":"select"}});
        let invalid = TunSettings {
            mtu: 100,
            ..TunSettings::default()
        };
        assert!(configure(&mut bad, true, &invalid).is_err());
    }
    #[test]
    #[ignore = "requires the core sidecar built by scripts/prepare-sidecars.py"]
    fn bundled_core_accepts_tun_config_without_starting_it() {
        // The defaults, and every option moved off its default (auto_redirect is emitted on Linux only).
        let adjusted = TunSettings {
            stack: "gvisor".into(),
            mtu: 9000,
            strict_route: false,
            ipv6: false,
            ipv6_chosen: true,
            auto_redirect: true,
            route_exclude: vec!["192.168.0.0/16".into(), "fc00::/7".into()],
            kill_switch: true,
            ..TunSettings::default()
        };
        for settings in [
            TunSettings::default(),
            TunSettings {
                stack: "system".into(),
                ..TunSettings::default()
            },
            adjusted,
        ] {
            let mut config = json!({"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":29876}],"outbounds":[{"type":"direct","tag":"direct"},{"type":"selector","tag":"select","outbounds":["direct"]}],"route":{"final":"select","rules":[{"domain_suffix":["cn"],"outbound":"direct"}]}});
            configure(&mut config, true, &settings).unwrap();
            crate::tray_speed::Endpoint::new()
                .unwrap()
                .configure(&mut config);
            let config = airport_tun_service::safe_config(config).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("tun-check.json");
            crate::storage::write_private(&path, config.to_string().as_bytes()).unwrap();
            let out = std::process::Command::new(crate::resolve_binary().unwrap())
                .args(["check", "-c"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{settings:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    #[test]
    #[ignore = "requires the core sidecar built by scripts/prepare-sidecars.py; uses only loopback sockets"]
    fn stun_routes_before_direct_rules_without_changing_system_network() {
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream, UdpSocket};
        use std::process::{Command, Stdio};
        use std::time::Duration;
        struct Process(std::process::Child);
        impl Drop for Process {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        udp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let target_port = udp.local_addr().unwrap().port();
        let mut config = json!({"inbounds":[], "outbounds":[{"type":"direct","tag":"direct"},{"type":"direct","tag":"proxy-test"}],"route":{"final":"proxy-test","rules":[{"ip_is_private":true,"outbound":"direct"}]}});
        configure(&mut config, true, &TunSettings::default()).unwrap();
        // Exercise the exact TUN rules via SOCKS UDP, without acquiring a virtual NIC.
        // Numeric loopback targets do not need the proxy-backed DNS transport.
        config.as_object_mut().unwrap().remove("dns");
        config["route"]
            .as_object_mut()
            .unwrap()
            .remove("default_domain_resolver");
        config["inbounds"] =
            json!([{"type":"mixed","tag":"tun-in","listen":"127.0.0.1","listen_port":port}]);
        config["log"] = json!({"level":"debug"});
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("check.json");
        std::fs::write(&path, config.to_string()).unwrap();
        let log_path = dir.path().join("core.log");
        let output = std::fs::File::create(&log_path).unwrap();
        let _child = Process(
            Command::new(crate::resolve_binary().unwrap())
                .args(["run", "-c"])
                .arg(&path)
                .stdout(Stdio::from(output.try_clone().unwrap()))
                .stderr(Stdio::from(output))
                .spawn()
                .unwrap(),
        );
        let mut tcp = (0..100)
            .find_map(|_| {
                let result = TcpStream::connect(("127.0.0.1", port)).ok();
                if result.is_none() {
                    std::thread::sleep(Duration::from_millis(30));
                }
                result
            })
            .unwrap_or_else(|| {
                panic!(
                    "core did not listen: {}",
                    std::fs::read_to_string(&log_path).unwrap_or_default()
                )
            });
        tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        tcp.write_all(&[5, 1, 0]).unwrap();
        let mut auth = [0; 2];
        tcp.read_exact(&mut auth).unwrap();
        assert_eq!(auth, [5, 0]);
        tcp.write_all(&[5, 3, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();
        let mut association = [0; 10];
        tcp.read_exact(&mut association).unwrap();
        assert_eq!(association[1], 0);
        let relay_port = u16::from_be_bytes([association[8], association[9]]);
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        // RFC 5389 Binding Request: header, magic cookie and transaction identifier.
        let stun = [
            0, 1, 0, 0, 0x21, 0x12, 0xa4, 0x42, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
        ];
        let mut packet = vec![0, 0, 0, 1, 127, 0, 0, 1];
        packet.extend(target_port.to_be_bytes());
        packet.extend(stun);
        client.send_to(&packet, ("127.0.0.1", relay_port)).unwrap();
        let mut buffer = [0; 1024];
        let (size, source) = udp.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..size], &stun);
        udp.send_to(&buffer[..size], source).unwrap();
        assert!(client.recv_from(&mut buffer).unwrap().0 >= stun.len());
        let logs = std::fs::read_to_string(log_path).unwrap();
        assert!(logs.contains("outbound/direct[proxy-test]"), "{logs}");
        assert!(!logs.contains("outbound/direct[direct]"), "{logs}");
    }

    // A node hostname answered with a fake address is a tunnel that can never carry a byte:
    // the core would dial the fake address straight back into the device, forever.
    #[test]
    fn node_hostnames_resolve_outside_the_fake_ip_range() {
        let mut config = json!({
            "inbounds":[{"type":"mixed"}],
            "outbounds":[
                {"type":"trojan","tag":"a","server":"sg01.example.net"},
                {"type":"vmess","tag":"b","server":"us01.example.net"},
                {"type":"direct","tag":"direct"},
                {"type":"wireguard","tag":"wg","server":"203.0.113.7"},
                {"type":"selector","tag":"select","outbounds":["a","b"]}
            ],
            "route":{"final":"select"}
        });
        configure(&mut config, true, &TunSettings::default()).unwrap();
        // Ahead of every other rule, fake-IP included, which still serves ordinary names.
        assert_eq!(
            config["dns"]["rules"][0],
            json!({"domain":["sg01.example.net","us01.example.net"],"server":"airport-local"})
        );
        // IPv6 is off, so AAAA is answered empty instead of with an address that cannot work.
        assert_eq!(
            config["dns"]["rules"][1],
            json!({"query_type":["AAAA"],"action":"predefined"})
        );
        assert_eq!(config["dns"]["rules"][2]["server"], "airport-fakeip");
        // The dialer resolves node names with the same resolver, so a node is never looked up
        // through the device either.
        assert_eq!(
            config["route"]["default_domain_resolver"],
            json!({"server":"airport-local","strategy":"prefer_ipv4"})
        );
        // Pinned by IP: dialling the resolver must not need a lookup of its own.
        let domestic = config["dns"]["servers"][0].clone();
        assert_eq!(domestic["type"], "https");
        assert_eq!(domestic["server"], "223.5.5.5");
        assert_eq!(domestic["tls"]["server_name"], "dns.alidns.com");
        // With every node given as an address there is no name to keep out of fake-IP, but the
        // resolver itself stays an endpoint.
        let mut fixed = json!({
            "inbounds":[{"type":"mixed"}],
            "outbounds":[{"type":"trojan","tag":"a","server":"203.0.113.7"}],
            "route":{"final":"a"}
        });
        configure(&mut fixed, true, &TunSettings::default()).unwrap();
        assert!(fixed["dns"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|rule| rule["domain"].is_null()));
        assert_eq!(fixed["dns"]["servers"][0]["type"], "https");
        // The chosen domestic endpoint is the one used; `local` has no meaning in the tunnel.
        for (preset, address, name) in [
            ("dnspod", "120.53.53.53", "doh.pub"),
            ("local", "223.5.5.5", "dns.alidns.com"),
            ("ali", "223.5.5.5", "dns.alidns.com"),
        ] {
            let mut config = json!({
                "inbounds":[{"type":"mixed"}],
                "outbounds":[{"type":"trojan","tag":"a","server":"sg01.example.net"}],
                "route":{"final":"a"}
            });
            configure(
                &mut config,
                true,
                &TunSettings {
                    dns_domestic: preset.into(),
                    ..TunSettings::default()
                },
            )
            .unwrap();
            let servers = config["dns"]["servers"].as_array().unwrap();
            assert!(
                servers.iter().all(|server| server["type"] != "local"),
                "{preset} named the system resolver"
            );
            assert_eq!(servers[0]["server"], address, "{preset}");
            assert_eq!(servers[0]["tls"]["server_name"], name, "{preset}");
        }
        // remote mode dials its DoH through the node, so it depends on this even more.
        let mut remote = json!({
            "inbounds":[{"type":"mixed"}],
            "outbounds":[{"type":"trojan","tag":"a","server":"sg01.example.net"}],
            "route":{"final":"a"}
        });
        configure(
            &mut remote,
            true,
            &TunSettings {
                dns_mode: "remote".into(),
                ..TunSettings::default()
            },
        )
        .unwrap();
        assert_eq!(remote["dns"]["rules"][0]["server"], "airport-local");
        assert_eq!(remote["dns"]["final"], "airport-remote");
    }

    // Whatever the settings are, no lookup may be handed the system resolver: inside the
    // tunnel that resolver is the device itself, and the answer would come straight back.
    #[test]
    fn the_system_resolver_is_never_named_inside_the_tunnel() {
        for preset in ["local", "ali", "dnspod"] {
            for mode in ["fakeip", "remote"] {
                let mut config = json!({
                    "inbounds":[{"type":"mixed"}],
                    "outbounds":[{"type":"trojan","tag":"a","server":"node.example.net"}],
                    "route":{"final":"a"}
                });
                configure(
                    &mut config,
                    true,
                    &TunSettings {
                        dns_domestic: preset.into(),
                        dns_mode: mode.into(),
                        ..TunSettings::default()
                    },
                )
                .unwrap();
                for server in config["dns"]["servers"].as_array().unwrap() {
                    assert_ne!(server["type"], "local", "{preset}/{mode}");
                    if server["type"] == "fakeip" {
                        continue;
                    }
                    assert!(
                        server["server"]
                            .as_str()
                            .is_some_and(|value| value.parse::<std::net::IpAddr>().is_ok()),
                        "{preset}/{mode} must dial its resolver by address"
                    );
                }
            }
        }
    }

    #[test]
    fn browser_dns_and_mode_switch_do_not_keep_tunnel_dns() {
        let mut c = json!({"inbounds":[{"type":"mixed"}],"route":{"final":"select","rules":[{"domain_suffix":["cn"],"outbound":"direct"}]}});
        configure(&mut c, true, &TunSettings::default()).unwrap();
        let rules = c["dns"]["rules"].as_array().unwrap();
        assert_eq!(rules[1]["domain_suffix"], json!(["cn"]));
        assert!(rules.iter().any(|r| r["server"] == "airport-fakeip" && r["rewrite_ttl"] == 30));
        assert!(rules.iter().any(|r| r["query_type"] == json!(["HTTPS","SVCB"]) && r["action"] == "predefined"));
        configure(&mut c, false, &TunSettings::default()).unwrap();
        assert!(c.get("dns").is_none());
        assert!(c["route"].get("default_domain_resolver").is_none());
    }
    #[test]
    #[ignore = "opt-in private config export for DNS regression"]
    fn export_dns_review_configs() {
        let dir = std::path::PathBuf::from(std::env::var("AIRPORT_DNS_REVIEW_DIR").unwrap());
        let raw = std::fs::read_to_string(std::env::var("AIRPORT_DNS_REVIEW_PROFILE").unwrap()).unwrap();
        for mode in ["fakeip", "remote", "system"] {
            let mut c: Value = serde_json::from_str(&raw).unwrap();
            configure(&mut c, true, &TunSettings { dns_mode: if mode == "remote" { "remote".into() } else { "fakeip".into() }, ..TunSettings::default() }).unwrap();
            if mode == "system" { configure(&mut c, false, &TunSettings::default()).unwrap(); }
            crate::storage::write_private(&dir.join(format!("{mode}.json")), c.to_string().as_bytes()).unwrap();
        }
    }

    #[test]
    fn plain_proxy_does_not_acquire_tun() {
        let mut config =
            json!({"inbounds":[{"type":"mixed"},{"type":"tun"}],"route":{"final":"select"}});
        configure(&mut config, false, &TunSettings::default()).unwrap();
        assert!(config.get("dns").is_none());
        assert_eq!(config["inbounds"].as_array().unwrap().len(), 1);
    }
}
