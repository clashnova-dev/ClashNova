use crate::message::{msg, text};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{net::TcpListener, path::Path};

pub const DEFAULT_PROXY_PORT: u16 = 2080;
pub const MIN_PROXY_PORT: u16 = 1024;
// Generated profiles include application-owned route/DNS data in addition to the
// separately bounded 2 MiB subscription. Reconnects also read the expanded TUN profile.
pub(crate) const MAX_CONFIG_BYTES: usize = 8 * 1024 * 1024;

pub fn default_proxy_port() -> u16 {
    DEFAULT_PROXY_PORT
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkSettings {
    pub proxy_port: u16,
    pub allow_lan: bool,
    /// Absent in files written before TUN became tunable; the defaults are the old fixed values.
    #[serde(default)]
    pub tun: TunSettings,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            proxy_port: DEFAULT_PROXY_PORT,
            allow_lan: false,
            tun: TunSettings::default(),
        }
    }
}

pub const DEFAULT_TUN_MTU: u32 = 1500;
/// Above this the virtual device stops helping: larger datagrams are only fragmented again
/// on the way out of the proxy server.
pub const MAX_TUN_MTU: u32 = 9000;
pub const MAX_TUN_EXCLUSIONS: usize = 64;

/// The parts of the virtual network device a user may need to adjust to their machine.
/// The defaults are the conservative choice on every platform; nothing here can turn off
/// capturing traffic or its DNS.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TunSettings {
    /// `mixed`: system TCP with a userspace UDP stack. `system` and `gvisor` are fallbacks
    /// for machines where a firewall or driver interferes with one of the two.
    pub stack: String,
    pub mtu: u32,
    /// Keeps traffic and DNS from leaving beside the tunnel. Some virtual machine and
    /// container network drivers on Windows need it off.
    pub strict_route: bool,
    /// Whether names resolve to IPv6 addresses. Off unless the user turns it on and saves:
    /// the tunnel accepts an IPv6 handshake locally, so a server without IPv6 never falls
    /// back to IPv4 and the computer looks offline. IPv6 traffic is captured either way.
    pub ipv6: bool,
    /// Set when the user saves network settings. Files from before that choice existed
    /// stored the old default (`true`) and are read as off.
    #[serde(default)]
    pub ipv6_chosen: bool,
    /// Linux only: redirect with nftables instead of routing alone. Faster and coexists
    /// with container bridges, but needs nftables on the machine.
    pub auto_redirect: bool,
    /// Destinations that stay off the virtual device entirely.
    pub route_exclude: Vec<String>,
    /// Block the network when the tunnel drops unexpectedly, until it is back or the user
    /// disconnects. Off unless asked for: it changes what a dropped connection means.
    #[serde(default)]
    pub kill_switch: bool,
    /// DNS 覆写 (reference: clash-verge-rev). `fakeip` hands foreign names a fake IP and lets
    /// the exit resolve them; `remote` resolves foreign names for real through the node's DoH.
    #[serde(default = "default_dns_mode")]
    pub dns_mode: String,
    /// Foreign resolver preset, used in `remote` mode: `cloudflare` | `google` | `quad9`.
    #[serde(default = "default_dns_remote")]
    pub dns_remote: String,
    /// Domestic/direct resolver preset: `local` (system) | `ali` | `dnspod`.
    #[serde(default = "default_dns_domestic")]
    pub dns_domestic: String,
}

fn default_dns_mode() -> String {
    "fakeip".into()
}
fn default_dns_remote() -> String {
    "cloudflare".into()
}
fn default_dns_domestic() -> String {
    "local".into()
}

impl Default for TunSettings {
    fn default() -> Self {
        Self {
            stack: "mixed".into(),
            mtu: DEFAULT_TUN_MTU,
            strict_route: true,
            ipv6: false,
            ipv6_chosen: false,
            auto_redirect: false,
            route_exclude: vec![],
            kill_switch: false,
            dns_mode: default_dns_mode(),
            dns_remote: default_dns_remote(),
            dns_domestic: default_dns_domestic(),
        }
    }
}

impl TunSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !airport_tun_service::TUN_STACKS.contains(&self.stack.as_str()) {
            return Err("settings.tunStackInvalid".into());
        }
        if u64::from(self.mtu) < airport_tun_service::MIN_TUN_MTU || self.mtu > MAX_TUN_MTU {
            return Err("settings.tunMtuInvalid".into());
        }
        if self.route_exclude.len() > MAX_TUN_EXCLUSIONS
            || !self
                .route_exclude
                .iter()
                .all(|prefix| airport_tun_service::valid_cidr(prefix))
        {
            return Err("settings.tunExcludeInvalid".into());
        }
        Ok(())
    }
}

impl NetworkSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.proxy_port < MIN_PROXY_PORT {
            return Err("settings.portInvalid".into());
        }
        self.tun.validate()
    }

    pub fn listen_address(&self) -> &str {
        if self.allow_lan {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        }
    }

    pub fn ensure_available(&self) -> Result<(), String> {
        self.validate()?;
        TcpListener::bind((self.listen_address(), self.proxy_port)).map_err(|_| {
            msg(
                "native.proxyPortUnavailable",
                &[("port", self.proxy_port.into())],
            )
        })?;
        Ok(())
    }

    /// The native process owns listener settings, even if a subscription carries
    /// different addresses. All connection paths use the same validated value.
    pub fn apply_to_config(&self, raw: &str) -> Result<String, String> {
        self.validate()?;
        if raw.len() > MAX_CONFIG_BYTES {
            return Err("native.configTooLarge".into());
        }
        let mut config: Value = serde_json::from_str(raw).map_err(|_| "native.configInvalid")?;
        // A subscription must not create an unauthenticated controller, change
        // the client's cache paths, or write arbitrary log files.
        let object = config.as_object_mut().ok_or("native.configNotObject")?;
        object.remove("services");
        object.remove("experimental");
        object.insert(
            "log".into(),
            serde_json::json!({"level":"info","timestamp":true}),
        );
        let inbounds = config["inbounds"]
            .as_array_mut()
            .ok_or("native.configNoInbounds")?;
        if !inbounds.iter().any(|item| item["type"] == "mixed") {
            return Err("native.configNoMixedInbound".into());
        }
        // Only this process may create listeners. A subscription cannot smuggle in a second
        // LAN listener or a tunnel. Native TUN configuration is added later from validated settings.
        *inbounds = vec![
            serde_json::json!({"type":"mixed", "tag":"mixed-in", "listen":self.listen_address(), "listen_port":self.proxy_port}),
        ];
        airport_tun_service::validate_profile_options(&config)?;
        // The file is loaded and validated again on connect. Large imported rules can grow
        // past the old 2 MiB limit when pretty-printed, so persist compact JSON instead.
        let encoded = serde_json::to_string(&config).map_err(|error| error.to_string())?;
        if encoded.len() > MAX_CONFIG_BYTES {
            return Err("native.configTooLarge".into());
        }
        Ok(encoded)
    }
}

pub fn parse(raw: &[u8]) -> Result<NetworkSettings, String> {
    let mut settings: NetworkSettings =
        serde_json::from_slice(raw).map_err(|_| "native.networkSettingsInvalid")?;
    // The old default handed applications IPv6 addresses. The tunnel completes that
    // handshake locally, so a server without IPv6 never falls back to IPv4.
    if !settings.tun.ipv6_chosen {
        settings.tun.ipv6 = false;
    }
    settings.validate()?;
    Ok(settings)
}

pub fn load(path: &Path) -> Result<NetworkSettings, String> {
    match std::fs::read(path) {
        Ok(raw) => parse(&raw),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(NetworkSettings::default())
        }
        Err(error) => Err(msg(
            "native.networkSettingsReadFailed",
            &[("error", text(error))],
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn builtin_csv(source: &str, name: &str) -> Vec<String> {
        let declaration = format!("const {name} =");
        let literal = source
            .split_once(declaration.as_str())
            .unwrap()
            .1
            .trim_start();
        let end = literal.find(';').unwrap();
        let csv: String = serde_json::from_str(&literal[..end]).unwrap();
        csv.split(',').map(str::to_owned).collect()
    }

    /// Exercise the exact production rule lists, not a small hand-written fixture.
    fn rule_profile() -> Value {
        let domains = builtin_csv(include_str!("../../src/lib/china-domains.ts"), "RAW");
        serde_json::json!({
            "inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":2080}],
            "outbounds":[
                {"type":"socks","tag":"node","server":"127.0.0.1","server_port":1081},
                {"type":"selector","tag":"select","outbounds":["node"],"default":"node"},
                {"type":"direct","tag":"direct"}
            ],
            "route":{"final":"select","rules":[
                {"ip_is_private":true,"outbound":"direct"},
                {"domain_suffix":domains,"outbound":"direct"}
            ]}
        })
    }

    #[test]
    fn compact_production_rules_survive_save_and_reconnect() {
        let source = rule_profile();
        let legacy = serde_json::to_string_pretty(&source).unwrap();
        assert_eq!(
            source["route"]["rules"][1]["domain_suffix"]
                .as_array()
                .unwrap()
                .len(),
            500
        );
        assert_eq!(source["route"]["rules"].as_array().unwrap().len(), 2);
        assert!(
            legacy.len() < 32 * 1024,
            "built-in rules must remain compact"
        );
        let settings = NetworkSettings::default();
        // Existing cached config files must work without deleting or reimporting subscriptions.
        let saved = settings.apply_to_config(&legacy).unwrap();
        assert!(!saved.contains('\n'));
        assert_eq!(settings.apply_to_config(&saved).unwrap(), saved);
        for tun in [false, true] {
            let mut runtime: Value = serde_json::from_str(&saved).unwrap();
            let endpoint = crate::tray_speed::Endpoint::new().unwrap();
            for _ in 0..3 {
                runtime =
                    serde_json::from_str(&settings.apply_to_config(&runtime.to_string()).unwrap())
                        .unwrap();
                crate::tun::configure(&mut runtime, tun, &settings.tun).unwrap();
                if tun {
                    let rules = runtime["dns"]["rules"].as_array().unwrap();
                    let domestic = rules
                        .iter()
                        .find(|rule| rule["domain_suffix"].is_array())
                        .unwrap();
                    assert_eq!(
                        domestic["domain_suffix"],
                        source["route"]["rules"][1]["domain_suffix"]
                    );
                }
                endpoint.configure(&mut runtime);
                let request = airport_tun_service::Request {
                    id: "0".repeat(32),
                    time: 1,
                    action: airport_tun_service::Action::Start {
                        config: runtime.clone(),
                        session: "1".repeat(32),
                    },
                };
                let mut frame = Vec::new();
                // Preserve the installed service's existing 4 MiB protocol budget.
                airport_tun_service::write_message(
                    &mut frame,
                    &[7; 32],
                    b"airport/request/v1",
                    &request,
                )
                .unwrap();
                assert!(frame.len() <= airport_tun_service::MAX_FRAME + 4);
            }
        }
    }

    #[test]
    fn legacy_large_profiles_still_save_and_reconnect() {
        let mut source = rule_profile();
        // Preserve coverage for pre-upgrade/provider configs without shipping the old lists.
        let domains: Vec<String> = (0..100_000)
            .map(|index| format!("legacy-site-{index}.example.test"))
            .collect();
        source["route"]["rules"][1]["domain_suffix"] = serde_json::json!(domains);
        let legacy = serde_json::to_string_pretty(&source).unwrap();
        assert!(legacy.len() > 2 * 1024 * 1024);
        let settings = NetworkSettings::default();
        let saved = settings.apply_to_config(&legacy).unwrap();
        assert!(!saved.contains('\n'));
        assert_eq!(settings.apply_to_config(&saved).unwrap(), saved);
        let restored: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(
            restored["route"]["rules"][1]["domain_suffix"],
            source["route"]["rules"][1]["domain_suffix"]
        );
    }

    #[test]
    fn oversized_generated_profiles_remain_bounded() {
        assert_eq!(
            NetworkSettings::default().apply_to_config(&" ".repeat(MAX_CONFIG_BYTES + 1)),
            Err("native.configTooLarge".into())
        );
    }

    #[test]
    fn validates_ports_and_defaults_to_loopback() {
        let mut settings = NetworkSettings::default();
        assert_eq!(settings.listen_address(), "127.0.0.1");
        for port in [0, 1, 1023] {
            settings.proxy_port = port;
            assert!(settings.validate().is_err());
        }
        for port in [1024, 2080, 65535] {
            settings.proxy_port = port;
            assert!(settings.validate().is_ok());
        }
    }
    #[test]
    fn owns_listeners_before_the_native_tunnel_is_created() {
        let settings = NetworkSettings {
            proxy_port: 3456,
            allow_lan: true,
            ..Default::default()
        };
        let raw = r#"{"inbounds":[{"type":"mixed","listen":"::","listen_port":9},{"type":"tun","tag":"tun-in"}],"route":{"final":"select"}}"#;
        let config: Value = serde_json::from_str(&settings.apply_to_config(raw).unwrap()).unwrap();
        assert_eq!(config["inbounds"][0]["listen"], "0.0.0.0");
        assert_eq!(config["inbounds"][0]["listen_port"], 3456);
        assert_eq!(config["inbounds"].as_array().unwrap().len(), 1);
        assert_eq!(config["route"]["final"], "select");
        assert!(settings.apply_to_config("{}").is_err());
    }
    #[test]
    fn ordinary_proxy_profiles_cannot_load_local_key_files() {
        let raw = r#"{"inbounds":[{"type":"mixed"}],"outbounds":[{"type":"trojan","tls":{"key_path":"/tmp/private.key"}}]}"#;
        assert!(NetworkSettings::default().apply_to_config(raw).is_err());
    }
    #[test]
    fn rejects_occupied_ports_and_roundtrips_private_settings() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let settings = NetworkSettings {
            proxy_port: listener.local_addr().unwrap().port(),
            allow_lan: false,
            ..Default::default()
        };
        assert!(settings.ensure_available().is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("network-settings.json");
        assert_eq!(load(&path).unwrap(), NetworkSettings::default());
        crate::storage::write_private(&path, &serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        std::fs::write(&path, "{bad}").unwrap();
        assert!(load(&path).is_err());
    }

    #[test]
    fn tun_settings_default_for_old_files_and_reject_bad_values() {
        // A file saved before TUN was tunable has no `tun` key.
        let old: NetworkSettings =
            serde_json::from_str(r#"{"proxyPort":2080,"allowLan":false}"#).unwrap();
        assert_eq!(old.tun, TunSettings::default());
        assert!(old.validate().is_ok());
        let with = |change: fn(&mut TunSettings)| {
            let mut settings = NetworkSettings::default();
            change(&mut settings.tun);
            settings.validate()
        };
        assert!(with(|t| t.stack = "gvisor".into()).is_ok());
        assert!(with(|t| t.mtu = 9000).is_ok());
        assert!(
            with(|t| t.route_exclude = vec!["192.168.0.0/16".into(), "fc00::/7".into()]).is_ok()
        );
        assert_eq!(
            with(|t| t.stack = "lwip".into()).unwrap_err(),
            "settings.tunStackInvalid"
        );
        assert_eq!(
            with(|t| t.mtu = 1279).unwrap_err(),
            "settings.tunMtuInvalid"
        );
        assert_eq!(
            with(|t| t.mtu = 9001).unwrap_err(),
            "settings.tunMtuInvalid"
        );
        assert_eq!(
            with(|t| t.route_exclude = vec!["192.168.0.1".into()]).unwrap_err(),
            "settings.tunExcludeInvalid"
        );
        assert_eq!(
            with(|t| t.route_exclude = vec!["10.0.0.0/8".into(); MAX_TUN_EXCLUSIONS + 1])
                .unwrap_err(),
            "settings.tunExcludeInvalid"
        );
        // Unknown keys are refused, as for the rest of the file.
        assert!(serde_json::from_str::<NetworkSettings>(r#"{"proxyPort":2080,"allowLan":false,"tun":{"stack":"mixed","mtu":1500,"strictRoute":true,"ipv6":true,"autoRedirect":false,"routeExclude":[],"device":"utun9"}}"#).is_err());
    }

    #[test]
    fn historical_ipv6_default_stays_off_until_the_user_saves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("network-settings.json");
        std::fs::write(
            &path,
            r#"{"proxyPort":2081,"allowLan":true,"tun":{"stack":"mixed","mtu":1500,"strictRoute":true,"ipv6":true,"autoRedirect":false,"routeExclude":[],"killSwitch":false}}"#,
        )
        .unwrap();
        let loaded = load(&path).unwrap();
        assert!(!loaded.tun.ipv6);
        assert!(!loaded.tun.ipv6_chosen);
        let chosen = NetworkSettings {
            tun: TunSettings {
                ipv6: true,
                ipv6_chosen: true,
                ..loaded.tun.clone()
            },
            ..loaded
        };
        crate::storage::write_private(&path, &serde_json::to_vec(&chosen).unwrap()).unwrap();
        let again = load(&path).unwrap();
        assert!(again.tun.ipv6);
        assert!(again.tun.ipv6_chosen);
    }

    #[test]
    fn subscription_cannot_expose_control_services() {
        let raw = r#"{"inbounds":[{"type":"mixed"}],"services":[{"type":"api","listen":"0.0.0.0","listen_port":9090}],"experimental":{"clash_api":{"external_controller":"0.0.0.0:9091"}},"log":{"output":"/tmp/imported-log"}}"#;
        let config: Value =
            serde_json::from_str(&NetworkSettings::default().apply_to_config(raw).unwrap())
                .unwrap();
        assert!(config.get("services").is_none());
        assert!(config.get("experimental").is_none());
        assert!(config["log"].get("output").is_none());
    }
}
