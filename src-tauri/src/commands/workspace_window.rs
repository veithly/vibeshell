//! Native state needed to finish a tab drag outside the originating webview.
//! No hooks or background input recording are installed.
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

static SAVE_HANDLER_READY: AtomicBool = AtomicBool::new(false);

pub fn save_handler_ready() -> bool {
    SAVE_HANDLER_READY.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn workspace_save_handler_ready(window: tauri::Window) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the main workspace owns the save handler".into());
    }
    SAVE_HANDLER_READY.store(true, Ordering::SeqCst);
    Ok(())
}

/// Detaching a pane creates a separate native window after Tauri startup.
/// Install the same glass material before showing its content. Main-window
/// setup happens separately; only detached windows can invoke this command.
#[tauri::command]
pub fn workspace_configure_native_glass(window: tauri::WebviewWindow) -> Result<(), String> {
    if !is_detached_glass_window(window.label()) {
        return Err("Only a detached workspace can initialize its window material".into());
    }
    #[cfg(target_os = "macos")]
    {
        let detached = window.clone();
        window
            .run_on_main_thread(move || crate::platform::configure_native_glass_window(&detached))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn is_detached_glass_window(label: &str) -> bool {
    label.starts_with("detach-") && label.len() > "detach-".len()
}

#[cfg(test)]
mod glass_tests {
    use super::is_detached_glass_window;

    #[test]
    fn native_glass_command_rejects_the_main_or_foreign_window() {
        assert!(!is_detached_glass_window("main"));
        assert!(!is_detached_glass_window("detach-"));
        assert!(!is_detached_glass_window("settings"));
        assert!(is_detached_glass_window("detach-418d030c-a10b-4da4"));
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePointerState {
    x: f64,
    y: f64,
    primary_down: Option<bool>,
    escape_down: bool,
}

#[cfg(target_os = "macos")]
fn buttons() -> (Option<bool>, bool) {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
        fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    }
    // Combined-session state; left button and Escape. Read only during a gesture.
    unsafe {
        (
            Some(CGEventSourceButtonState(0, 0)),
            CGEventSourceKeyState(0, 53),
        )
    }
}

#[cfg(target_os = "windows")]
fn buttons() -> (Option<bool>, bool) {
    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(key: i32) -> i16;
    }
    unsafe { (Some(GetAsyncKeyState(0x01) < 0), GetAsyncKeyState(0x1B) < 0) }
}

#[cfg(all(
    not(any(target_os = "android", target_os = "ios")),
    not(any(target_os = "macos", target_os = "windows"))
))]
fn buttons() -> (Option<bool>, bool) {
    (None, false)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn workspace_pointer_state(window: tauri::Window) -> Result<WorkspacePointerState, String> {
    if window.label() != "main" && !window.label().starts_with("detach-") {
        return Err("Not a workspace window".into());
    }
    let point = window
        .cursor_position()
        .map_err(|error| error.to_string())?;
    let (primary_down, escape_down) = buttons();
    Ok(WorkspacePointerState {
        x: point.x,
        y: point.y,
        primary_down,
        escape_down,
    })
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub fn workspace_pointer_state(_window: tauri::Window) -> Result<WorkspacePointerState, String> {
    Err("Workspace window dragging is unavailable on mobile".into())
}

#[tauri::command]
pub fn workspace_exit(app: tauri::AppHandle, window: tauri::Window) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the main workspace may quit the app".into());
    }
    app.exit(0);
    Ok(())
}
