//! Native traffic sampling keeps working when the webview is hidden.
use crate::{connection::Phase, tray, Core};
use serde::Deserialize;
use std::{
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

#[derive(Clone, PartialEq)]
pub struct Endpoint {
    port: u16,
    secret: String,
}

impl Endpoint {
    pub(crate) fn blocking_request(
        &self,
        client: &reqwest::blocking::Client,
        method: reqwest::Method,
        path: &str,
    ) -> reqwest::blocking::RequestBuilder {
        client
            .request(method, format!("http://127.0.0.1:{}{path}", self.port))
            .bearer_auth(&self.secret)
    }
    pub(crate) fn request(
        &self,
        client: &reqwest::Client,
        method: reqwest::Method,
        path: &str,
    ) -> reqwest::RequestBuilder {
        client
            .request(method, format!("http://127.0.0.1:{}{path}", self.port))
            .bearer_auth(&self.secret)
    }
    pub fn new() -> Result<Self, String> {
        let socket =
            TcpListener::bind("127.0.0.1:0").map_err(|_| "native.trafficEndpointUnavailable")?;
        let port = socket
            .local_addr()
            .map_err(|_| "native.trafficPortUnreadable")?
            .port();
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| "native.trafficSecretFailed")?;
        Ok(Self {
            port,
            secret: bytes.iter().map(|b| format!("{b:02x}")).collect(),
        })
    }

    pub fn configure(&self, config: &mut serde_json::Value) {
        // Connection monitoring needs process identity before any app policy exists.
        // Process search is supported by sing-box on macOS, Windows and Linux.
        if !config["route"].is_object() {
            config["route"] = serde_json::json!({});
        }
        config["route"]["find_process"] = true.into();
        // Never trust an imported controller address or secret. Credentials stay
        // in Rust and the private runtime config; they are not sent to the UI.
        config["experimental"] = serde_json::json!({"clash_api": {
            "external_controller": format!("127.0.0.1:{}", self.port), "secret": self.secret
        }});
    }
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Debug)]
pub struct Speed {
    pub up: u64,
    pub down: u64,
}
impl Speed {
    pub fn label(self) -> String {
        format!("↑ {}  ↓ {}", rate(self.up), rate(self.down))
    }
}
fn rate(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = "B/s";
    for next in ["KB/s", "MB/s", "GB/s", "TB/s"] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    if unit == "B/s" {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.1} {unit}")
    }
}

#[derive(Default)]
struct Frames {
    pending: Vec<u8>,
}
impl Frames {
    fn push(&mut self, bytes: &[u8]) -> Result<Option<Speed>, ()> {
        if self.pending.len() + bytes.len() > 8192 {
            return Err(());
        }
        self.pending.extend_from_slice(bytes);
        let mut latest = None;
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            latest = Some(serde_json::from_slice::<Speed>(&self.pending[..end]).map_err(|_| ())?);
            self.pending.drain(..=end);
        }
        Ok(latest)
    }
}

type Sample = Arc<Mutex<Option<(Speed, Instant)>>>;
async fn consume(endpoint: Endpoint, sample: Sample) {
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .read_timeout(Duration::from_secs(3))
        .build()
    else {
        return;
    };
    loop {
        let result = endpoint
            .request(&client, reqwest::Method::GET, "/traffic")
            .send()
            .await;
        if let Ok(mut response) = result.and_then(reqwest::Response::error_for_status) {
            let mut frames = Frames::default();
            while let Ok(Some(bytes)) = response.chunk().await {
                match frames.push(&bytes) {
                    Ok(Some(speed)) => *sample.lock().unwrap() = Some((speed, Instant::now())),
                    Ok(None) => {}
                    Err(()) => break,
                }
            }
        }
        *sample.lock().unwrap() = None;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub fn start(app: AppHandle, core: Arc<Mutex<Core>>) {
    tauri::async_runtime::spawn(async move {
        let sample: Sample = Arc::default();
        let mut active = None;
        let mut task: Option<tauri::async_runtime::JoinHandle<()>> = None;
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let controls = app.state::<tray::Controls>();
            let enabled = controls.speed_enabled();
            let endpoint = core.try_lock().ok().and_then(|state| {
                (enabled && state.phase == Phase::Connected && state.child.is_some())
                    .then(|| state.traffic_endpoint.clone())
                    .flatten()
            });
            if active != endpoint {
                if let Some(previous) = task.take() {
                    previous.abort();
                    let _ = previous.await;
                }
                *sample.lock().unwrap() = None;
                active = endpoint.clone();
                if let Some(endpoint) = endpoint {
                    task = Some(tauri::async_runtime::spawn(consume(
                        endpoint,
                        sample.clone(),
                    )));
                }
            }
            let speed = if active.is_none() {
                Some(Speed::default())
            } else {
                sample
                    .lock()
                    .unwrap()
                    .filter(|(_, received)| received.elapsed() < Duration::from_secs(3))
                    .map(|(speed, _)| speed)
            };
            // UI calls must happen without the core mutex: connecting and exiting
            // may hold it while doing IO. Updates are deduplicated in the tray.
            let _ = controls.render_speed(&app, enabled, speed);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_rates_without_confusing_bytes_and_bits() {
        assert_eq!(Speed::default().label(), "↑ 0 B/s  ↓ 0 B/s");
        assert_eq!(rate(1536), "1.5 KB/s");
        assert_eq!(rate(1048576), "1.0 MB/s");
    }
    #[test]
    fn handles_split_and_coalesced_frames_and_rejects_invalid_input() {
        let mut frames = Frames::default();
        assert_eq!(frames.push(br#"{"up":123,"down":"#), Ok(None));
        assert_eq!(
            frames.push(b"456}\n{\"up\":7,\"down\":8}\n"),
            Ok(Some(Speed { up: 7, down: 8 }))
        );
        assert!(frames.push(b"{\"up\":-1,\"down\":0}\n").is_err());
        assert!(Frames::default().push(&vec![b'x'; 8193]).is_err());
        assert!(Frames::default().push(b"{}\n").is_err());
    }
    #[test]
    fn controller_is_private_and_rotates_credentials() {
        let a = Endpoint::new().unwrap();
        let b = Endpoint::new().unwrap();
        assert_ne!(a.secret, b.secret);
        assert_eq!(a.secret.len(), 64);
        let mut config = serde_json::json!({"experimental":{"clash_api":{"external_controller":"0.0.0.0:9090","secret":"imported"}}});
        a.configure(&mut config);
        assert_eq!(config["route"]["find_process"], true);
        config["route"]["find_process"] = false.into();
        config["route"]["final"] = "select".into();
        a.configure(&mut config);
        assert_eq!(config["route"]["find_process"], true);
        assert_eq!(config["route"]["final"], "select");
        assert!(config["experimental"]["clash_api"]["external_controller"]
            .as_str()
            .unwrap()
            .starts_with("127.0.0.1:"));
        assert_eq!(config["experimental"]["clash_api"]["secret"], a.secret);
    }
}
