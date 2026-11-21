//! Connection observations are ephemeral and never persisted with subscriptions.
use crate::{connection::Phase, tray_speed::Endpoint, Core};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{Manager, State};

const HISTORY_LIMIT: usize = 500;
const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default)]
    pub network: String,
    #[serde(default, rename = "type")]
    pub inbound: String,
    #[serde(default)]
    pub host: String,
    #[serde(default, rename = "sourceIP")]
    pub source_ip: String,
    #[serde(default)]
    pub source_port: String,
    #[serde(default, rename = "destinationIP")]
    pub destination_ip: String,
    #[serde(default)]
    pub destination_port: String,
    #[serde(default)]
    pub process_path: String,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub metadata: Metadata,
    pub upload: u64,
    pub download: u64,
    pub start: String,
    #[serde(default)]
    pub chains: Vec<String>,
    #[serde(default)]
    pub rule: String,
    #[serde(default)]
    pub rule_payload: String,
    #[serde(skip_deserializing)]
    pub upload_speed: u64,
    #[serde(skip_deserializing)]
    pub download_speed: u64,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Frame {
    upload_total: u64,
    download_total: u64,
    connections: Option<Vec<Connection>>,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    session: u64,
    running: bool,
    error: Option<String>,
    upload_total: u64,
    download_total: u64,
    active: Vec<Connection>,
    closed: VecDeque<Connection>,
}
#[derive(Default)]
pub struct Monitor {
    endpoint: Option<Endpoint>,
    snapshot: Snapshot,
    sampled: Option<Instant>,
}
impl Monitor {
    fn switch(&mut self, endpoint: Option<Endpoint>) {
        if self.endpoint == endpoint {
            return;
        }
        self.endpoint = endpoint;
        self.sampled = None;
        if self.endpoint.is_some() {
            self.snapshot = Snapshot {
                session: self.snapshot.session.wrapping_add(1),
                running: true,
                ..Snapshot::default()
            };
        } else {
            let previous = std::mem::take(&mut self.snapshot.active);
            for row in previous {
                self.archive(row);
            }
            self.snapshot.running = false;
            self.snapshot.error = None;
        }
    }
    fn archive(&mut self, mut row: Connection) {
        row.upload_speed = 0;
        row.download_speed = 0;
        self.snapshot.closed.push_front(row);
        self.snapshot.closed.truncate(HISTORY_LIMIT);
    }
    fn accept(&mut self, frame: Frame, now: Instant) {
        let seconds = self.sampled.map(|at| now.duration_since(at).as_secs_f64());
        let mut old: HashMap<_, _> = std::mem::take(&mut self.snapshot.active)
            .into_iter()
            .map(|row| (row.id.clone(), row))
            .collect();
        let mut active = frame.connections.unwrap_or_default();
        for row in &mut active {
            if let (Some(prev), Some(dt)) = (old.remove(&row.id), seconds) {
                if dt > 0.0 {
                    row.upload_speed = (row.upload.saturating_sub(prev.upload) as f64 / dt) as u64;
                    row.download_speed =
                        (row.download.saturating_sub(prev.download) as f64 / dt) as u64;
                }
            }
        }
        for row in old.into_values() {
            self.archive(row);
        }
        active.sort_by(|a, b| b.start.cmp(&a.start).then_with(|| a.id.cmp(&b.id)));
        self.snapshot.active = active;
        self.snapshot.upload_total = frame.upload_total;
        self.snapshot.download_total = frame.download_total;
        self.snapshot.error = None;
        self.sampled = Some(now);
    }
}
fn endpoint(core: &Arc<Mutex<Core>>) -> Result<Option<Endpoint>, String> {
    let state = core.try_lock().map_err(|_| "native.connectionStateBusy")?;
    Ok((state.phase == Phase::Connected && state.child.is_some())
        .then(|| state.traffic_endpoint.clone())
        .flatten())
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|_| "native.connectionsClientFailed".into())
}
async fn sample(client: &reqwest::Client, endpoint: &Endpoint) -> Result<Frame, String> {
    let mut response = endpoint
        .request(client, reqwest::Method::GET, "/connections")
        .send()
        .await
        .map_err(|_| "native.connectionsUnavailable")?
        .error_for_status()
        .map_err(|_| "native.connectionsReadRejected")?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "native.connectionsReadInterrupted")?
    {
        if bytes.len() + chunk.len() > RESPONSE_LIMIT {
            return Err("native.connectionsTooLarge".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "native.connectionsDataInvalid".into())
}
pub fn start(app: tauri::AppHandle, core: Arc<Mutex<Core>>) {
    let monitor = Arc::new(Mutex::new(Monitor::default()));
    app.manage(monitor.clone());
    tauri::async_runtime::spawn(async move {
        let Ok(client) = client() else {
            return;
        };
        loop {
            if let Ok(current) = endpoint(&core) {
                monitor.lock().unwrap().switch(current.clone());
                if let Some(ref current) = current {
                    let result = sample(&client, current).await;
                    // A response from an old core must never overwrite the new session.
                    if endpoint(&core).ok().flatten().as_ref() == Some(current) {
                        let mut state = monitor.lock().unwrap();
                        match result {
                            Ok(frame) => state.accept(frame, Instant::now()),
                            Err(error) => {
                                state.snapshot.error = Some(error);
                                for row in &mut state.snapshot.active {
                                    row.upload_speed = 0;
                                    row.download_speed = 0;
                                }
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}
#[tauri::command]
pub fn list_connections(state: State<Arc<Mutex<Monitor>>>) -> Result<Snapshot, String> {
    state
        .lock()
        .map(|s| s.snapshot.clone())
        .map_err(|_| "native.connectionsListFailed".into())
}
#[tauri::command]
pub async fn application_icon(state: State<'_, Arc<Mutex<Monitor>>>, app: String) -> Result<Option<Vec<u8>>, String> {
    let path = {
        let monitor = state.lock().map_err(|_| "native.connectionsListFailed")?;
        monitor.snapshot.active.iter().chain(monitor.snapshot.closed.iter())
            .map(|row| &row.metadata.process_path)
            .find(|path| **path == app).cloned()
    };
    let Some(path) = path else { return Ok(None); };
    tauri::async_runtime::spawn_blocking(move || {
        let raw = path.rsplit_once(" (").filter(|(_, user)| user.ends_with(')') && !user.contains(['/', '\\'])).map_or(path.as_str(), |(exe, _)| exe);
        crate::app_icons::load(raw)
    }).await.map_err(|e| e.to_string())
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
#[tauri::command]
pub async fn close_connections(
    core: State<'_, Arc<Mutex<Core>>>,
    state: State<'_, Arc<Mutex<Monitor>>>,
    session: u64,
    id: Option<String>,
) -> Result<(), String> {
    if id.as_ref().is_some_and(|id| !valid_id(id)) {
        return Err("native.connectionIdInvalid".into());
    }
    let endpoint = endpoint(core.inner())?.ok_or("native.notConnected")?;
    {
        let monitor = state
            .lock()
            .map_err(|_| "native.connectionStateUnreadable")?;
        if monitor.snapshot.session != session || monitor.endpoint.as_ref() != Some(&endpoint) {
            return Err("native.connectionSessionChangedRetry".into());
        }
    }
    let path = id
        .map(|id| format!("/connections/{id}"))
        .unwrap_or_else(|| "/connections".into());
    endpoint
        .request(&client()?, reqwest::Method::DELETE, &path)
        .send()
        .await
        .map_err(|_| "native.closeConnectionFailed")?
        .error_for_status()
        .map_err(|_| "native.closeConnectionRejected")?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn frame(id: &str, down: u64) -> Frame {
        Frame {
            connections: Some(vec![Connection {
                id: id.into(),
                download: down,
                ..Connection::default()
            }]),
            ..Frame::default()
        }
    }
    #[test]
    fn rates_history_and_session_reset() {
        let mut m = Monitor::default();
        m.switch(Some(Endpoint::new().unwrap()));
        let now = Instant::now();
        m.accept(frame("a", 100), now);
        assert_eq!(m.snapshot.active[0].download_speed, 0);
        m.accept(frame("a", 300), now + Duration::from_secs(2));
        assert_eq!(m.snapshot.active[0].download_speed, 100);
        m.accept(frame("a", 10), now + Duration::from_secs(3));
        assert_eq!(m.snapshot.active[0].download_speed, 0);
        m.accept(Frame::default(), now + Duration::from_secs(4));
        assert_eq!(m.snapshot.closed.len(), 1);
        for _ in 0..600 {
            m.archive(Connection::default());
        }
        assert_eq!(m.snapshot.closed.len(), 500);
        m.switch(None);
        assert!(!m.snapshot.running);
        m.switch(Some(Endpoint::new().unwrap()));
        assert_eq!(m.snapshot.session, 2);
        assert!(m.snapshot.closed.is_empty());
    }
    #[test]
    fn reads_native_core_frame_and_keeps_zero_connections_valid() {
        let frame: Frame = serde_json::from_str(r#"{"uploadTotal":1024,"downloadTotal":2048,"connections":[{"id":"abcdef01-2345-6789-abcd-0123456789ab","metadata":{"network":"tcp","type":"mixed","sourceIP":"127.0.0.1","sourcePort":"53412","destinationIP":"127.0.0.1","destinationPort":"443","host":"example.test","dnsMode":"normal","processPath":""},"upload":1024,"download":2048,"start":"2026-11-14T04:00:00Z","chains":["direct"],"rule":"","rulePayload":""}]}"#).unwrap();
        let mut m = Monitor::default();
        m.accept(frame, Instant::now());
        assert_eq!(m.snapshot.active[0].metadata.destination_port, "443");
        assert_eq!(m.snapshot.active[0].metadata.inbound, "mixed");
        assert_eq!(m.snapshot.download_total, 2048);
        let frame =
            serde_json::from_str(r#"{"uploadTotal":1024,"downloadTotal":2048,"connections":null}"#)
                .unwrap();
        m.accept(frame, Instant::now());
        assert!(m.snapshot.active.is_empty());
        assert_eq!(m.snapshot.closed.len(), 1);
    }
    #[test]
    fn reject_injected_paths() {
        assert!(valid_id("abcdef01-2345-6789-abcd-0123456789ab"));
        for id in [
            "",
            "../connections",
            "aaaaaaaa/../../connections",
            "abcdef01-2345-6789-abcd-0123456789az",
        ] {
            assert!(!valid_id(id));
        }
    }
}
