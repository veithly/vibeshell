//! Mobile stubs for desktop-only local PTY commands.

use serde::{Deserialize, Serialize};

use crate::local_shell::{LocalShellInfo, ShellInfo};

const UNSUPPORTED: &str = "Local shell sessions are unavailable on mobile";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLocalShellRequest {
    pub shell_id: Option<String>,
    pub cols: Option<u32>,
    pub rows: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellInputRequest {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellBytesRequest {
    pub session_id: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellResizeRequest {
    pub session_id: String,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellSessionRequest {
    pub session_id: String,
}

#[tauri::command]
pub fn local_shell_list_shells() -> Vec<ShellInfo> {
    Vec::new()
}

#[tauri::command]
pub fn local_shell_get_default() -> Option<ShellInfo> {
    None
}

#[tauri::command]
pub async fn local_shell_list_sessions() -> Result<Vec<LocalShellInfo>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub async fn local_shell_create(
    request: CreateLocalShellRequest,
) -> Result<LocalShellInfo, String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}

#[tauri::command]
pub async fn local_shell_send_input(request: LocalShellInputRequest) -> Result<(), String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}

#[tauri::command]
pub async fn local_shell_send_bytes(request: LocalShellBytesRequest) -> Result<(), String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}

#[tauri::command]
pub async fn local_shell_resize(request: LocalShellResizeRequest) -> Result<(), String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}

#[tauri::command]
pub async fn local_shell_attach(
    request: LocalShellSessionRequest,
) -> Result<LocalShellInfo, String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}

#[tauri::command]
pub async fn local_shell_detach(request: LocalShellSessionRequest) -> Result<(), String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}

#[tauri::command]
pub async fn local_shell_kill(request: LocalShellSessionRequest) -> Result<(), String> {
    let _ = request;
    Err(UNSUPPORTED.to_string())
}

#[tauri::command]
pub async fn local_shell_kill_all() -> Result<(), String> {
    Err(UNSUPPORTED.to_string())
}
