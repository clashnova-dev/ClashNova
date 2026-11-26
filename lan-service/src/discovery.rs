//! 设备发现（LAN-001）。
//!
//! LocalSend 以 UDP 组播（非 mDNS）做发现。为兼容「多网卡 + 代理 TUN」环境，
//! announce 会**逐物理网卡**组播发出（`set_multicast_if_v4` + 绑定该网卡源地址），
//! 并在**每块可用网卡**上加入组播组——避免报文仅从 OS 默认网卡发出而被 sing-box
//! 的 TUN/默认路由吞掉、根本到不了物理局域网（见设计文档第 4.1、11 节）。
//! 另加 5 秒周期重播，覆盖丢包与「晚上线」的对端。明文 best-effort，组播不可用时
//! 降级到手动添加 / HTTP register。

use crate::error::Result;
use crate::protocol::{DeviceInfo, MulticastAnnounce};
use crate::transport::{Discovered, DiscoverySink};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::oneshot;

/// LocalSend 组播地址。选在 224.0.0.0/24（链路本地组播）是因为部分 Android 设备
/// 拒绝其他组播组，且链路本地组播在 macOS 上通常无需 multicast 授权即可收发。
pub const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 167);
/// 组播端口，与 HTTP 端口同为默认 53317。
pub const MULTICAST_PORT: u16 = crate::protocol::DEFAULT_PORT;

/// 周期重播间隔：覆盖丢包、以及在本机之后才上线的对端。
const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(5);

/// 可用于局域网直连的本机网卡地址。
#[derive(Debug, Clone)]
pub struct LocalInterface {
    pub name: String,
    pub ipv4: Ipv4Addr,
}

/// 枚举适合局域网直连的物理网卡 IPv4（私网、非回环）。
///
/// 只在这些网卡上收发组播，避免把发现报文送进 TUN/代理回环（见设计文档第 11 节）。
/// 注意：TUN 虚拟网卡（如 172.19.0.1）也是私网地址；此处做私网+非回环的基础过滤，
/// 并额外按网卡名排除常见虚拟/隧道网卡。即便某块虚拟网卡漏网，逐网卡发送也只是
/// 多发一份无害报文，物理网卡仍能正常把 announce 送上局域网。
pub fn usable_interfaces() -> Result<Vec<LocalInterface>> {
    let mut out = Vec::new();
    for iface in if_addrs::get_if_addrs()? {
        if iface.is_loopback() {
            continue;
        }
        if is_virtual_iface(&iface.name) {
            continue;
        }
        if let IpAddr::V4(ip) = iface.ip() {
            if ip.is_private() {
                out.push(LocalInterface {
                    name: iface.name,
                    ipv4: ip,
                });
            }
        }
    }
    Ok(out)
}

/// 按网卡名排除常见的虚拟/隧道/代理网卡（best-effort，跨平台命名并不统一）。
fn is_virtual_iface(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    const VIRTUAL_PREFIXES: &[&str] = &[
        "utun",      // macOS WireGuard/sing-box TUN、VPN
        "tun",       // 通用 TUN
        "tap",       // 通用 TAP
        "awdl",      // Apple Wireless Direct Link
        "llw",       // Apple low-latency WLAN
        "gif", "stf", // macOS 隧道
        "bridge",    // 桥接（含 Docker/虚拟机桥）
        "vmnet", "vnic", // VMware/虚拟机
        "vboxnet",   // VirtualBox host-only
        "wg",        // WireGuard
        "zt",        // ZeroTier
        "tailscale", // Tailscale
        "docker", "veth", // 容器
        "sing",      // sing-box 命名的 TUN（部分平台）
    ];
    VIRTUAL_PREFIXES.iter().any(|p| n.starts_with(p))
}

/// UDP 组播发现信标（LAN-001）。
///
/// 启动时逐网卡广播本机 announce，并持续监听对端 announce：收到后回填到发现通道；
/// 若对端 `announce=true`，单播回应自身信息（覆盖本机先上线、对端后到的情形）。
/// 同时每 [`ANNOUNCE_INTERVAL`] 周期重播一次。
pub struct Beacon {
    announce: MulticastAnnounce,
    ifaces: Vec<Ipv4Addr>,
    shutdown: Option<oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<()>,
}

impl Beacon {
    pub async fn start(local: DeviceInfo, sink: DiscoverySink) -> Result<Self> {
        let recv_sock = Arc::new(bind_multicast()?);
        let ifaces: Vec<Ipv4Addr> = usable_interfaces()
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.ipv4)
            .collect();
        if ifaces.is_empty() {
            log::warn!("未发现可用物理网卡，组播发现可能不可用，请改用手动添加");
        } else {
            log::info!("组播发现网卡: {ifaces:?}");
        }

        let local_fingerprint = local.fingerprint.clone();
        let announce = MulticastAnnounce {
            device: local,
            announce: true,
        };
        let announce_bytes = serde_json::to_vec(&announce).unwrap_or_default();
        let reply_bytes = {
            let mut reply = announce.clone();
            reply.announce = false; // 单播回应不再要求对端回复，避免回环风暴
            serde_json::to_vec(&reply).unwrap_or_default()
        };

        // 立即逐网卡广播一次。
        send_announce_all(&ifaces, &announce_bytes);

        let (tx, mut rx) = oneshot::channel::<()>();
        let recv = recv_sock.clone();
        let task_ifaces = ifaces.clone();
        let join = tokio::spawn(async move {
            // 持有 recv_sock 的所有权，保证套接字在任务存活期间不被释放。
            let _keepalive = recv_sock;
            let mut buf = vec![0u8; 64 * 1024];
            let mut ticker = tokio::time::interval(ANNOUNCE_INTERVAL);
            ticker.tick().await; // 消费立即触发的首个 tick（启动已显式广播过）
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    _ = ticker.tick() => {
                        send_announce_all(&task_ifaces, &announce_bytes);
                    }
                    res = recv.recv_from(&mut buf) => {
                        let Ok((n, src)) = res else { continue; };
                        let Ok(msg) = serde_json::from_slice::<MulticastAnnounce>(&buf[..n]) else {
                            continue;
                        };
                        // 忽略自己发出的报文。
                        if msg.device.fingerprint == local_fingerprint {
                            continue;
                        }
                        let port = msg.device.port.unwrap_or(MULTICAST_PORT);
                        let _ = sink.send(Discovered {
                            info: msg.device.clone(),
                            addr: SocketAddr::new(src.ip(), port),
                        });
                        // 对端要求回复则单播回应（announce=false，避免回环广播风暴）。
                        if msg.announce && !reply_bytes.is_empty() {
                            let _ = recv.send_to(&reply_bytes, src).await;
                        }
                    }
                }
            }
        });

        Ok(Self {
            announce,
            ifaces,
            shutdown: Some(tx),
            join,
        })
    }

    /// 主动重新广播本机 announce（逐网卡）。
    pub async fn announce(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&self.announce)?;
        send_announce_all(&self.ifaces, &bytes);
        Ok(())
    }

    /// 停止监听并释放组播套接字。
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.join.await;
    }
}

/// 逐物理网卡把 announce 组播出去。为每块网卡显式设定组播出口并绑定源地址，
/// 确保报文真正从物理局域网口离开，而非被 TUN/默认路由吞掉。
fn send_announce_all(ifaces: &[Ipv4Addr], bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let group = SockAddr::from(SocketAddr::from((MULTICAST_ADDR, MULTICAST_PORT)));
    if ifaces.is_empty() {
        // 没枚举到网卡时退化为 OS 默认网卡发送一次。
        if let Ok(sock) = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
            let _ = sock.send_to(bytes, &group);
        }
        return;
    }
    for ip in ifaces {
        if let Err(e) = send_on_iface(*ip, bytes, &group) {
            log::debug!("网卡 {ip} 组播发送失败: {e}");
        }
    }
}

/// 在指定网卡上发送一次组播报文。
fn send_on_iface(ip: Ipv4Addr, bytes: &[u8], group: &SockAddr) -> std::io::Result<()> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    sock.set_multicast_if_v4(&ip)?;
    // 绑定到该网卡 IP，使报文源地址正确（对端据此单播回应 / 建连）。
    sock.bind(&SockAddr::from(SocketAddr::from((ip, 0))))?;
    sock.send_to(bytes, group)?;
    Ok(())
}

/// 绑定接收用组播套接字：复用地址/端口、在每块可用网卡加入组播组、开启回环。
fn bind_multicast() -> Result<UdpSocket> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    #[cfg(unix)]
    sock.set_reuse_port(true)?;
    let bind_addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, MULTICAST_PORT));
    sock.bind(&SockAddr::from(bind_addr))?;

    // 在每块可用物理网卡上加入组播组（而非 UNSPECIFIED 让 OS 只选一块、常被 TUN 抢占）。
    let ifaces = usable_interfaces().unwrap_or_default();
    let mut joined = 0usize;
    for iface in &ifaces {
        match sock.join_multicast_v4(&MULTICAST_ADDR, &iface.ipv4) {
            Ok(()) => joined += 1,
            Err(e) => log::debug!("网卡 {} 加入组播失败: {e}", iface.ipv4),
        }
    }
    if joined == 0 {
        // 没有任何网卡成功加入，退回 UNSPECIFIED（让 OS 选择），至少不比从前差。
        let _ = sock.join_multicast_v4(&MULTICAST_ADDR, &Ipv4Addr::UNSPECIFIED);
    }

    sock.set_multicast_loop_v4(true)?;
    sock.set_nonblocking(true)?;
    let std_sock: std::net::UdpSocket = sock.into();
    Ok(UdpSocket::from_std(std_sock)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_ifaces_are_excluded() {
        assert!(is_virtual_iface("utun4"));
        assert!(is_virtual_iface("tun0"));
        assert!(is_virtual_iface("awdl0"));
        assert!(is_virtual_iface("bridge100"));
        assert!(is_virtual_iface("vboxnet0"));
        assert!(!is_virtual_iface("en0"));
        assert!(!is_virtual_iface("eth0"));
        assert!(!is_virtual_iface("wlan0"));
    }

    #[test]
    fn usable_interfaces_does_not_error() {
        // 具体结果依赖运行环境，这里只验证不 panic/报错。
        let _ = usable_interfaces().expect("enumerate interfaces");
    }
}
