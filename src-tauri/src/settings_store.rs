//! Fixed-file preference access. The webview never chooses a filesystem path.
use serde_json::{Map, Value};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::State;
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const KEYS: &[&str] = &[
    "preferences",
    "subscriptions",
    "networkSettings",
    "networkSecurity",
    "appPolicies",
    "mainWindow",
    "notifications",
    "ipVisible",
    "ipAutoRefresh",
    "session",
];
pub struct SettingsStore {
    path: PathBuf,
    data: Mutex<Result<Map<String, Value>, String>>,
}
impl SettingsStore {
    pub fn open(path: PathBuf) -> Self {
        let read = || -> Result<Map<String, Value>, String> {
            let metadata = match fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
                other => other.map_err(|e| e.to_string())?,
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("native.settingsStoreInvalid".into());
            }
            let mut bytes = Vec::new();
            fs::File::open(&path)
                .map_err(|e| e.to_string())?
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err("native.settingsStoreTooLarge".into());
            }
            let data = serde_json::from_slice(&bytes).map_err(|_| "native.settingsStoreInvalid")?;
            // Preserve the existing plugin file and all values, tightening its permissions atomically.
            crate::storage::write_private(&path, &bytes)?;
            Ok(data)
        };
        let data = read();
        Self {
            path,
            data: Mutex::new(data),
        }
    }
    fn key(key: &str) -> Result<(), String> {
        if KEYS.contains(&key) {
            Ok(())
        } else {
            Err("native.settingsKeyInvalid".into())
        }
    }
    fn get(&self, key: &str) -> Result<Option<Value>, String> {
        Self::key(key)?;
        let data = self
            .data
            .lock()
            .map_err(|_| "native.settingsStoreInvalid")?;
        Ok(data.as_ref().map_err(Clone::clone)?.get(key).cloned())
    }
    fn set(&self, key: &str, value: Value) -> Result<(), String> {
        Self::key(key)?;
        let mut data = self
            .data
            .lock()
            .map_err(|_| "native.settingsStoreInvalid")?;
        let mut next = data.as_ref().map_err(Clone::clone)?.clone();
        next.insert(key.to_owned(), value);
        let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("native.settingsStoreTooLarge".into());
        }
        crate::storage::write_private(&self.path, &bytes)?;
        *data = Ok(next);
        Ok(())
    }
}
#[tauri::command]
pub async fn get_client_setting(
    state: State<'_, Arc<SettingsStore>>,
    key: String,
) -> Result<Option<Value>, String> {
    let store = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.get(&key))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn set_client_setting(
    state: State<'_, Arc<SettingsStore>>,
    key: String,
    value: Value,
) -> Result<(), String> {
    let store = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.set(&key, value))
        .await
        .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ui_and_remembered_session_settings_share_the_private_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("airport-settings.json");
        let store = SettingsStore::open(path.clone());
        store.set("mainWindow", serde_json::json!({"width":900,"height":670})).unwrap();
        store.set("notifications", Value::Bool(false)).unwrap();
        store.set("ipVisible", Value::Bool(false)).unwrap();
        store.set("ipAutoRefresh", Value::Bool(true)).unwrap();
        store.set("session", serde_json::json!({"refreshToken":"test-only-token"})).unwrap();
        let reread = SettingsStore::open(path);
        assert_eq!(reread.get("mainWindow").unwrap().unwrap()["width"], 900);
        assert_eq!(reread.get("notifications").unwrap(), Some(Value::Bool(false)));
        assert_eq!(reread.get("session").unwrap().unwrap()["refreshToken"], "test-only-token");
        // Empty token is an explicit logout marker, not a missing key that could reimport a legacy token.
        reread.set("session", serde_json::json!({"refreshToken":""})).unwrap();
        assert_eq!(reread.get("session").unwrap().unwrap()["refreshToken"], "");
    }
    #[test]
    fn preserves_old_values_with_private_atomic_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("airport-settings.json");
        fs::write(
            &path,
            br#"{"preferences":{"locale":"en"},"subscriptions":[{"id":"existing"}]}"#,
        )
        .unwrap();
        let store = SettingsStore::open(path.clone());
        assert_eq!(
            store.get("subscriptions").unwrap().unwrap()[0]["id"],
            "existing"
        );
        store
            .set("appPolicies", serde_json::json!({"qa":"direct"}))
            .unwrap();
        let reread = SettingsStore::open(path.clone());
        assert_eq!(reread.get("preferences").unwrap().unwrap()["locale"], "en");
        assert_eq!(reread.get("appPolicies").unwrap().unwrap()["qa"], "direct");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn rejects_paths_and_does_not_overwrite_corrupt_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("airport-settings.json");
        fs::write(&path, b"broken").unwrap();
        let store = SettingsStore::open(path.clone());
        assert!(store.get("../tun-service.json").is_err());
        assert!(store.set("/tmp/other", Value::Null).is_err());
        assert!(store.set("preferences", Value::Null).is_err());
        assert_eq!(fs::read(path).unwrap(), b"broken");
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symbolic_linked_stores() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original");
        fs::write(&original, b"{}").unwrap();
        let link = dir.path().join("airport-settings.json");
        std::os::unix::fs::symlink(&original, &link).unwrap();
        let store = SettingsStore::open(link);
        assert!(store.get("preferences").is_err());
        assert_eq!(fs::read(original).unwrap(), b"{}");
    }
}
