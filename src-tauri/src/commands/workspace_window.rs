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
