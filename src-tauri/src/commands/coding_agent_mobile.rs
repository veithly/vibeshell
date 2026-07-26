//! Mobile stubs for desktop-only local coding-agent commands.

use serde_json::Value;

const UNSUPPORTED: &str = "Local coding agents are unavailable on mobile";

#[tauri::command]
pub async fn coding_agent_list() -> Vec<Value> {
    Vec::new()
}

#[tauri::command]
pub async fn coding_agent_launch(request: Value) -> Result<Value, String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}

#[tauri::command]
pub async fn coding_agent_workspace_status(request: Value) -> Result<Value, String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}

#[tauri::command]
pub async fn coding_agent_workspace_diff(request: Value) -> Result<Value, String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}
