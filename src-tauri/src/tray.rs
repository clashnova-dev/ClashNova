use crate::{connection::Phase, show_main, Core};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, Wry,
};

/// Tray icon for the current connection state. macOS uses a monochrome template
/// (auto-tinted by the menu bar); other platforms use the colored cat. Each has
/// a connected (check) and a disconnected (cross) variant.
fn tray_icon(connected: bool) -> tauri::image::Image<'static> {
    #[cfg(target_os = "macos")]
    {
        if connected {
            tauri::include_image!("icons/tray/mac-on.png")
        } else {
            tauri::include_image!("icons/tray/mac-off.png")
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if connected {
            tauri::include_image!("icons/tray/color-on.png")
        } else {
            tauri::include_image!("icons/tray/color-off.png")
        }
    }
}

#[derive(serde::Deserialize, Clone, PartialEq)]
pub struct TrayNode {
    id: String,
    name: String,
    enabled: bool,
}

pub struct Controls {
    status: MenuItem<Wry>,
    speed: MenuItem<Wry>,
    menu: Menu<Wry>,
    show_speed: AtomicBool,
    speed_rendered: Mutex<Option<(bool, String, String)>>,
    status_label: Mutex<String>,
    nodes: Submenu<Wry>,
    node_items: Mutex<Vec<(TrayNode, CheckMenuItem<Wry>)>>,
    profile: MenuItem<Wry>,
    connect: MenuItem<Wry>,
    disconnect: MenuItem<Wry>,
    modes: Vec<(String, CheckMenuItem<Wry>)>,
    routes: Vec<(String, CheckMenuItem<Wry>)>,
    route_title: MenuItem<Wry>,
    more: Submenu<Wry>,
    labels: Vec<(&'static str, MenuItem<Wry>)>,
}

/// Menu text is copy, so it lives in the frontend dictionary (`tray.*`) and arrives with every
/// `sync_tray`. The menu itself is built in `setup`, before the webview has loaded, and Show window
/// and Quit must work even if it never loads, so these English fallbacks cover the time until the
/// first sync. The last three are the connection states shown by the `status` item.
const FALLBACK: &[(&str, &str)] = &[
    ("open", "Show window"),
    ("system", "System proxy"),
    ("tun", "TUN mode"),
    ("route-title", "Proxy mode"),
    ("global", "Global"),
    ("rule", "Rule"),
    ("profile", "Proxy group"),
    ("node-list", "Select node"),
    ("connect", "Connect"),
    ("disconnect", "Disconnect"),
    ("more", "More"),
    ("sub", "Subscriptions…"),
    ("settings", "Settings…"),
    ("help", "Help…"),
    ("quit", "Quit ClashNova"),
    ("connected", "Connected"),
    ("failed", "Connection failed"),
    ("idle", "Disconnected"),
];

fn fallback_text(id: &str) -> &'static str {
    FALLBACK
        .iter()
        .find(|(item, _)| *item == id)
        .map_or("", |(_, text)| text)
}

/// The label the frontend sent for a menu item, or the fallback when it sent none.
fn label_text<'a>(labels: &'a HashMap<String, String>, id: &str) -> &'a str {
    labels
        .get(id)
        .map(String::as_str)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| fallback_text(id))
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let text = fallback_text;
    let item = |id: &str, enabled| MenuItem::with_id(app, id, text(id), enabled, None::<&str>);
    let idle = format!("ClashNova · {}", text("idle"));
    let status = MenuItem::with_id(app, "status", &idle, false, None::<&str>)?;
    let speed = MenuItem::with_id(app, "traffic-speed", "", false, None::<&str>)?;
    let nodes = Submenu::with_id(app, "node-list", text("node-list"), false)?;
    let profile = item("profile", false)?;
    let route_title = item("route-title", false)?;
    let open = item("open", true)?;
    let connect = item("connect", false)?;
    let disconnect = item("disconnect", false)?;
    let mut modes = Vec::new();
    for id in ["system", "tun"] {
        modes.push((
            id.to_string(),
            CheckMenuItem::with_id(app, id, text(id), true, id == "system", None::<&str>)?,
        ));
    }
    let mut routes = Vec::new();
    for id in ["global", "rule"] {
        routes.push((
            id.to_string(),
            CheckMenuItem::with_id(app, id, text(id), true, id == "rule", None::<&str>)?,
        ));
    }
    let subscriptions = item("sub", true)?;
    let settings = item("settings", true)?;
    let help = item("help", true)?;
    let quit = item("quit", true)?;
    let more = Submenu::with_items(app, text("more"), true, &[&subscriptions, &settings, &help])?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &modes[0].1,
            &modes[1].1,
            &PredefinedMenuItem::separator(app)?,
            &route_title,
            &routes[0].1,
            &routes[1].1,
            &PredefinedMenuItem::separator(app)?,
            &profile,
            &nodes,
            &PredefinedMenuItem::separator(app)?,
            &status,
            &connect,
            &disconnect,
            &PredefinedMenuItem::separator(app)?,
            &more,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    app.manage(Controls {
        status,
        speed,
        menu: menu.clone(),
        show_speed: AtomicBool::new(false),
        speed_rendered: Mutex::new(None),
        status_label: Mutex::new(idle.clone()),
        nodes,
        node_items: Mutex::new(Vec::new()),
        profile,
        connect,
        disconnect,
        modes,
        routes,
        route_title,
        more,
        labels: vec![
            ("open", open),
            ("sub", subscriptions),
            ("settings", settings),
            ("help", help),
            ("quit", quit),
        ],
    });
    TrayIconBuilder::with_id("airport")
        .icon(tray_icon(false))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(&idle)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            action => {
                if matches!(action, "nodes" | "settings" | "sub" | "help") {
                    show_main(app);
                }
                // The same frontend actions regenerate config and reconnect when needed.
                let _ = app.emit("tray://action", action);
            }
        })
        .build(app)?;
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct TraySnapshot {
    #[serde(default, rename = "showTraySpeed")]
    show_tray_speed: bool,
    mode: String,
    route: String,
    node: String,
    selected: String,
    nodes: Vec<TrayNode>,
    profile: String,
    ready: bool,
    busy: bool,
    #[serde(default)]
    labels: HashMap<String, String>,
}

#[tauri::command]
pub fn sync_tray(
    app: AppHandle,
    state: State<Arc<Mutex<Core>>>,
    snapshot: TraySnapshot,
) -> Result<(), String> {
    let TraySnapshot {
        show_tray_speed,
        mode,
        route,
        node,
        selected,
        nodes,
        profile,
        ready,
        busy,
        labels,
    } = snapshot;
    let controls = app.state::<Controls>();
    controls
        .show_speed
        .store(show_tray_speed, Ordering::Relaxed);
    // A connect operation holds the core lock while validating the node.
    // Skip this refresh rather than block the native menu/UI thread.
    let Ok(core) = state.try_lock() else {
        return Ok(());
    };
    let phase = core.phase;
    drop(core);
    let controls = app.state::<Controls>();
    let connected = matches!(phase, Phase::Connected | Phase::Reconnecting);
    let text = |id: &str| label_text(&labels, id);
    let label = text(if connected {
        "connected"
    } else if phase == Phase::Failed {
        "failed"
    } else {
        "idle"
    });
    let update = || -> tauri::Result<()> {
        if let Some(tray) = app.tray_by_id("airport") {
            tray.set_icon(Some(tray_icon(connected)))?;
            // set_icon drops the template flag, so a later icon would render as literal
            // black and stop adapting to the light/dark menu bar — re-assert it here.
            #[cfg(target_os = "macos")]
            tray.set_icon_as_template(true)?;
        }
        controls.status.set_text(format!("ClashNova · {label}"))?;
        *controls.status_label.lock().unwrap() = format!("ClashNova · {label}");
        controls.nodes.set_text(format!(
            "{} · {}",
            text("node-list"),
            node.chars().take(28).collect::<String>().replace('&', "&&")
        ))?;
        controls.nodes.set_enabled(!nodes.is_empty() && !busy)?;
        controls.profile.set_text(format!(
            "{} · {}",
            text("profile"),
            profile
                .chars()
                .take(40)
                .collect::<String>()
                .replace('&', "&&")
        ))?;
        let mut items = controls.node_items.lock().unwrap();
        if items.len() != nodes.len()
            || items.iter().zip(&nodes).any(|((old, _), node)| old != node)
        {
            for (_, item) in items.drain(..) {
                controls.nodes.remove(&item)?;
            }
            for node in &nodes {
                let item = CheckMenuItem::with_id(
                    &app,
                    format!("select-node:{}", node.id),
                    node.name.replace('&', "&&"),
                    node.enabled,
                    node.id == selected,
                    None::<&str>,
                )?;
                controls.nodes.append(&item)?;
                items.push((node.clone(), item));
            }
        }
        for (node, item) in items.iter() {
            item.set_checked(node.id == selected)?;
            item.set_enabled(node.enabled && !busy)?;
        }
        controls
            .connect
            .set_enabled(ready && !busy && matches!(phase, Phase::Idle | Phase::Failed))?;
        controls.disconnect.set_enabled(connected && !busy)?;
        controls.connect.set_text(text("connect"))?;
        controls.disconnect.set_text(text("disconnect"))?;
        controls.route_title.set_text(text("route-title"))?;
        controls.more.set_text(text("more"))?;
        for (id, item) in &controls.labels {
            item.set_text(text(id))?;
        }
        for (id, item) in &controls.modes {
            item.set_text(text(id))?;
            item.set_checked(id == &mode)?;
            item.set_enabled(!busy)?;
        }
        for (id, item) in &controls.routes {
            item.set_text(text(id))?;
            item.set_checked(id == &route)?;
            item.set_enabled(!busy)?;
        }

        Ok(())
    };
    update().map_err(|e| e.to_string())
}

impl Controls {
    pub fn speed_enabled(&self) -> bool {
        self.show_speed.load(Ordering::Relaxed)
    }

    pub fn render_speed(
        &self,
        app: &AppHandle,
        enabled: bool,
        speed: Option<crate::tray_speed::Speed>,
    ) -> tauri::Result<()> {
        let label = if enabled {
            speed
                .map(|value| value.label())
                .unwrap_or_else(|| "↑ —  ↓ —".into())
        } else {
            String::new()
        };
        let status = self.status_label.lock().unwrap().clone();
        let key = (enabled, label.clone(), status.clone());
        let mut rendered = self.speed_rendered.lock().unwrap();
        if rendered.as_ref() == Some(&key) {
            return Ok(());
        }
        if let Some(tray) = app.tray_by_id("airport") {
            // Windows has no native title next to the notification-area icon.
            #[cfg(not(target_os = "windows"))]
            tray.set_title(if enabled { Some(label.as_str()) } else { None })?;
            #[cfg(not(target_os = "linux"))]
            tray.set_tooltip(Some(if enabled {
                format!("{status}\n{label}")
            } else {
                status
            }))?;
        }
        if enabled {
            self.speed.set_text(&label)?;
        }
        let previously_enabled = rendered.as_ref().is_some_and(|previous| previous.0);
        if enabled && !previously_enabled {
            self.menu.insert(&self.speed, 13)?;
        }
        if !enabled && previously_enabled {
            self.menu.remove(&self.speed)?;
        }
        *rendered = Some(key);
        Ok(())
    }
}
