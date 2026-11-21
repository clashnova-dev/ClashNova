//! Awaitable native notification boundary: delivery errors never escape as unhandled JS promises.
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
#[tauri::command]
pub async fn send_desktop_notification(
    app: AppHandle,
    title: String,
    body: String,
) -> Result<(), String> {
    if title.chars().count() > 80 || body.chars().count() > 300 || body.trim().is_empty() {
        return Err("native.notificationLengthInvalid".into());
    }
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}
