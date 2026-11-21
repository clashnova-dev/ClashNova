use crate::message::{msg, text};
use crate::{show_main, storage};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    enabled: bool,
    show_window: String,
    toggle_connection: String,
    toggle_route: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            show_window: "CommandOrControl+Shift+A".into(),
            toggle_connection: "CommandOrControl+Shift+X".into(),
            toggle_route: "CommandOrControl+Shift+M".into(),
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    config: Config,
    registered: bool,
    error: Option<String>,
}
pub struct Controls {
    status: Mutex<Status>,
    active: Mutex<HashMap<u32, (Shortcut, &'static str)>>,
    update: Mutex<()>,
    recording: AtomicBool,
}
fn parse(config: &Config) -> Result<HashMap<u32, (Shortcut, &'static str)>, String> {
    let mut map = HashMap::new();
    let mut seen = HashSet::new();
    for (text, action) in [
        (&config.show_window, "show_window"),
        (&config.toggle_connection, "toggle_connection"),
        (&config.toggle_route, "toggle_route"),
    ] {
        if text.is_empty() {
            continue;
        }
        if text.len() > 100 {
            return Err("native.shortcutInvalid".into());
        }
        let key: Shortcut = text.parse().map_err(|_| "native.shortcutInvalid")?;
        if !key
            .mods
            .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
        {
            return Err("native.shortcutNeedsModifier".into());
        }
        if !seen.insert(key.id()) {
            return Err("native.shortcutDuplicate".into());
        }
        if config.enabled {
            map.insert(key.id(), (key, action));
        }
    }
    Ok(map)
}
fn apply(
    app: &AppHandle,
    controls: &Controls,
    config: Config,
    persist: bool,
) -> Result<Status, String> {
    let _update = controls.update.lock().map_err(|_| "native.shortcutsBusy")?;
    let desired = parse(&config)?;
    let previous = controls
        .active
        .lock()
        .map_err(|_| "native.shortcutsUnreadable")?
        .clone();
    let added: Vec<_> = desired
        .iter()
        .filter(|(id, _)| !previous.contains_key(id))
        .map(|(_, v)| v.0)
        .collect();
    let removed: Vec<_> = previous
        .iter()
        .filter(|(id, _)| !desired.contains_key(id))
        .map(|(_, v)| v.0)
        .collect();
    let manager = app.global_shortcut();
    let result = (|| -> Result<(), String> {
        for key in &added {
            manager
                .register(*key)
                .map_err(|_| msg("native.shortcutUnavailable", &[("shortcut", text(key))]))?;
        }
        for key in &removed {
            manager
                .unregister(*key)
                .map_err(|_| "native.shortcutReleaseFailed")?;
        }
        if persist {
            let path = app
                .path()
                .app_config_dir()
                .map_err(|_| "native.settingsDirUnavailable")?
                .join("shortcuts.json");
            storage::write_private(
                &path,
                &serde_json::to_vec(&config).map_err(|_| "native.shortcutsSaveFailed")?,
            )?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        for key in &added {
            if manager.is_registered(*key) {
                let _ = manager.unregister(*key);
            }
        }
        let mut rollback_ok = true;
        for key in &removed {
            if !manager.is_registered(*key) && manager.register(*key).is_err() {
                rollback_ok = false;
            }
        }
        if !rollback_ok {
            let mut status = controls.status.lock().unwrap();
            status.registered = false;
            status.error = Some("native.shortcutsRollbackFailed".into());
        }
        return Err(error);
    }
    *controls.active.lock().unwrap() = desired;
    let status = Status {
        registered: config.enabled,
        config,
        error: None,
    };
    *controls.status.lock().unwrap() = status.clone();
    Ok(status)
}
pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, key, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                let controls = app.state::<Controls>();
                if controls.recording.load(Ordering::Relaxed) {
                    return;
                }
                let action = controls
                    .active
                    .try_lock()
                    .ok()
                    .and_then(|m| m.get(&key.id()).map(|v| v.1));
                match action {
                    Some("show_window") => {
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false)
                                && window.is_focused().unwrap_or(false)
                            {
                                let _ = window.hide();
                            } else {
                                show_main(app);
                            }
                        }
                    }
                    Some("toggle_connection") => {
                        let _ = app.emit("tray://action", "toggle-connection");
                    }
                    Some("toggle_route") => {
                        let _ = app.emit("tray://action", "toggle-route");
                    }
                    _ => {}
                }
            })
            .build(),
    )?;
    let path = app.path().app_config_dir()?.join("shortcuts.json");
    let loaded = match std::fs::read(&path) {
        Ok(raw) => serde_json::from_slice::<Config>(&raw)
            .map_err(|_| "native.shortcutsConfigInvalid".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(_) => Err("native.shortcutsSettingsUnreadable".into()),
    };
    let config = loaded.as_ref().cloned().unwrap_or_default();
    app.manage(Controls {
        status: Mutex::new(Status {
            config: config.clone(),
            registered: false,
            error: loaded.err(),
        }),
        active: Mutex::new(HashMap::new()),
        update: Mutex::new(()),
        recording: AtomicBool::new(false),
    });
    let controls = app.state::<Controls>();
    if controls.status.lock().unwrap().error.is_some() {
        return Ok(());
    }
    if let Err(error) = apply(app, &controls, config, false) {
        controls.status.lock().unwrap().error = Some(error);
    }
    Ok(())
}
pub fn end_recording(app: &AppHandle) {
    if let Some(state) = app.try_state::<Controls>() {
        state.recording.store(false, Ordering::Relaxed);
    }
}

#[tauri::command]
pub fn set_shortcut_recording(state: State<Controls>, recording: bool) {
    state.recording.store(recording, Ordering::Relaxed);
}

#[tauri::command]
pub fn load_shortcuts(state: State<Controls>) -> Result<Status, String> {
    state
        .status
        .lock()
        .map(|s| s.clone())
        .map_err(|_| "native.shortcutsUnreadable".into())
}
#[tauri::command]
pub fn save_shortcuts(
    app: AppHandle,
    state: State<Controls>,
    config: Config,
) -> Result<Status, String> {
    apply(&app, &state, config, true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_and_bare_shortcuts() {
        let mut c = Config {
            enabled: true,
            ..Config::default()
        };
        assert_eq!(parse(&c).unwrap().len(), 3);
        c.toggle_route = c.show_window.clone();
        assert!(parse(&c).is_err());
        c.toggle_route = "Shift+KeyA".into();
        assert!(parse(&c).is_err());
        c.toggle_route = "".into();
        assert_eq!(parse(&c).unwrap().len(), 2);
        c.enabled = false;
        assert!(parse(&c).unwrap().is_empty());
    }
}
