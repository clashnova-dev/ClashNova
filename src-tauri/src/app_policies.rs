//! Durable application policies and authenticated, in-place core updates.
use crate::{connection::Phase, settings_store::SettingsStore, tray_speed::Endpoint, Core};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Read,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::State;

type Policies = BTreeMap<String, String>;
const KINDS: [&str; 3] = ["block", "proxy", "direct"];
fn valid_name(app: &str) -> bool {
    !app.is_empty()
        && app.chars().count() <= 256
        && !app.chars().any(|c| c.is_control() || c == '/' || c == '\\')
}
fn validate(policies: &Policies) -> Result<(), String> {
    if policies.len() > 1024
        || policies
            .iter()
            .any(|(app, policy)| !valid_name(app) || !KINDS.contains(&policy.as_str()))
    {
        return Err("native.appPolicyInvalid".into());
    }
    if serde_json::to_vec(policies)
        .map_err(|e| e.to_string())?
        .len()
        > 256 * 1024
    {
        return Err("native.appPolicyInvalid".into());
    }
    Ok(())
}
fn app_regex(app: &str) -> String {
    let mut name = String::new();
    for c in app.chars() {
        if ".*+?^${}()|[]\\".contains(c) {
            name.push('\\');
        }
        name.push(c);
    }
    if app.to_ascii_lowercase().ends_with(".exe") {
        format!(r"(?i)(?:^|[\\/]){name}$")
    } else {
        format!(r"(?:^|/){name}(?:\.app/|$)")
    }
}
fn update_config(raw: &[u8], policies: &Policies) -> Result<Vec<u8>, String> {
    let mut config: Value = serde_json::from_slice(raw).map_err(|e| e.to_string())?;
    let route = config
        .get_mut("route")
        .and_then(Value::as_object_mut)
        .ok_or("native.appPolicyInvalid")?;
    let previous = route
        .get("rules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut rules = Vec::new();
    for kind in KINDS {
        let mut expressions: Vec<_> = policies
            .iter()
            .filter(|(_, policy)| policy.as_str() == kind)
            .map(|(app, _)| app_regex(app))
            .collect();
        expressions.push("^$".into());
        let mut rule =
            json!({"inbound":[format!("clashnova-app-{kind}")], "process_path_regex":expressions});
        if kind == "block" {
            rule["action"] = "reject".into();
        } else {
            rule["outbound"] = if kind == "proxy" { "select" } else { "direct" }.into();
        }
        rules.push(rule);
    }
    rules.extend(previous.into_iter().filter(|rule| {
        !KINDS
            .iter()
            .any(|kind| rule["inbound"] == json!([format!("clashnova-app-{kind}")]))
    }));
    route.insert("rules".into(), rules.into());
    let bytes = serde_json::to_vec(&config).map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("native.appPolicyInvalid".into());
    }
    Ok(bytes)
}

#[derive(Deserialize)]
struct RuntimeResult {
    applied: bool,
    #[serde(default)]
    closed: u64,
}
#[derive(Serialize)]
pub struct PolicyResult {
    policies: Policies,
    applied: bool,
    closed: u64,
}
fn apply_runtime(endpoint: &Endpoint, policies: &Policies) -> Result<RuntimeResult, String> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|_| "native.appPolicyUnavailable")?;
    let response = endpoint
        .blocking_request(&client, reqwest::Method::PUT, "/app-policies")
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(policies).map_err(|e| e.to_string())?)
        .send()
        .map_err(|_| "native.appPolicyUnavailable")?;
    if !response.status().is_success() {
        return Err("native.appPolicyUnavailable".into());
    }
    let mut bytes = Vec::new();
    response
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "native.appPolicyUnavailable")?;
    if bytes.len() > 4096 {
        return Err("native.appPolicyUnavailable".into());
    }
    let result: RuntimeResult =
        serde_json::from_slice(&bytes).map_err(|_| "native.appPolicyUnavailable")?;
    if !result.applied {
        return Err("native.appPolicyUnavailable".into());
    }
    Ok(result)
}

fn set_policy(
    core: &mut Core,
    store: &SettingsStore,
    app: String,
    policy: String,
) -> Result<PolicyResult, String> {
    if !matches!(core.phase, Phase::Idle | Phase::Failed | Phase::Connected) {
        return Err("native.connectionBusy".into());
    }
    if !valid_name(&app) || !(policy == "inherit" || KINDS.contains(&policy.as_str())) {
        return Err("native.appPolicyInvalid".into());
    }
    let old_value = store.get("appPolicies")?.unwrap_or(Value::Null);
    let previous: Policies = if old_value.is_null() {
        BTreeMap::new()
    } else {
        serde_json::from_value(old_value.clone()).map_err(|_| "native.appPolicyInvalid")?
    };
    validate(&previous)?;
    let mut next = previous.clone();
    if policy == "inherit" {
        next.remove(&app);
    } else {
        next.insert(app, policy);
    }
    validate(&next)?;
    let endpoint = if core.phase == Phase::Connected {
        Some(core.traffic_endpoint.clone().ok_or("native.notConnected")?)
    } else {
        None
    };
    // Update the startup snapshot too: a core crash must not restore an old block/allow rule.
    let saved = if let Some(path) = core.config_path.as_ref() {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("native.appPolicyInvalid".into());
        }
        let updated = update_config(&bytes, &next)?;
        Some((path.clone(), bytes, updated))
    } else {
        None
    };
    if let Some((path, _, updated)) = &saved {
        crate::storage::write_private(path, updated)?;
    }
    if let Err(error) = store.set("appPolicies", json!(next)) {
        if let Some((path, old, _)) = &saved {
            if crate::storage::write_private(path, old).is_err() {
                let _ = crate::stop_core(core);
                return Err("native.appPolicyRestoreFailed".into());
            }
        }
        return Err(error);
    }
    if let Some(endpoint) = endpoint {
        match apply_runtime(&endpoint, &next) {
            Ok(result) => {
                return Ok(PolicyResult {
                    policies: next,
                    applied: true,
                    closed: result.closed,
                })
            }
            Err(error) => {
                let file_ok = saved
                    .as_ref()
                    .is_none_or(|(path, old, _)| crate::storage::write_private(path, old).is_ok());
                let store_ok = store.set("appPolicies", old_value).is_ok();
                // A response may have been lost after applying. Restore the old runtime
                // snapshot as well; if its state cannot be established, stop safely.
                let runtime_ok = apply_runtime(&endpoint, &previous).is_ok();
                if !file_ok || !store_ok || !runtime_ok {
                    let _ = crate::stop_core(core);
                }
                if !file_ok || !store_ok {
                    return Err("native.appPolicyRestoreFailed".into());
                }
                return Err(error);
            }
        }
    }
    Ok(PolicyResult {
        policies: next,
        applied: false,
        closed: 0,
    })
}

#[tauri::command]
pub async fn set_application_policy(
    core: State<'_, Arc<Mutex<Core>>>,
    store: State<'_, Arc<SettingsStore>>,
    app: String,
    policy: String,
) -> Result<PolicyResult, String> {
    let core = core.inner().clone();
    let store = store.inner().clone();
    crate::blocking(move || {
        let mut core = core.lock().map_err(|_| "native.connectionBusy")?;
        set_policy(&mut core, &store, app, policy)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Core, SettingsStore, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let original = br#"{"route":{"final":"select","rules":[{"ip_is_private":true,"outbound":"direct"}]},"outbounds":[{"type":"direct","tag":"direct"}]}"#.to_vec();
        std::fs::write(&path, &original).unwrap();
        let mut core = Core::new();
        core.config_path = Some(path);
        let store = SettingsStore::open(dir.path().join("policy-prefs.json"));
        (dir, core, store, original)
    }
    #[test]
    fn offline_policy_is_durable_and_crash_config_tracks_block_and_unblock() {
        let (dir, mut core, store, _) = setup();
        let result = set_policy(&mut core, &store, "Cursor".into(), "block".into()).unwrap();
        assert!(!result.applied);
        assert_eq!(result.policies["Cursor"], "block");
        let reopened = SettingsStore::open(dir.path().join("policy-prefs.json"));
        assert_eq!(
            reopened.get("appPolicies").unwrap().unwrap()["Cursor"],
            "block"
        );
        let config: Value =
            serde_json::from_slice(&std::fs::read(core.config_path.as_ref().unwrap()).unwrap())
                .unwrap();
        assert_eq!(
            config["route"]["rules"][0]["inbound"],
            json!(["clashnova-app-block"])
        );
        assert_eq!(
            config["route"]["rules"][0]["process_path_regex"][0],
            app_regex("Cursor")
        );
        let result = set_policy(&mut core, &store, "Cursor".into(), "inherit".into()).unwrap();
        assert!(result.policies.is_empty());
        let config: Value =
            serde_json::from_slice(&std::fs::read(core.config_path.as_ref().unwrap()).unwrap())
                .unwrap();
        assert_eq!(config["route"]["rules"].as_array().unwrap().len(), 4);
        assert_eq!(
            config["route"]["rules"][0]["process_path_regex"],
            json!(["^$"])
        );
        assert_eq!(config["route"]["rules"][3]["ip_is_private"], true);
    }
    #[test]
    fn failed_storage_write_does_not_change_the_startup_config() {
        let (dir, mut core, _, original) = setup();
        let bad = dir.path().join("bad.json");
        let store = SettingsStore::open(bad.clone());
        std::fs::create_dir(bad).unwrap();
        assert!(set_policy(&mut core, &store, "Cursor".into(), "block".into()).is_err());
        assert_eq!(
            std::fs::read(core.config_path.as_ref().unwrap()).unwrap(),
            original
        );
    }
    #[test]
    fn invalid_names_policies_and_busy_connections_leave_preferences_unchanged() {
        let (_dir, mut core, store, _) = setup();
        for (app, policy) in [
            ("", "block"),
            ("../app", "block"),
            ("app\\child", "block"),
            ("Cursor", "script"),
        ] {
            assert!(set_policy(&mut core, &store, app.into(), policy.into()).is_err());
        }
        core.phase = Phase::Connecting;
        assert!(set_policy(&mut core, &store, "Cursor".into(), "block".into()).is_err());
        assert!(store.get("appPolicies").unwrap().is_none());
    }
    #[test]
    fn rejected_runtime_update_rolls_back_preferences_and_startup_config() {
        use std::{io::Write, net::TcpListener};
        let (_dir, mut core, store, original) = setup();
        let endpoint = Endpoint::new().unwrap();
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let request = endpoint
            .blocking_request(&client, reqwest::Method::PUT, "/app-policies")
            .build()
            .unwrap();
        let server = TcpListener::bind(("127.0.0.1", request.url().port().unwrap())).unwrap();
        server.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            for status in ["500 Internal Server Error", "200 OK"] {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match server.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        other => panic!("policy fixture request missing: {other:?}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = [0; 4096];
                let _ = socket.read(&mut bytes);
                let body = r#"{"applied":true,"closed":0}"#;
                write!(
                    socket,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        core.phase = Phase::Connected;
        core.traffic_endpoint = Some(endpoint);
        store
            .set("appPolicies", json!({"Editor":"direct"}))
            .unwrap();
        assert!(set_policy(&mut core, &store, "Cursor".into(), "block".into()).is_err());
        worker.join().unwrap();
        assert_eq!(core.phase, Phase::Connected);
        assert_eq!(
            store.get("appPolicies").unwrap().unwrap(),
            json!({"Editor":"direct"})
        );
        assert_eq!(
            std::fs::read(core.config_path.as_ref().unwrap()).unwrap(),
            original
        );
    }
}
