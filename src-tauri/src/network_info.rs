use crate::{connection::Phase, tray_speed::Endpoint, Core};
use serde::Serialize;
use std::{
    net::{IpAddr, ToSocketAddrs},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::State;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitInfo {
    pub(crate) ip: String,
    country: String,
    pub(crate) via_proxy: bool,
    checked_at: String,
    geo: Option<GeoInfo>,
    source: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoInfo {
    asn: Option<u64>,
    isp: String,
    organization: String,
    region: String,
    city: String,
    timezone: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
}
fn parse_geo(raw: &[u8], via_proxy: bool) -> Result<ExitInfo, String> {
    let data: serde_json::Value =
        serde_json::from_slice(raw).map_err(|_| "native.geoDataInvalid")?;
    if data["success"] != true {
        return Err("native.geoUnavailable".into());
    }
    let text = |value: &serde_json::Value| {
        value
            .as_str()
            .unwrap_or("")
            .chars()
            .take(160)
            .collect::<String>()
    };
    let ip = data["ip"]
        .as_str()
        .ok_or("native.exitIpMissing")?
        .parse::<IpAddr>()
        .map_err(|_| "native.exitIpInvalid")?;
    let country = data["country_code"]
        .as_str()
        .filter(|s| s.len() == 2 && s.bytes().all(|b| b.is_ascii_uppercase()))
        .unwrap_or("");
    Ok(ExitInfo {
        ip: ip.to_string(),
        country: country.into(),
        via_proxy,
        checked_at: chrono::Utc::now().to_rfc3339(),
        source: "IPWHOIS".into(),
        geo: Some(GeoInfo {
            asn: data["connection"]["asn"]
                .as_u64()
                .filter(|n| *n <= u32::MAX as u64),
            isp: text(&data["connection"]["isp"]),
            organization: text(&data["connection"]["org"]),
            region: text(&data["region"]),
            city: text(&data["city"]),
            timezone: text(&data["timezone"]["id"]),
            latitude: data["latitude"]
                .as_f64()
                .filter(|n| n.is_finite() && (-90.0..=90.0).contains(n)),
            longitude: data["longitude"]
                .as_f64()
                .filter(|n| n.is_finite() && (-180.0..=180.0).contains(n)),
        }),
    })
}
fn parse_trace(text: &str, via_proxy: bool) -> Result<ExitInfo, String> {
    let field = |key: &str| text.lines().find_map(|line| line.strip_prefix(key));
    let ip = field("ip=")
        .ok_or("native.exitIpMissing")?
        .parse::<IpAddr>()
        .map_err(|_| "native.exitIpInvalid")?;
    let country = field("loc=")
        .filter(|s| s.len() == 2 && s.bytes().all(|b| b.is_ascii_uppercase()))
        .unwrap_or("");
    Ok(ExitInfo {
        ip: ip.to_string(),
        country: country.into(),
        via_proxy,
        checked_at: chrono::Utc::now().to_rfc3339(),
        geo: None,
        source: "Cloudflare".into(),
    })
}
fn current(state: &Arc<Mutex<Core>>) -> Result<(Option<Endpoint>, u16), String> {
    let core = state.try_lock().map_err(|_| "native.connectionStateBusy")?;
    if !matches!(core.phase, Phase::Idle | Phase::Failed | Phase::Connected) {
        return Err("native.connectionStateBusy".into());
    }
    Ok((
        if core.phase == Phase::Connected {
            core.traffic_endpoint.clone()
        } else {
            None
        },
        core.network.proxy_port,
    ))
}
async fn bounded(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "native.networkInfoReadFailed")?
    {
        if bytes.len() + chunk.len() > 65536 {
            return Err("native.networkInfoTooLarge".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
#[tauri::command]
pub async fn get_exit_info(state: State<'_, Arc<Mutex<Core>>>) -> Result<ExitInfo, String> {
    let (endpoint, port) = current(state.inner())?;
    let result = query_exit_info(endpoint.as_ref().map(|_| port)).await?;
    if current(state.inner())?.0 != endpoint {
        return Err("native.networkChangedRecheckExit".into());
    }
    Ok(result)
}

pub(crate) async fn query_exit_info(proxy_port: Option<u16>) -> Result<ExitInfo, String> {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8));
    if let Some(port) = proxy_port {
        builder = builder.proxy(
            reqwest::Proxy::all(format!("http://127.0.0.1:{port}"))
                .map_err(|_| "native.localProxyUnavailable")?,
        );
    }
    let client = builder
        .build()
        .map_err(|_| "native.exitCheckClientFailed")?;
    // One geolocation request follows the same proxy path as the detected IP.
    // On provider/rate-limit failure retain basic IP information via trace.
    if let Ok(response) = client
        .get("https://ipwho.is/")
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        if response.status().is_success() {
            if let Ok(raw) = bounded(response).await {
                if let Ok(info) = parse_geo(&raw, proxy_port.is_some()) {
                    return Ok(info);
                }
            }
        }
    }
    let response = client
        .get("https://www.cloudflare.com/cdn-cgi/trace")
        .send()
        .await
        .map_err(|_| "native.exitIpQueryFailed")?
        .error_for_status()
        .map_err(|_| "native.exitCheckUnavailable")?;
    let raw = bounded(response).await?;
    parse_trace(
        std::str::from_utf8(&raw).map_err(|_| "native.exitCheckFormatInvalid")?,
        proxy_port.is_some(),
    )
}

#[derive(Serialize)]
pub struct AutoSelection {
    tag: String,
    delay: Option<u64>,
}
#[tauri::command]
pub async fn get_auto_selection(
    state: State<'_, Arc<Mutex<Core>>>,
) -> Result<Option<AutoSelection>, String> {
    let (endpoint, _) = current(state.inner())?;
    let Some(ref endpoint) = endpoint else {
        return Ok(None);
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|_| "native.autoNodeClientFailed")?;
    let response = endpoint
        .request(&client, reqwest::Method::GET, "/proxies/__airport_auto__")
        .send()
        .await
        .map_err(|_| "native.autoNodeUnavailable")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let raw = bounded(
        response
            .error_for_status()
            .map_err(|_| "native.autoNodeUnavailable")?,
    )
    .await?;
    let value: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|_| "native.autoNodeDataInvalid")?;
    let tag = value["now"].as_str().unwrap_or("").to_owned();
    // URLTest history describes its selected outbound's most recent probe.
    let delay = value["history"]
        .as_array()
        .and_then(|items| items.last())
        .and_then(|item| item["delay"].as_u64())
        .filter(|v| *v > 0);
    if current(state.inner())?.0.as_ref() != Some(endpoint) {
        return Err("native.connectionSessionChanged".into());
    }
    Ok((!tag.is_empty()).then_some(AutoSelection { tag, delay }))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkCheck {
    local_ok: bool,
    proxy: String,
    dns_ok: bool,
    exit_ip: String,
    direct_ip: String,
    ipv6_ip: String,
    proxy_ipv6_ip: String,
    connected: bool,
    mode: String,
    tun_ipv6: bool,
}

/// One trace request. `no_proxy` skips the operating-system proxy; an explicit port is this app's proxy.
async fn trace(proxy_port: Option<u16>, url: &str) -> (bool, String) {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5));
    if let Some(port) = proxy_port {
        let Ok(proxy) = reqwest::Proxy::all(format!("http://127.0.0.1:{port}")) else {
            return (false, String::new());
        };
        builder = builder.proxy(proxy);
    }
    let Ok(client) = builder.build() else {
        return (false, String::new());
    };
    let Ok(response) = client.get(url).send().await else {
        return (false, String::new());
    };
    if !response.status().is_success() {
        return (false, String::new());
    }
    let Ok(raw) = bounded(response).await else {
        return (false, String::new());
    };
    let text = String::from_utf8_lossy(&raw);
    let ip = parse_trace(&text, false)
        .map(|info| info.ip)
        .unwrap_or_default();
    (!ip.is_empty(), ip)
}

async fn proxy_trace(connected: bool, port: u16) -> (bool, String) {
    if connected && port > 0 {
        trace(Some(port), "https://www.cloudflare.com/cdn-cgi/trace").await
    } else {
        (false, String::new())
    }
}

/// System DNS, bounded so a stuck resolver cannot hold the check open.
async fn dns_resolves() -> bool {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let ok = ("one.one.one.one", 443u16)
            .to_socket_addrs()
            .map(|mut addrs| addrs.next().is_some())
            .unwrap_or(false);
        let _ = tx.send(ok);
    });
    let start = Instant::now();
    loop {
        match rx.try_recv() {
            Ok(ok) => return ok,
            Err(mpsc::TryRecvError::Disconnected) => return false,
            Err(mpsc::TryRecvError::Empty) => {
                if start.elapsed() >= Duration::from_secs(4) {
                    return false;
                }
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        }
    }
}

/// Local network, proxy, DNS and IPv6 run together. Nothing here changes the system proxy.
#[tauri::command]
pub async fn network_check(state: State<'_, Arc<Mutex<Core>>>) -> Result<NetworkCheck, String> {
    // Capture identity as well as settings: reconnecting to another node must invalidate
    // in-flight observations, even when the new connection uses the same local port.
    let snapshot = || -> Result<_, String> {
        let core = state.inner().try_lock().map_err(|_| "native.connectionStateBusy")?;
        if !matches!(core.phase, Phase::Idle | Phase::Failed | Phase::Connected) {
            return Err("native.connectionStateBusy".into());
        }
        Ok((core.phase, core.mode.clone(), core.network.tun.ipv6,
            core.network.proxy_port, core.traffic_endpoint.clone()))
    };
    let before = snapshot()?;
    let (phase, mode, tun_ipv6, port, _) = before.clone();
    let connected = phase == Phase::Connected;
    let (direct, v6, proxied, proxy_v6, dns_ok) = tokio::join!(
        trace(None, "https://www.cloudflare.com/cdn-cgi/trace"),
        trace(None, "https://ipv6.cloudflare.com/cdn-cgi/trace"),
        proxy_trace(connected, port),
        async {
            if connected && port > 0 {
                trace(Some(port), "https://ipv6.cloudflare.com/cdn-cgi/trace").await
            } else { (false, String::new()) }
        },
        dns_resolves(),
    );
    if snapshot()? != before {
        return Err("native.networkChangedRecheckExit".into());
    }
    let proxy_ipv6_ip = if proxy_v6.0 && proxy_v6.1.contains(':') {
        proxy_v6.1
    } else { String::new() };
    let ipv6_ip = if v6.0 && v6.1.contains(':') {
        v6.1
    } else {
        String::new()
    };
    let proxy = if !connected {
        "off"
    } else if proxied.0 {
        "ok"
    } else {
        "fail"
    };
    // A failed proxy probe must never present the direct IP as a successful proxy exit.
    let exit_ip = if connected { proxied.1 } else { direct.1.clone() };
    let direct_ip = if mode != "tun" { direct.1 } else { String::new() };
    Ok(NetworkCheck {
        local_ok: direct.0,
        proxy: proxy.into(),
        dns_ok,
        exit_ip,
        direct_ip,
        ipv6_ip,
        proxy_ipv6_ip,
        connected,
        mode,
        tun_ipv6,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn trace_rejects_success_status_without_valid_ip() {
        for body in ["<html>captive portal</html>", "ip=invalid\n", "ip=8.8.8.8\nloc=US\n"] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            let server = std::thread::spawn(move || {
                use std::io::{Read, Write};
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request = [0; 2048];
                stream.read(&mut request).unwrap();
                stream.write_all(response.as_bytes()).unwrap();
            });
            let result = trace(None, &format!("http://{address}")).await;
            assert_eq!(result.0, body.starts_with("ip=8.8.8.8"));
            server.join().unwrap();
        }
    }
    #[test]
    fn geolocation_validates_success_ip_and_coordinates() {
        let info=parse_geo(br#"{"success":true,"ip":"203.0.113.10","country_code":"SG","region":"Singapore","city":"Singapore","latitude":1.3,"longitude":999,"connection":{"asn":64512,"isp":"Example ISP","org":"Example Org"},"timezone":{"id":"Asia/Singapore"}}"#,true).unwrap();
        let geo = info.geo.unwrap();
        assert_eq!(geo.asn, Some(64512));
        assert_eq!(geo.latitude, Some(1.3));
        assert_eq!(geo.longitude, None);
        assert_eq!(geo.isp, "Example ISP");
        assert!(parse_geo(br#"{"success":false,"ip":"203.0.113.10"}"#, false).is_err());
        assert!(parse_geo(br#"{"success":true,"ip":"invalid"}"#, false).is_err());
    }
    #[test]
    fn parses_only_valid_public_network_metadata() {
        let v = parse_trace("fl=secret\nip=203.0.113.12\nloc=SG\n", true).unwrap();
        assert_eq!(v.ip, "203.0.113.12");
        assert_eq!(v.country, "SG");
        assert!(v.via_proxy);
        assert!(parse_trace("ip=bad\nloc=SG", false).is_err());
        assert_eq!(
            parse_trace("ip=2001:db8::1\nloc=<html>", false)
                .unwrap()
                .country,
            ""
        );
    }
}
