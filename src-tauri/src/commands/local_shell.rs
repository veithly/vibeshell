//! Tauri commands for local shell management.

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::local_shell::{LocalShellInfo, LocalShellManager, ShellInfo};

/// Window during which terminal output chunks are coalesced into one event so
/// a busy PTY produces a few batched events instead of one per read.
const OUTPUT_COALESCE_WINDOW: std::time::Duration = std::time::Duration::from_millis(12);

/// Maximum payload carried by a single session-output event (before base64).
const MAX_OUTPUT_EVENT_BYTES: usize = 256 * 1024;

/// Replay backlogs are re-chunked to at most this many bytes per event.
const MAX_REPLAY_EVENT_BYTES: usize = 64 * 1024;

fn encode_output_payload(data: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn emit_local_shell_output_event(app: &AppHandle, session_id: &str, data: Vec<u8>) {
    for chunk in data.chunks(MAX_OUTPUT_EVENT_BYTES) {
        let event = LocalShellOutputEvent {
            session_id: session_id.to_string(),
            data: encode_output_payload(chunk),
        };
        let _ = app.emit("session-output", event);
    }
}

pub(crate) fn ensure_local_shell_output_bridge(
    app: AppHandle,
    session: Arc<crate::local_shell::LocalShellSession>,
) {
    if !session.claim_output_bridge() {
        return;
    }

    let session_id = session.id.clone();
    let mut receiver = session.subscribe();
    tokio::spawn(async move {
        debug!(
            "[LocalShell Command] Output bridge started for session {}",
            session_id
        );
        loop {
            // Block until the next chunk. A lagged receiver must NOT kill the
            // bridge: the terminal would freeze with no way to restart it.
            let mut pending = match receiver.recv().await {
                Ok(data) => data,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(
                        "[LocalShell Command] Output bridge lagged {} chunks for session {}",
                        skipped, session_id
                    );
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };

            // Coalesce bursts: drain everything else that arrives within a
            // small window so each event carries a meaningful batch.
            let deadline = tokio::time::Instant::now() + OUTPUT_COALESCE_WINDOW;
            let mut closed = false;
            while let Ok(result) = tokio::time::timeout_at(deadline, receiver.recv()).await {
                match result {
                    Ok(data) => pending.extend_from_slice(&data),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(
                            "[LocalShell Command] Output bridge lagged {} chunks for session {}",
                            skipped, session_id
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        closed = true;
                        break;
                    }
                }
            }

            emit_local_shell_output_event(&app, &session_id, pending);

            if closed {
                break;
            }
        }
        debug!(
            "[LocalShell Command] Output bridge ended for session {}",
            session_id
        );
    });
}

/// Output event for local shell sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellOutputEvent {
    pub session_id: String,
    /// Base64-encoded terminal output. Raw bytes would JSON-serialize as a
    /// number array, inflating every chunk roughly 4x on the IPC bridge.
    pub data: String,
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
pub fn local_shell_list_shells(manager: State<'_, Arc<LocalShellManager>>) -> Vec<ShellInfo> {
    info!("[LocalShell Command] Listing available shells");
    manager.list_shells()
}

/// Get the default shell
#[tauri::command]
pub fn local_shell_get_default(manager: State<'_, Arc<LocalShellManager>>) -> Option<ShellInfo> {
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

    info!(
        "[LocalShell Command] Creating session with shell: {:?}, size: {}x{}",
        request.shell_id, cols, rows
    );

    // Create the session
    let session = if let Some(shell_id) = request.shell_id {
        manager.create_session(&shell_id, cols, rows).await
    } else {
        manager.create_default_session(cols, rows).await
    }
    .map_err(|e| e.to_string())?;

    let info = session.get_info().await;
    ensure_local_shell_output_bridge(app, session);

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
    webview: tauri::WebviewWindow,
    manager: State<'_, Arc<LocalShellManager>>,
    request: LocalShellSessionRequest,
) -> Result<LocalShellInfo, String> {
    let session = manager
        .get_session(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session.attach();

    let session_id = request.session_id.clone();

    // Replay buffered output so a re-attaching frontend (e.g. a split pane
    // recreated after drag) redraws recent history instead of going blank.
    // Emitted before the forwarder subscribes so the listener is already
    // registered on the frontend side (the Terminal registers its listener
    // before invoking local_shell_attach). The backlog is concatenated into
    // <=64KB batched events instead of one emit per buffered chunk.
    let mut backlog: Vec<u8> = Vec::new();
    for data in session.replay_output() {
        backlog.extend_from_slice(&data);
    }
    for chunk in backlog.chunks(MAX_REPLAY_EVENT_BYTES) {
        let event = LocalShellOutputEvent {
            session_id: session_id.clone(),
            data: encode_output_payload(chunk),
        };
        let _ = webview.emit_to(webview.label(), "session-output", event);
    }

    ensure_local_shell_output_bridge(app, session.clone());

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
    info!(
        "[LocalShell Command] Killing session: {}",
        request.session_id
    );
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
    manager.kill_all().await.map_err(|e| e.to_string())
}
