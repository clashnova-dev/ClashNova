//! Private, authenticated IPC for the installed TUN service. No shell/path RPC.
pub mod message;
pub mod runtime_log;
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use message::{msg, text};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub const PORT: u16 = 47653;
/// Bumped whenever the service accepts or returns something an older build would not:
/// an app talking to a service of another protocol reports a mismatch and offers a repair
/// instead of sending a config the installed service would reject.
/// 2: device options (stack, route exclusions, Linux auto_redirect), binary digests in responses,
///    and the kill switch (`Arm`, `Release`, `blocking`).
pub const PROTOCOL: u32 = 2;
/// The virtual device's own addresses. They are fixed because the kill switch recognises
/// traffic that is already inside the tunnel by them.
pub const TUN_ADDRESS_V4: &str = "172.19.0.1";
pub const TUN_ADDRESS_V6: &str = "fdfe:dcba:9876::1";
pub const TUN_PREFIX_V4: &str = "172.19.0.1/30";
pub const TUN_PREFIX_V6: &str = "fdfe:dcba:9876::1/126";
/// The group the core runs under on Unix. It names no group on the system; the kill switch
/// lets sockets owned by it through, which is how the core still reaches the proxy servers
/// while everything else is blocked.
pub const CORE_GID: u32 = 47653;
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
pub const LEASE_SECONDS: u64 = 15;
pub const SERVICE: &str = "dev.airport.tun";
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub key: [u8; 32],
}
/// How long one IPC exchange may take. The caller usually holds the app's connection state
/// while it waits, so a call that cannot take long must not be given long to fail.
#[derive(Clone, Copy)]
pub struct Timeouts {
    pub connect: Duration,
    pub read: Duration,
    pub write: Duration,
}
impl Timeouts {
    /// Start validates the config with the core before answering, so it needs a long read.
    pub const CONTROL: Self = Self {
        connect: Duration::from_secs(2),
        read: Duration::from_secs(12),
        write: Duration::from_secs(3),
    };
    /// A heartbeat is answered from memory.
    pub const HEARTBEAT: Self = Self {
        connect: Duration::from_secs(1),
        read: Duration::from_secs(4),
        write: Duration::from_secs(2),
    };
}
impl Credentials {
    pub fn generate() -> Result<Self> {
        let mut key = [0; 32];
        getrandom::fill(&mut key).map_err(|e| e.to_string())?;
        Ok(Self { key })
    }
    pub fn call(&self, action: Action) -> Result<Response> {
        let response = self.exchange(action, Timeouts::CONTROL)?;
        if let Some(error) = &response.error {
            return Err(error.clone());
        }
        Ok(response)
    }
    /// One authenticated round trip. `Err` means the exchange itself failed (service not
    /// reachable, timed out, not authentic); a refusal by the service comes back as
    /// `Ok` with `error` set. Callers that must tell a flaky exchange from a definite
    /// answer, such as the heartbeat, use this instead of `call`.
    pub fn exchange(&self, action: Action, timeouts: Timeouts) -> Result<Response> {
        let request = Request {
            id: random_id()?,
            time: now(),
            action,
        };
        let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
        let mut stream = TcpStream::connect_timeout(&addr, timeouts.connect)
            .map_err(|_| "native.tunServiceNotRunning")?;
        stream
            .set_read_timeout(Some(timeouts.read))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(timeouts.write))
            .map_err(|e| e.to_string())?;
        write_message(&mut stream, &self.key, b"airport/request/v1", &request)?;
        let response: Response = read_message(&mut stream, &self.key, b"airport/response/v1")?;
        if response.id != request.id || response.protocol != PROTOCOL {
            return Err("native.tunServiceMismatch".into());
        }
        Ok(response)
    }
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Status,
    Start {
        config: Value,
        session: String,
    },
    Heartbeat {
        session: String,
    },
    Stop {
        session: String,
    },
    /// The app has verified that this session's tunnel carries traffic. Lifts a block left by an
    /// earlier exit, and from now on blocks the network if the core exits without being asked to.
    Arm {
        session: String,
    },
    /// The user disconnected: lift the block and stop protecting.
    Release,
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub time: u64,
    pub action: Action,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub id: String,
    pub protocol: u32,
    pub running: bool,
    pub session: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub logs: Vec<String>,
    /// SHA-256 of the core the service runs and of the service binary itself. The app compares
    /// them with the copies it ships: after an app update the installed, privileged copies stay
    /// as they were until the service is repaired, and the app should say so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helper_sha256: Option<String>,
    /// The kill switch is blocking the network: the core exited unexpectedly while armed.
    #[serde(default)]
    pub blocking: bool,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn random_id() -> Result<String> {
    let mut data = [0u8; 16];
    getrandom::fill(&mut data).map_err(|e| e.to_string())?;
    Ok(data.iter().map(|b| format!("{b:02x}")).collect())
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit())
}
/// An IP prefix such as `192.168.0.0/16` or `fc00::/7`.
pub fn valid_cidr(value: &str) -> bool {
    let Some((address, prefix)) = value.split_once('/') else {
        return false;
    };
    if prefix.is_empty() || prefix.len() > 3 || !prefix.bytes().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let Ok(prefix) = prefix.parse::<u8>() else {
        return false;
    };
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(_)) => prefix <= 32,
        Ok(IpAddr::V6(_)) => prefix <= 128,
        Err(_) => false,
    }
}
pub const TUN_STACKS: [&str; 3] = ["mixed", "system", "gvisor"];
pub const MIN_TUN_MTU: u64 = 1280;
pub const MAX_TUN_MTU: u64 = 65535;
pub const MAX_ROUTE_EXCLUSIONS: usize = 256;

/// The virtual device may only be shaped in ways the app offers. Everything here runs as root:
/// an option is accepted because it was reviewed, never because the core happens to know it.
fn check_tun(inbound: &Value) -> Result<()> {
    let invalid = |option: &str| msg("native.tunDeviceOptionInvalid", &[("option", text(option))]);
    // Capturing all traffic and its DNS is the point of the mode; a request cannot ask for a
    // device that leaves either outside.
    if inbound["auto_route"] != true || inbound["dns_mode"] != "hijack" {
        return Err("native.tunRouteDnsInvalid".into());
    }
    let prefixes = |name: &str, most: usize| -> Result<()> {
        match &inbound[name] {
            Value::Null => Ok(()),
            Value::Array(items)
                if items.len() <= most
                    && items
                        .iter()
                        .all(|item| item.as_str().is_some_and(valid_cidr)) =>
            {
                Ok(())
            }
            _ => Err(invalid(name)),
        }
    };
    // The kill switch tells tunnelled traffic by these addresses, so no other is accepted.
    if inbound["address"] != serde_json::json!([TUN_PREFIX_V4, TUN_PREFIX_V6]) {
        return Err(invalid("address"));
    }
    prefixes("route_exclude_address", MAX_ROUTE_EXCLUSIONS)?;
    match &inbound["mtu"] {
        Value::Null => {}
        mtu if mtu
            .as_u64()
            .is_some_and(|mtu| (MIN_TUN_MTU..=MAX_TUN_MTU).contains(&mtu)) => {}
        _ => return Err(invalid("mtu")),
    }
    match &inbound["stack"] {
        Value::Null => {}
        stack
            if stack
                .as_str()
                .is_some_and(|stack| TUN_STACKS.contains(&stack)) => {}
        _ => return Err(invalid("stack")),
    }
    if !matches!(inbound["strict_route"], Value::Null | Value::Bool(_)) {
        return Err(invalid("strict_route"));
    }
    match inbound["auto_redirect"] {
        Value::Null | Value::Bool(false) => {}
        // nftables redirection exists on Linux only; elsewhere the core would refuse to start.
        Value::Bool(true) if cfg!(target_os = "linux") => {}
        _ => return Err(invalid("auto_redirect")),
    }
    Ok(())
}
pub fn write_message<T: Serialize>(
    stream: &mut impl Write,
    key: &[u8; 32],
    domain: &[u8],
    message: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FRAME - 64 {
        return Err("native.tunRequestTooLarge".into());
    }
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "native.tunCredentialsInvalid")?;
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &bytes,
                aad: domain,
            },
        )
        .map_err(|_| "native.tunEncryptFailed")?;
    stream
        .write_all(&((encrypted.len() + 12) as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&nonce))
        .and_then(|_| stream.write_all(&encrypted))
        .map_err(|e| e.to_string())
}
pub fn read_message<T: for<'a> Deserialize<'a>>(
    stream: &mut impl Read,
    key: &[u8; 32],
    domain: &[u8],
) -> Result<T> {
    let mut len = [0; 4];
    stream.read_exact(&mut len).map_err(|e| e.to_string())?;
    let len = u32::from_be_bytes(len) as usize;
    if !(28..=MAX_FRAME).contains(&len) {
        return Err("native.tunMessageLengthInvalid".into());
    }
    let mut bytes = vec![0; len];
    stream.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "native.tunCredentialsInvalid")?;
    let plain = cipher
        .decrypt(
            Nonce::from_slice(&bytes[..12]),
            Payload {
                msg: &bytes[12..],
                aad: domain,
            },
        )
        .map_err(|_| "native.tunAuthFailed")?;
    serde_json::from_slice(&plain).map_err(|_| "native.tunMessageInvalid".into())
}

/// The destinations a validated config keeps off the virtual device. They stay reachable
/// while the kill switch blocks, as they are when the tunnel is up.
pub fn route_exclusions(config: &Value) -> Vec<String> {
    config["inbounds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|inbound| inbound["type"] == "tun")
        .flat_map(|inbound| {
            inbound["route_exclude_address"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|prefix| prefix.as_str().map(str::to_owned))
        .filter(|prefix| valid_cidr(prefix))
        .collect()
}

/// Subscription data cannot ask the core to read/write local files or execute commands.
/// Shared by unprivileged connections, latency tests and the privileged TUN service.
pub fn validate_profile_options(config: &Value) -> Result<()> {
    fn inspect(value: &Value, parent: &str, depth: usize) -> Result<()> {
        if depth > 32 {
            return Err("native.tunConfigTooDeep".into());
        }
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    // rule_set 有两种形态,都要安全:规则里是标签引用(字符串),放行;`route.rule_set`
                    // 定义里是对象,只放行 SagerNet 官方规则集(经 jsDelivr 可信镜像)的远程 https 源
                    // ——local 规则集会让 root 核心读本地文件,非 SagerNet 路径(jsDelivr 可托管任意
                    // 仓库)、非 https 一律拒绝。改源要同步改这里。下方通用检查还会再挡任何内嵌 path。
                    if key == "rule_set"
                        && !value.as_array().is_some_and(|items| {
                            items.iter().all(|item| {
                                item.is_string()
                                    || (item["type"].as_str() == Some("remote")
                                        && item["url"].as_str().is_some_and(|url| {
                                            url.starts_with("https://cdn.jsdelivr.net/gh/SagerNet/")
                                        }))
                            })
                        })
                    {
                        return Err(msg(
                            "native.tunConfigForbiddenOption",
                            &[("option", text("rule_set"))],
                        ));
                    }
                    if (key.ends_with("_path") && key != "process_path")
                        || key.ends_with("_paths")
                        || key.ends_with("_file")
                        || key.ends_with("_files")
                        || key.ends_with("_directory")
                        || matches!(
                            key.as_str(),
                            "include"
                                | "acme"
                                | "cache_file"
                                | "external_ui"
                                | "external_ui_download_url"
                                | "output"
                                | "directory"
                                | "working_directory"
                                | "command"
                                | "script"
                                // Entering another network namespace is a path lookup as root.
                                | "netns"
                        )
                        || (key == "path"
                            && parent != "transport"
                            && !(parent == "servers"
                                && map.get("type").and_then(Value::as_str) == Some("https")))
                    {
                        return Err(msg(
                            "native.tunConfigForbiddenOption",
                            &[("option", text(key))],
                        ));
                    }
                    inspect(value, key, depth + 1)?;
                }
            }
            Value::Array(items) => {
                for item in items {
                    inspect(item, parent, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    inspect(config, "", 0)
}

/// Imported profiles must never grant a root core arbitrary filesystem access.
/// Only a closed protocol set is supported; local rule files/ACME/plugins are rejected.
pub fn safe_config(mut config: Value) -> Result<Value> {
    let object = config.as_object_mut().ok_or("native.tunConfigInvalid")?;
    if object.keys().any(|k| {
        !matches!(
            k.as_str(),
            "log" | "dns" | "route" | "inbounds" | "outbounds" | "experimental"
        )
    }) {
        return Err("native.tunConfigUnsupportedTopLevel".into());
    }
    object.insert(
        "log".into(),
        serde_json::json!({"level":"info", "timestamp":false}),
    );
    let api = config["experimental"]["clash_api"].clone();
    let endpoint = api["external_controller"]
        .as_str()
        .ok_or("native.tunControllerMissing")?
        .parse::<SocketAddr>()
        .map_err(|_| "native.tunControllerAddressInvalid")?;
    if !endpoint.ip().is_loopback()
        || endpoint.port() < 1024
        || api["secret"].as_str().unwrap_or_default().len() < 32
    {
        return Err("native.tunControllerInsecure".into());
    }
    config["experimental"] = serde_json::json!({"clash_api":{"external_controller":endpoint.to_string(),"secret":api["secret"]}});
    let inbounds = config["inbounds"]
        .as_array()
        .ok_or("native.tunInboundsMissing")?;
    if inbounds.len() != 2 || inbounds.iter().filter(|i| i["type"] == "tun").count() != 1 {
        return Err("native.tunInboundsInvalid".into());
    }
    for inbound in inbounds {
        let fields = inbound.as_object().ok_or("native.tunInboundInvalid")?;
        let allowed: &[&str] = if inbound["type"] == "tun" {
            &[
                "type",
                "tag",
                "address",
                "mtu",
                "auto_route",
                "strict_route",
                "dns_mode",
                "stack",
                "auto_redirect",
                "route_exclude_address",
            ]
        } else {
            &["type", "tag", "listen", "listen_port"]
        };
        if fields.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err("native.tunInboundExtraOptions".into());
        }
        match inbound["type"].as_str() {
            Some("tun") => check_tun(inbound)?,
            Some("mixed") => {
                let port = inbound["listen_port"].as_u64().unwrap_or(0);
                if !(1024..=65535).contains(&port)
                    || !matches!(inbound["listen"].as_str(), Some("127.0.0.1" | "0.0.0.0"))
                {
                    return Err("native.tunMixedInboundInvalid".into());
                }
            }
            _ => return Err("native.tunExtraListeners".into()),
        }
    }
    let outbounds = config["outbounds"]
        .as_array()
        .ok_or("native.tunOutboundsMissing")?;
    if outbounds.len() > 2048 {
        return Err("native.tunTooManyNodes".into());
    }
    for outbound in outbounds {
        if !matches!(
            outbound["type"].as_str(),
            Some(
                "direct"
                    | "block"
                    | "dns"
                    | "selector"
                    | "urltest"
                    | "shadowsocks"
                    | "vmess"
                    | "vless"
                    | "trojan"
                    | "hysteria"
                    | "hysteria2"
                    | "tuic"
                    | "socks"
                    | "http"
                    | "anytls"
            )
        ) {
            return Err("native.tunProtocolUnsupported".into());
        }
    }
    // Native DoH has an HTTP URI path, not a filesystem path.
    if let Some(servers) = config["dns"]["servers"].as_array_mut() {
        for server in servers {
            if server["type"] == "https" {
                if server["path"].as_str().is_some_and(|p| p != "/dns-query") {
                    return Err("native.tunDnsPathInvalid".into());
                }
                server.as_object_mut().unwrap().remove("path");
            }
        }
    }
    validate_profile_options(&config)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ipc_rejects_tampering_wrong_key_and_reflection() {
        let c = Credentials::generate().unwrap();
        let req = Request {
            id: random_id().unwrap(),
            time: now(),
            action: Action::Status,
        };
        let mut bytes = vec![];
        write_message(&mut bytes, &c.key, b"request", &req).unwrap();
        let decoded: Request = read_message(&mut &bytes[..], &c.key, b"request").unwrap();
        assert_eq!(decoded.id, req.id);
        assert!(read_message::<Request>(&mut &bytes[..], &[1; 32], b"request").is_err());
        assert!(read_message::<Request>(&mut &bytes[..], &c.key, b"response").is_err());
        let n = bytes.len();
        bytes[n - 1] ^= 1;
        assert!(read_message::<Request>(&mut &bytes[..], &c.key, b"request").is_err());
    }
    #[test]
    fn bounds_frame_before_allocating() {
        assert!(
            read_message::<Request>(&mut &[255, 255, 255, 255][..], &[0; 32], b"request").is_err()
        );
    }
    fn config() -> Value {
        serde_json::json!({"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":2080},{"type":"tun","address":[TUN_PREFIX_V4,TUN_PREFIX_V6],"auto_route":true,"dns_mode":"hijack"}],"outbounds":[{"type":"direct","tag":"direct"}],"experimental":{"clash_api":{"external_controller":"127.0.0.1:29999","secret":"a".repeat(64)}}})
    }
    #[test]
    fn denies_privileged_file_access_and_unknown_protocol() {
        let mut c = config();
        c["outbounds"][0]["tls"] = serde_json::json!({"certificate_path":"/etc/shadow"});
        assert!(safe_config(c).is_err());
        let mut c = config();
        c["outbounds"][0]["type"] = "ssh".into();
        assert!(safe_config(c).is_err());
        let mut c = config();
        c["route"] = serde_json::json!({"rule_set":[{"path":"/etc/shadow"}]});
        assert!(safe_config(c).is_err());
        assert!(safe_config(config()).is_ok());
    }
    #[test]
    fn allows_only_trusted_remote_rule_sets() {
        // 规则里的标签引用(字符串)放行,配套定义为可信镜像上的远程 https 源。
        let mut c = config();
        c["route"]["rule_set"] = serde_json::json!([
            {"type":"remote","tag":"geosite-cn","format":"binary","url":"https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs"}
        ]);
        c["route"]["rules"] = serde_json::json!([{"rule_set":["geosite-cn"],"outbound":"direct"}]);
        assert!(safe_config(c.clone()).is_ok());
        // local 规则集(root 读本地文件)拒绝。
        c["route"]["rule_set"] =
            serde_json::json!([{"type":"local","tag":"x","format":"binary","path":"/etc/shadow"}]);
        assert!(safe_config(c.clone()).is_err());
        // 非白名单主机拒绝。
        c["route"]["rule_set"] = serde_json::json!([{"type":"remote","tag":"x","format":"binary","url":"https://evil.example/x.srs"}]);
        assert!(safe_config(c.clone()).is_err());
        // jsDelivr 主机但非 SagerNet 路径(可托管任意仓库)拒绝。
        c["route"]["rule_set"] = serde_json::json!([{"type":"remote","tag":"x","format":"binary","url":"https://cdn.jsdelivr.net/gh/evil/repo@x/y.srs"}]);
        assert!(safe_config(c.clone()).is_err());
        // 非 https 拒绝。
        c["route"]["rule_set"] = serde_json::json!([{"type":"remote","tag":"x","format":"binary","url":"http://cdn.jsdelivr.net/x.srs"}]);
        assert!(safe_config(c.clone()).is_err());
        // 规则 matcher 里混入对象想夹带 local path,拒绝。
        c["route"]["rule_set"] = serde_json::json!([]);
        c["route"]["rules"] = serde_json::json!([{"rule_set":[{"type":"local","path":"/etc/shadow"}],"outbound":"direct"}]);
        assert!(safe_config(c).is_err());
    }
    #[test]
    fn accepts_live_app_slots_without_granting_file_or_listener_access() {
        let mut c = config();
        c["outbounds"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"type":"selector","tag":"select","outbounds":["direct"]}));
        c["route"] = serde_json::json!({"find_process":true,"rules":[
            {"inbound":["clashnova-app-block"],"process_path_regex":["^$"],"action":"reject"},
            {"inbound":["clashnova-app-proxy"],"process_path_regex":["^$"],"outbound":"select"},
            {"inbound":["clashnova-app-direct"],"process_path_regex":["^$"],"outbound":"direct"}
        ]});
        let checked = safe_config(c.clone()).unwrap();
        assert_eq!(checked["route"]["rules"].as_array().unwrap().len(), 3);
        c["route"]["rules"][0]["path"] = "/etc/shadow".into();
        assert!(safe_config(c).is_err());
    }
    #[test]
    fn untrusted_profile_options_block_local_access_but_keep_uri_paths() {
        for value in [
            serde_json::json!({"tls":{"key_path":"/tmp/private.key"}}),
            serde_json::json!({"dns":{"servers":[{"type":"hosts","path":"/etc/hosts"}]}}),
            serde_json::json!({"tls":{"acme":{"data_directory":"/tmp/data"}}}),
            serde_json::json!({"command":"not-to-be-executed"}),
        ] {
            assert!(validate_profile_options(&value).is_err());
        }
        assert!(validate_profile_options(&serde_json::json!({"transport":{"type":"ws","path":"/proxy"},"dns":{"servers":[{"type":"https","path":"/dns-query"}]},"route":{"rules":[{"process_path":["/Applications/QA.app/Contents/MacOS/QA"]}]}})).is_ok());
    }
    #[test]
    fn device_options_are_limited_to_reviewed_values() {
        let with = |options: Value| {
            let mut c = config();
            for (key, value) in options.as_object().unwrap() {
                c["inbounds"][1][key] = value.clone();
            }
            safe_config(c)
        };
        assert!(with(serde_json::json!({
            "address":["172.19.0.1/30","fdfe:dcba:9876::1/126"], "mtu":1500, "stack":"mixed",
            "strict_route":true, "route_exclude_address":["192.168.0.0/16","fc00::/7"]
        }))
        .is_ok());
        assert_eq!(
            with(serde_json::json!({"auto_redirect":true})).is_ok(),
            cfg!(target_os = "linux")
        );
        assert!(with(serde_json::json!({"auto_redirect":false})).is_ok());
        for bad in [
            serde_json::json!({"auto_route":false}),
            serde_json::json!({"dns_mode":"disabled"}),
            serde_json::json!({"mtu":1279}),
            serde_json::json!({"mtu":65536}),
            serde_json::json!({"mtu":"1500"}),
            serde_json::json!({"stack":"lwip"}),
            serde_json::json!({"strict_route":"yes"}),
            serde_json::json!({"auto_redirect":"true"}),
            serde_json::json!({"address":["172.19.0.1/30","fdfe:dcba:9876::1/126","10.0.0.1/8"]}),
            serde_json::json!({"address":["not-an-address"]}),
            // Any address but the fixed pair: the kill switch would not recognise the tunnel.
            serde_json::json!({"address":["10.10.0.1/30","fdfe:dcba:9876::1/126"]}),
            serde_json::json!({"address":["172.19.0.1/30"]}),
            serde_json::json!({"route_exclude_address":["192.168.0.0/33"]}),
            serde_json::json!({"route_exclude_address":"192.168.0.0/16"}),
            // Options the core knows but the service was never asked to review.
            serde_json::json!({"interface_name":"utun9"}),
            serde_json::json!({"include_uid":[0]}),
            serde_json::json!({"netns":"/run/netns/x"}),
            serde_json::json!({"platform":{"http_proxy":{"enabled":true}}}),
        ] {
            assert!(with(bad.clone()).is_err(), "accepted {bad}");
        }
    }
    #[test]
    fn route_exclusions_come_from_the_device_only() {
        let mut c = config();
        c["inbounds"][1]["route_exclude_address"] =
            serde_json::json!(["192.168.0.0/16", "fc00::/7"]);
        c["inbounds"][0]["route_exclude_address"] = serde_json::json!(["10.0.0.0/8"]);
        assert_eq!(route_exclusions(&c), ["192.168.0.0/16", "fc00::/7"]);
        assert!(route_exclusions(&config()).is_empty());
    }
    #[test]
    fn recognises_ip_prefixes() {
        for ok in [
            "0.0.0.0/0",
            "10.0.0.0/8",
            "192.168.1.1/32",
            "::/0",
            "fc00::/7",
            "fe80::1/128",
        ] {
            assert!(valid_cidr(ok), "{ok}");
        }
        for bad in [
            "",
            "10.0.0.0",
            "10.0.0.0/",
            "10.0.0.0/33",
            "::/129",
            "10.0.0.0/+8",
            "10.0.0.0/08x",
            "a/8",
            "10.0.0.0/8/8",
            " 10.0.0.0/8",
            "10.0.0.0/ 8",
            "10.0.0.0/0008",
        ] {
            assert!(!valid_cidr(bad), "{bad}");
        }
    }
    #[test]
    fn denies_file_and_namespace_options_wherever_they_appear() {
        for (section, value) in [
            (
                "outbounds",
                serde_json::json!([{"type":"direct","tag":"direct","netns":"/run/netns/x"}]),
            ),
            (
                "dns",
                serde_json::json!({"servers":[{"type":"dhcp","dhcp_lease_files":["/etc/shadow"]}]}),
            ),
            (
                "route",
                serde_json::json!({"rules":[{"pid_file":"/run/x.pid","outbound":"direct"}]}),
            ),
            (
                "dns",
                serde_json::json!({"servers":[{"type":"hosts","path":["/etc/shadow"]}]}),
            ),
            ("route", serde_json::json!({"state_directory":"/var/lib/x"})),
        ] {
            let mut c = config();
            c[section] = value;
            assert!(
                safe_config(c).is_err(),
                "accepted a file option in {section}"
            );
        }
    }
    #[test]
    fn rejects_public_controller() {
        let mut c = config();
        c["experimental"]["clash_api"]["external_controller"] = "0.0.0.0:12345".into();
        assert!(safe_config(c).is_err());
    }
}
