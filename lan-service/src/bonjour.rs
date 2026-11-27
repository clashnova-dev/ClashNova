//! Bonjour / DNS-SD discovery (via `mdns-sd`).
//!
//! The primary cross-platform auto-discovery leg. Advertises this receiver as a
//! `_localsend._tcp` service carrying a TXT record (fingerprint, alias, device
//! type), and browses for the same service type, pushing resolved peers into the
//! shared [`DiscoverySink`]. iOS/macOS/Android peers advertise the same service
//! type and TXT keys via native Bonjour (no multicast entitlement needed), so all
//! platforms interoperate at the DNS-SD level.

use std::collections::HashMap;
use std::net::SocketAddr;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::protocol::{DeviceInfo, DeviceType, TransportProtocol};
use crate::transport::{Discovered, DiscoverySink};

/// DNS-SD service type shared by all ClashNova / LocalSend peers.
pub const SERVICE_TYPE: &str = "_localsend._tcp.local.";

type R<T> = std::result::Result<T, mdns_sd::Error>;

/// A running Bonjour responder + browser. Dropping/stopping unregisters and
/// shuts down the daemon, which ends the browse task.
pub struct Bonjour {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Bonjour {
    /// Register our own service and start browsing. Resolved peers (other than
    /// ourselves) are pushed into `sink`. `fingerprint` is our certificate
    /// fingerprint; `port` is our receiver port.
    pub fn start(
        alias: &str,
        device_type: DeviceType,
        fingerprint: &str,
        port: u16,
        sink: DiscoverySink,
    ) -> R<Self> {
        let daemon = ServiceDaemon::new()?;

        // 广播地址取物理网卡（排除 TUN/VPN），逗号分隔交给 mdns-sd 解析。
        let ips = crate::discovery::usable_interfaces()
            .unwrap_or_default()
            .iter()
            .map(|i| i.ipv4.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let fp_short = &fingerprint[..fingerprint.len().min(8)];
        let host = format!("clashnova-{fp_short}.local.");
        let instance = format!("{}-{fp_short}", sanitize(alias));
        let props: HashMap<String, String> = HashMap::from([
            ("fp".to_string(), fingerprint.to_string()),
            ("alias".to_string(), alias.to_string()),
            ("type".to_string(), device_type_str(device_type)),
            ("proto".to_string(), "https".to_string()),
            ("v".to_string(), "2".to_string()),
        ]);
        let info = ServiceInfo::new(SERVICE_TYPE, &instance, &host, ips.as_str(), port, props)?;
        let fullname = info.get_fullname().to_string();
        daemon.register(info)?;

        let rx = daemon.browse(SERVICE_TYPE)?;
        let me = fingerprint.to_string();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv_async().await {
                let ServiceEvent::ServiceResolved(svc) = event else {
                    continue;
                };
                let txt = &svc.txt_properties;
                let Some(fp) = txt.get_property_val_str("fp") else {
                    continue;
                };
                if fp.is_empty() || fp == me {
                    continue; // 无指纹或自己
                }
                let Some(ip) = svc
                    .addresses
                    .iter()
                    .find(|a| a.is_ipv4())
                    .map(|a| a.to_ip_addr())
                else {
                    continue;
                };
                let info = DeviceInfo {
                    alias: txt.get_property_val_str("alias").unwrap_or("").to_string(),
                    version: txt.get_property_val_str("v").unwrap_or("2").to_string(),
                    device_model: None,
                    device_type: txt.get_property_val_str("type").and_then(parse_device_type),
                    fingerprint: fp.to_string(),
                    port: Some(svc.port),
                    protocol: Some(TransportProtocol::Https),
                    download: None,
                };
                let _ = sink.send(Discovered {
                    info,
                    addr: SocketAddr::new(ip, svc.port),
                });
            }
        });

        Ok(Self { daemon, fullname })
    }

    /// 注销服务并关闭守护，浏览任务随之结束。
    pub fn stop(self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// mDNS 实例名里的点会被当作分隔符，替换掉以免歧义。
fn sanitize(alias: &str) -> String {
    let s: String = alias
        .chars()
        .map(|c| if c == '.' || c == '\\' { '_' } else { c })
        .collect();
    if s.trim().is_empty() {
        "ClashNova".to_string()
    } else {
        s
    }
}

fn device_type_str(t: DeviceType) -> String {
    match t {
        DeviceType::Desktop => "desktop",
        DeviceType::Laptop => "laptop",
        DeviceType::Phone => "phone",
        DeviceType::Tablet => "tablet",
        DeviceType::Server => "server",
        DeviceType::Tv => "tv",
        DeviceType::Unknown => "unknown",
    }
    .to_string()
}

fn parse_device_type(s: &str) -> Option<DeviceType> {
    match s {
        "desktop" => Some(DeviceType::Desktop),
        "laptop" => Some(DeviceType::Laptop),
        "phone" | "mobile" => Some(DeviceType::Phone),
        "tablet" => Some(DeviceType::Tablet),
        "server" | "headless" => Some(DeviceType::Server),
        "tv" => Some(DeviceType::Tv),
        _ => Some(DeviceType::Unknown),
    }
}
