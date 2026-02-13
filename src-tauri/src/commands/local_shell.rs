//! Tauri commands for local shell management.

use serde::{Deserialize, Serialize};
use tauri::{State, AppHandle, Emitter};
use std::sync::Arc;
use log::{info, debug};

use crate::local_shell::{LocalShellManager, LocalShellInfo, ShellInfo};

/// Output event for local shell sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellOutputEvent {
    pub session_id: String,
    pub data: Vec<u8>,
}

/// Request to create a local shell session
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLocalShellRequest {
    /// Shell ID (e.g., "powershell", "bash"). If None, uses default shell.
    pub shell_id: Option<String>,
    /// Terminal columns
    pub cols: Option<u32>,
    /// Terminal rows
    pub rows: Option<u32>,
}

/// Request to send input to a local shell session
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellInputRequest {
    pub session_id: String,
    pub data: String,
}

/// Request to send raw bytes to a local shell session
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellBytesRequest {
    pub session_id: String,
    pub data: Vec<u8>,
}

/// Request to resize a local shell session
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellResizeRequest {
    pub session_id: String,
    pub cols: u32,
    pub rows: u32,
}

/// Request with just session ID
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellSessionRequest {
    pub session_id: String,
}

/// List all available shells on the system
#[tauri::command]
pub fn local_shell_list_shells(
    manager: State<'_, Arc<LocalShellManager>>,
) -> Vec<ShellInfo> {
    info!("[LocalShell Command] Listing available shells");
    manager.list_shells()
}

/// Get the default shell
#[tauri::command]
pub fn local_shell_get_default(
    manager: State<'_, Arc<LocalShellManager>>,
) -> Option<ShellInfo> {
    info!("[LocalShell Command] Getting default shell");
    manager.get_default_shell()
}

/// List all active local shell sessions
#[tauri::command]
pub async fn local_shell_list_sessions(
    manager: State<'_, Arc<LocalShellManager>>,
) -> Result<Vec<LocalShellInfo>, String> {
    info!("[LocalShell Command] Listing sessions");
    Ok(manager.list_sessions().await)
}

/// Create a new local shell session
#[tauri::command]
pub async fn local_shell_create(
    app: AppHandle,
    manager: State<'_, Arc<LocalShellManager>>,
    request: CreateLocalShellRequest,
) -> Result<LocalShellInfo, String> {
    let cols = request.cols.unwrap_or(80) as u16;
    let rows = request.rows.unwrap_or(24) as u16;

    info!("[LocalShell Command] Creating session with shell: {:?}, size: {}x{}",
          request.shell_id, cols, rows);

    // Create the session
    let session = if let Some(shell_id) = request.shell_id {
        manager.create_session(&shell_id, cols, rows).await
    } else {
        manager.create_default_session(cols, rows).await
    }.map_err(|e| e.to_string())?;

    let session_id = session.id.clone();
    let info = session.get_info().await;

    // Subscribe to session output and emit events
    let mut receiver = session.subscribe();
    tokio::spawn(async move {
        debug!("[LocalShell Command] Output bridge started for session {}", session_id);
        while let Ok(data) = receiver.recv().await {
            let event = LocalShellOutputEvent {
                session_id: session_id.clone(),
                data,
            };
            // Emit to frontend using the same event name as SSH sessions
            let _ = app.emit("session-output", event);
        }
        debug!("[LocalShell Command] Output bridge ended for session {}", session_id);
    });

    info!("[LocalShell Command] Session created: {}", info.id);
    Ok(info)
}

/// Send input to a local shell session (as string)
#[tauri::command]
pub async fn local_shell_send_input(
    manager: State<'_, Arc<LocalShellManager>>,
    request: LocalShellInputRequest,
) -> Result<(), String> {
    let session = manager
        .get_session(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session
        .write_input(request.data.as_bytes())
        .map_err(|e| e.to_string())
}

/// Send raw bytes to a local shell session
#[tauri::command]
pub async fn local_shell_send_bytes(
    manager: State<'_, Arc<LocalShellManager>>,
    request: LocalShellBytesRequest,
) -> Result<(), String> {
    let session = manager
        .get_session(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session
        .write_input(&request.data)
        .map_err(|e| e.to_string())
}

/// Resize a local shell session
#[tauri::command]
pub async fn local_shell_resize(
    manager: State<'_, Arc<LocalShellManager>>,
    request: LocalShellResizeRequest,
) -> Result<(), String> {
    let session = manager
        .get_session(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session
        .resize(request.cols as u16, request.rows as u16)
        .map_err(|e| e.to_string())
}

/// Attach to a local shell session
#[tauri::command]
pub async fn local_shell_attach(
    app: AppHandle,
    manager: State<'_, Arc<LocalShellManager>>,
    request: LocalShellSessionRequest,
) -> Result<LocalShellInfo, String> {
    let session = manager
        .get_session(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session.attach();

    let session_id = request.session_id.clone();
    let mut receiver = session.subscribe();

    // Spawn task to forward output to frontend
    tokio::spawn(async move {
        while let Ok(data) = receiver.recv().await {
            let event = LocalShellOutputEvent {
                session_id: session_id.clone(),
                data,
            };
            let _ = app.emit("session-output", event);
        }
    });

    Ok(session.get_info().await)
}

/// Detach from a local shell session
#[tauri::command]
pub async fn local_shell_detach(
    manager: State<'_, Arc<LocalShellManager>>,
    request: LocalShellSessionRequest,
) -> Result<(), String> {
    let session = manager
        .get_session(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session.detach();
    Ok(())
}

/// Kill a local shell session
#[tauri::command]
pub async fn local_shell_kill(
    manager: State<'_, Arc<LocalShellManager>>,
    request: LocalShellSessionRequest,
) -> Result<(), String> {
    info!("[LocalShell Command] Killing session: {}", request.session_id);
    manager
        .kill_session(&request.session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Kill all local shell sessions
#[tauri::command]
pub async fn local_shell_kill_all(
    manager: State<'_, Arc<LocalShellManager>>,
) -> Result<(), String> {
    info!("[LocalShell Command] Killing all sessions");
    manager
        .kill_all()
        .await
        .map_err(|e| e.to_string())
}
