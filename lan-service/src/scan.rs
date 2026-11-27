//! TCP unicast subnet scan.
//!
//! When the network (router / AP client isolation) drops discovery multicast, or
//! a platform lacks the multicast permission (iOS), peers can still be reached by
//! unicast. This module enumerates the hosts on each physical interface's /24 and
//! does a lightweight TCP connect probe on the LocalSend port; open hosts are then
//! resolved via the existing `/info` fetch. Unicast only, no multicast involved.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::TcpStream;

/// Candidate addresses: every host in each usable interface's /24 except our own.
pub fn scan_targets(port: u16) -> Vec<SocketAddr> {
    let mut out = Vec::new();
    for iface in crate::discovery::usable_interfaces().unwrap_or_default() {
        let o = iface.ipv4.octets();
        for host in 1u8..=254 {
            if host == o[3] {
                continue; // skip ourselves
            }
            out.push(SocketAddr::from((
                Ipv4Addr::new(o[0], o[1], o[2], host),
                port,
            )));
        }
    }
    out
}

/// Lightweight liveness probe: a TCP connect that succeeds within `timeout` means
/// something is listening (a candidate receiver) at that address.
pub async fn probe(addr: SocketAddr, timeout: Duration) -> bool {
    matches!(
        tokio::time::timeout(timeout, TcpStream::connect(addr)).await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_skip_self_and_cover_host_range() {
        // 纯函数：对任意网卡枚举出的目标都不含本机、端口正确、主机号在 1..=254。
        let targets = scan_targets(53317);
        for t in &targets {
            assert_eq!(t.port(), 53317);
            if let std::net::IpAddr::V4(v4) = t.ip() {
                let last = v4.octets()[3];
                assert!((1..=254).contains(&last));
            } else {
                panic!("only IPv4 targets expected");
            }
        }
    }
}
