//! Bounded, explicitly approved text transfers over the existing LAN listener.
use crate::{
    protocol::{DeviceInfo, FileDto},
    storage::{HistoryRow, Storage},
    transport::{AcceptDecision, AcceptPolicy},
    LanError, LanEvent, Result,
};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;
use uuid::Uuid;

pub const TEXT_LIMIT: usize = 64 * 1024;
pub const TEXT_PATH: &str = "/api/clashnova/v1/messages";
pub const TEXT_TYPE: &str = "application/vnd.clashnova.text";
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRequest {
    pub id: String,
    pub info: DeviceInfo,
    pub text: String,
}

pub fn validate_text(text: &str) -> Result<()> {
    if text.trim().is_empty() || text.len() > TEXT_LIMIT || text.contains('\0') {
        return Err(LanError::InvalidText);
    }
    Ok(())
}

pub struct TextInbox {
    storage: Arc<Storage>,
    policy: Arc<dyn AcceptPolicy>,
    events: broadcast::Sender<LanEvent>,
    pending: Arc<Mutex<HashSet<String>>>,
}
struct PendingGuard {
    pending: Arc<Mutex<HashSet<String>>>,
    id: String,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.pending
            .lock()
            .expect("text pending lock")
            .remove(&self.id);
    }
}
impl TextInbox {
    pub fn new(
        storage: Arc<Storage>,
        policy: Arc<dyn AcceptPolicy>,
        events: broadcast::Sender<LanEvent>,
    ) -> Self {
        Self {
            storage,
            policy,
            events,
            pending: Arc::new(Mutex::new(HashSet::new())),
        }
    }
    pub async fn receive(&self, request: TextRequest) -> StatusCode {
        if validate_text(&request.text).is_err()
            || Uuid::parse_str(&request.id).is_err()
            || request.info.alias.len() > 256
            || request.info.fingerprint.is_empty()
            || request.info.fingerprint.len() > 256
        {
            return StatusCode::BAD_REQUEST;
        }
        // A retried accepted request is acknowledged without another prompt or duplicate row.
        match self
            .storage
            .text_matches(&request.id, &request.info.fingerprint, &request.text)
        {
            Ok(Some(true)) => return StatusCode::OK,
            Ok(Some(false)) => return StatusCode::CONFLICT,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
            Ok(None) => {}
        }
        let _pending = {
            let mut pending = self.pending.lock().expect("text pending lock");
            if pending.contains(&request.id) || pending.len() >= 4 {
                return StatusCode::TOO_MANY_REQUESTS;
            }
            pending.insert(request.id.clone());
            PendingGuard {
                pending: self.pending.clone(),
                id: request.id.clone(),
            }
        };
        let file = FileDto {
            id: request.id.clone(),
            file_name: "message.txt".into(),
            size: request.text.len() as u64,
            file_type: TEXT_TYPE.into(),
            sha256: None,
            preview: Some(request.text.clone()),
            metadata: None,
        };
        let files = HashMap::from([(request.id.clone(), file)]);
        let accepted = tokio::time::timeout(
            Duration::from_secs(120),
            self.policy.decide(&request.id, &request.info, &files),
        )
        .await;
        if !matches!(accepted, Ok(AcceptDecision::Accept { .. })) {
            return StatusCode::FORBIDDEN;
        }
        let mut row = history_row(&request.id, "receive", &request.info.alias, &request.text);
        row.peer_fingerprint = request.info.fingerprint.clone();
        match self.storage.insert_text(&row, &request.info.fingerprint) {
            Ok(true) => {
                let _ = self.events.send(LanEvent::TextReceived(row));
                StatusCode::OK
            }
            Ok(false) => StatusCode::OK,
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

pub fn history_row(id: &str, direction: &str, peer: &str, text: &str) -> HistoryRow {
    HistoryRow {
        id: id.into(),
        direction: direction.into(),
        peer_alias: peer.into(),
        peer_fingerprint: String::new(),
        file_name: String::new(),
        size: text.len() as u64,
        state: "COMPLETED".into(),
        finished_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .ok(),
        save_path: None,
        text: Some(text.into()),
    }
}

pub async fn send_text(
    base: &str,
    request: &TextRequest,
    expected_fingerprint: Option<String>,
    identity: Option<&crate::tls::Identity>,
) -> Result<()> {
    validate_text(&request.text)?;
    let client = crate::tls::client(base.starts_with("https"), expected_fingerprint, identity)?;
    let response = client
        .post(format!("{base}{TEXT_PATH}"))
        .json(request)
        .send()
        .await?;
    match response.status() {
        StatusCode::OK => Ok(()),
        StatusCode::FORBIDDEN => Err(LanError::Rejected),
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED => Err(LanError::TextUnsupported),
        _ => {
            response.error_for_status()?;
            Err(LanError::Protocol(
                "Unexpected message acknowledgement".into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        protocol::{DeviceType, TransportProtocol},
        transport::{AcceptAll, Receiver, RejectAll},
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn request(text: &str) -> TextRequest {
        TextRequest {
            id: Uuid::new_v4().to_string(),
            text: text.into(),
            info: DeviceInfo {
                alias: "sender".into(),
                fingerprint: "sender-fp".into(),
                version: "2.0".into(),
                device_model: None,
                device_type: Some(DeviceType::Desktop),
                port: None,
                protocol: Some(TransportProtocol::Http),
                download: None,
            },
        }
    }
    struct CountingPolicy(Arc<AtomicUsize>);
    #[async_trait]
    impl AcceptPolicy for CountingPolicy {
        async fn decide(
            &self,
            _: &str,
            _: &DeviceInfo,
            _: &HashMap<String, FileDto>,
        ) -> AcceptDecision {
            self.0.fetch_add(1, Ordering::SeqCst);
            AcceptDecision::Accept {
                save_dir: std::path::PathBuf::new(),
            }
        }
    }
    fn test_identity() -> crate::tls::Identity {
        let dir = tempfile::tempdir().unwrap();
        crate::tls::Identity::load_or_create(dir.path()).unwrap()
    }
    /// mTLS 测试客户端：pin 服务器指纹并出示给定客户端身份证书。
    fn tls_client(server_fp: &str, id: &crate::tls::Identity) -> reqwest::Client {
        crate::tls::client(true, Some(server_fp.to_string()), Some(id)).unwrap()
    }
    /// 以真实 mTLS 起文本接收端。返回 (接收端, 存储, 服务器证书指纹, 客户端身份)。
    /// 强制 mTLS 后接收端只认已验证的客户端证书指纹，明文已无法携带身份。
    async fn receiver(
        policy: Arc<dyn AcceptPolicy>,
    ) -> (Receiver, Arc<Storage>, String, crate::tls::Identity) {
        let storage = Arc::new(Storage::in_memory().unwrap());
        let (events, _) = broadcast::channel(16);
        let inbox = Arc::new(TextInbox::new(storage.clone(), policy.clone(), events));
        let server_id = test_identity();
        let tls = crate::tls::server_config(&server_id).await.unwrap();
        let rx = Receiver::start_with_messages(
            "127.0.0.1:0".parse().unwrap(),
            policy,
            request("placeholder").info,
            None,
            None,
            Some(inbox),
            Some(tls),
            crate::transport::cancel_registry(),
        )
        .await
        .unwrap();
        (rx, storage, server_id.fingerprint.clone(), test_identity())
    }
    #[tokio::test]
    async fn approved_text_round_trip_is_literal_and_idempotent() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (rx, storage, fp, id) = receiver(Arc::new(CountingPolicy(calls.clone()))).await;
        let base = format!("https://{}", rx.local_addr());
        let req = request("hello\n<script>literal</script> 中文 😀");
        send_text(&base, &req, Some(fp.clone()), Some(&id)).await.unwrap();
        send_text(&base, &req, Some(fp.clone()), Some(&id)).await.unwrap();
        let rows = storage.list_history(200).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text.as_deref(), Some(req.text.as_str()));
        assert_eq!(rows[0].size, req.text.len() as u64);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let mut changed = req.clone();
        changed.text = "different".into();
        let status = tls_client(&fp, &id)
            .post(format!("{base}{TEXT_PATH}"))
            .json(&changed)
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::CONFLICT);
        rx.shutdown().await;
    }
    #[tokio::test]
    async fn invalid_and_oversized_messages_never_prompt_or_persist() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (rx, storage, fp, id) = receiver(Arc::new(CountingPolicy(calls.clone()))).await;
        let base = format!("https://{}", rx.local_addr());
        for text in [
            " ".into(),
            "x".repeat(TEXT_LIMIT + 1),
            "hello\0world".into(),
        ] {
            let response = tls_client(&fp, &id)
                .post(format!("{base}{TEXT_PATH}"))
                .json(&request(&text))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
        let response = tls_client(&fp, &id)
            .post(format!("{base}{TEXT_PATH}"))
            .header("content-type", "application/json")
            .body("x".repeat(600 * 1024))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert!(storage.list_history(200).unwrap().is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        // JSON escaping may enlarge the wire body; the 64 KiB limit applies to actual UTF-8 text.
        send_text(
            &base,
            &request(&format!("x{}", "\n".repeat(TEXT_LIMIT - 1))),
            Some(fp.clone()),
            Some(&id),
        )
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        rx.shutdown().await;
    }
    struct WaitingPolicy {
        calls: Arc<AtomicUsize>,
        permits: Arc<tokio::sync::Semaphore>,
    }
    #[async_trait]
    impl AcceptPolicy for WaitingPolicy {
        async fn decide(
            &self,
            _: &str,
            _: &DeviceInfo,
            _: &HashMap<String, FileDto>,
        ) -> AcceptDecision {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.permits.acquire().await.unwrap().forget();
            AcceptDecision::Accept {
                save_dir: std::path::PathBuf::new(),
            }
        }
    }
    #[tokio::test]
    async fn pending_text_requests_are_bounded_and_duplicate_requests_cannot_replace_approval() {
        let calls = Arc::new(AtomicUsize::new(0));
        let permits = Arc::new(tokio::sync::Semaphore::new(0));
        let (rx, storage, fp, id) = receiver(Arc::new(WaitingPolicy {
            calls: calls.clone(),
            permits: permits.clone(),
        }))
        .await;
        let base = format!("https://{}", rx.local_addr());
        let first = request("first");
        let mut jobs = Vec::new();
        for req in [
            first.clone(),
            request("second"),
            request("third"),
            request("fourth"),
        ] {
            let base = base.clone();
            let fp = fp.clone();
            let id = id.clone();
            jobs.push(tokio::spawn(
                async move { send_text(&base, &req, Some(fp), Some(&id)).await },
            ));
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while calls.load(Ordering::SeqCst) != 4 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        for req in [first, request("fifth")] {
            let status = tls_client(&fp, &id)
                .post(format!("{base}{TEXT_PATH}"))
                .json(&req)
                .send()
                .await
                .unwrap()
                .status();
            assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        }
        permits.add_permits(4);
        for job in jobs {
            job.await.unwrap().unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(storage.list_history(200).unwrap().len(), 4);
        rx.shutdown().await;
    }

    #[tokio::test]
    async fn rejected_text_is_not_recorded_and_legacy_peer_reports_unsupported() {
        let (rx, storage, fp, id) = receiver(Arc::new(RejectAll)).await;
        assert!(matches!(
            send_text(&format!("https://{}", rx.local_addr()), &request("hello"), Some(fp), Some(&id)).await,
            Err(LanError::Rejected)
        ));
        assert!(storage.list_history(200).unwrap().is_empty());
        rx.shutdown().await;
        let dir = tempfile::tempdir().unwrap();
        let rx = Receiver::start(
            "127.0.0.1:0".parse().unwrap(),
            Arc::new(AcceptAll {
                save_dir: dir.path().into(),
            }),
            request("x").info,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(
            send_text(&format!("http://{}", rx.local_addr()), &request("hello"), None, None).await,
            Err(LanError::TextUnsupported)
        ));
        rx.shutdown().await;
    }
    #[test]
    fn adding_text_storage_preserves_file_history_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite3");
        {
            let storage = Storage::open(&path).unwrap();
            let mut file = history_row("file", "receive", "peer", "");
            file.text = None;
            file.file_name = "old.txt".into();
            storage.insert_history(&file).unwrap();
            storage
                .insert_text(
                    &history_row("text", "send", "peer", "persistent text"),
                    "peer-fp",
                )
                .unwrap();
        }
        let storage = Storage::open(&path).unwrap();
        let rows = storage.list_history(200).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.file_name == "old.txt"));
        assert!(rows
            .iter()
            .any(|r| r.text.as_deref() == Some("persistent text")));
        storage.clear_history().unwrap();
        assert!(storage.list_history(200).unwrap().is_empty());
    }
}
