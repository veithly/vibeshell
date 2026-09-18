use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Emitter, State};

use crate::ipc::{IpcClient, IpcMessage, IpcSessionInfo};
use crate::local_shell::LocalShellManager;
use crate::mcp::SharedAgentInputTracker;
use crate::session::{Session, SessionInfo, SessionManager, SshCredential};
use crate::ssh::PtyConfig;
use crate::storage::Database;

use super::SftpState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub server_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub server_name: String,
    pub auth_type: String,          // "password" or "key"
    pub credential: String,         // password or private key content
    pub passphrase: Option<String>, // for encrypted keys
    pub cols: Option<u32>,
    pub rows: Option<u32>,
    #[serde(default)]
    pub force_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOutputEvent {
    pub session_id: String,
    /// Base64-encoded terminal output. Raw bytes would JSON-serialize as a
    /// number array, inflating every chunk roughly 4x on the IPC bridge.
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendInputRequest {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendBytesRequest {
    pub session_id: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeRequest {
    pub session_id: String,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAccessMode {
    Local,
    Remote,
}

pub struct SessionAccessState {
    mode: RwLock<SessionAccessMode>,
    remote_forwarders: Mutex<HashSet<String>>,
    local_manager: std::sync::OnceLock<std::sync::Weak<SessionManager>>,
}

impl SessionAccessState {
    pub fn new(mode: SessionAccessMode) -> Self {
        Self {
            mode: RwLock::new(mode),
            remote_forwarders: Mutex::new(HashSet::new()),
            local_manager: std::sync::OnceLock::new(),
        }
    }

    pub fn mode(&self) -> SessionAccessMode {
        *self.mode.read().expect("session access mode lock poisoned")
    }

    pub fn set_mode(&self, mode: SessionAccessMode) {
        *self
            .mode
            .write()
            .expect("session access mode lock poisoned") = mode;
    }

    pub fn is_remote(&self) -> bool {
        self.mode() == SessionAccessMode::Remote
    }

    pub fn bind_manager(&self, manager: &Arc<SessionManager>) {
        let _ = self.local_manager.set(Arc::downgrade(manager));
    }

    pub async fn is_remote_session(&self, session_id: &str) -> bool {
        if !self.is_remote() {
            return false;
        }
        match self.local_manager.get().and_then(std::sync::Weak::upgrade) {
            Some(manager) => manager.get(session_id).await.is_none(),
            None => true,
        }
    }

    fn mark_remote_forwarder_started(&self, session_id: &str) -> bool {
        let mut started = self
            .remote_forwarders
            .lock()
            .expect("remote forwarder lock poisoned");
        started.insert(session_id.to_string())
    }

    fn clear_remote_forwarder(&self, session_id: &str) {
        self.remote_forwarders
            .lock()
            .expect("remote forwarder lock poisoned")
            .remove(session_id);
    }
}

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

fn emit_session_output_event(app: &AppHandle, session_id: &str, data: Vec<u8>) {
    for chunk in data.chunks(MAX_OUTPUT_EVENT_BYTES) {
        let event = SessionOutputEvent {
            session_id: session_id.to_string(),
            data: encode_output_payload(chunk),
        };

        let _ = app.emit("session-output", event);
    }
}

async fn emit_replay_output(webview: &tauri::WebviewWindow, session: &Arc<Session>) {
    // Concatenate the snapshot first: one emit per buffered chunk floods the
    // IPC bridge on re-attach. Batches stay small so first paint is fast.
    let mut backlog: Vec<u8> = Vec::new();
    for data in session.replay_output().await {
        backlog.extend_from_slice(&data);
    }
    for chunk in backlog.chunks(MAX_REPLAY_EVENT_BYTES) {
        let _ = webview.emit_to(
            webview.label(),
            "session-output",
            SessionOutputEvent {
                session_id: session.id.clone(),
                data: encode_output_payload(chunk),
            },
        );
    }
}

async fn ensure_session_output_forwarder(app: AppHandle, session: Arc<Session>) {
    if !session.try_start_output_forwarder().await {
        return;
    }

    let session_id = session.id.clone();
    let mut receiver = session.subscribe();

    tokio::spawn(async move {
        loop {
            // Block until the next chunk. A lagged receiver must NOT kill the
            // forwarder: `while let Ok(..)` used to exit permanently on
            // `RecvError::Lagged`, freezing the terminal for the session's
            // lifetime because the forwarder is never restarted.
            let mut pending = match receiver.recv().await {
                Ok(data) => data,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    log::warn!(
                        "[Session] Output forwarder lagged, skipping {} chunks for session {}",
                        skipped,
                        session_id
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
                        log::warn!(
                            "[Session] Output forwarder lagged, skipping {} chunks for session {}",
                            skipped,
                            session_id
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        closed = true;
                        break;
                    }
                }
            }

            emit_session_output_event(&app, &session_id, pending);

            if closed {
                break;
            }
        }
    });
}

pub(crate) async fn ipc_send(message: IpcMessage) -> Result<IpcMessage, String> {
    tokio::task::spawn_blocking(move || IpcClient::send(&message).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("IPC worker failed: {}", e))?
}

fn parse_remote_session_state(state: &str) -> crate::session::SessionState {
    match state {
        "connected" => crate::session::SessionState::Connected,
        "disconnected" => crate::session::SessionState::Disconnected,
        "error" => crate::session::SessionState::Error,
        _ => crate::session::SessionState::Connecting,
    }
}

fn map_remote_session(info: IpcSessionInfo) -> SessionInfo {
    SessionInfo {
        id: info.id,
        server_id: info.server_id,
        server_name: info.server_name,
        state: parse_remote_session_state(&info.state),
        created_at: info.created_at,
        clients: info.clients,
    }
}

async fn fetch_remote_sessions() -> Result<Vec<SessionInfo>, String> {
    match ipc_send(IpcMessage::ListSessions).await? {
        IpcMessage::SessionList { sessions } => {
            Ok(sessions.into_iter().map(map_remote_session).collect())
        }
        IpcMessage::Error { message } => Err(message),
        other => Err(format!(
            "Unexpected IPC response while listing sessions: {:?}",
            other
        )),
    }
}

async fn fetch_remote_session(session_id: &str) -> Result<SessionInfo, String> {
    let sessions = fetch_remote_sessions().await?;
    sessions
        .into_iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))
}

async fn find_remote_reusable_session(server_name: &str) -> Result<Option<SessionInfo>, String> {
    Ok(fetch_remote_sessions()
        .await?
        .into_iter()
        .filter(|session| session.server_name == server_name)
        .filter(|session| matches!(session.state, crate::session::SessionState::Connected))
        .min_by_key(|session| session.created_at))
}

fn start_remote_session_forwarder(
    app: AppHandle,
    session_id: String,
    access_state: Arc<SessionAccessState>,
) {
    if !access_state.mark_remote_forwarder_started(&session_id) {
        return;
    }

    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            use std::io::BufRead;

            let mut reader = IpcClient::connect_streaming(&IpcMessage::AttachSession {
                session_id: session_id.clone(),
            })
            .map_err(|e| e.to_string())?;

            let mut first_line = String::new();
            reader
                .read_line(&mut first_line)
                .map_err(|e| format!("Failed to read remote attach response: {}", e))?;

            match serde_json::from_str::<IpcMessage>(first_line.trim())
                .map_err(|e| format!("Failed to parse remote attach response: {}", e))?
            {
                IpcMessage::Ok => {}
                IpcMessage::Error { message } => return Err(message),
                other => {
                    return Err(format!(
                        "Unexpected IPC response while attaching remote session: {:?}",
                        other
                    ));
                }
            }

            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        match serde_json::from_str::<IpcMessage>(trimmed) {
                            Ok(IpcMessage::SessionOutput { session_id, data }) => {
                                emit_session_output_event(&app, &session_id, data);
                            }
                            Ok(IpcMessage::SessionEnded { .. }) => break,
                            Ok(_) => {}
                            Err(err) => {
                                log::warn!(
                                    "[RemoteSession] Failed to parse streaming IPC payload for {}: {}",
                                    session_id,
                                    err
                                );
                            }
                        }
                    }
                    Err(err) => {
                        return Err(format!(
                            "Failed to read remote streaming output for {}: {}",
                            session_id, err
                        ));
                    }
                }
            }

            Ok(())
        })();

        if let Err(err) = result {
            log::warn!(
                "[RemoteSession] Forwarder for {} stopped with error: {}",
                session_id,
                err
            );
        }

        access_state.clear_remote_forwarder(&session_id);
    });
}

/// List all active sessions
#[tauri::command]
pub async fn session_list(
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
) -> Result<Vec<SessionInfo>, String> {
    let mut sessions = manager.list().await;
    if access_state.is_remote() {
        let local_ids: HashSet<_> = sessions.iter().map(|session| session.id.clone()).collect();
        sessions.extend(
            fetch_remote_sessions()
                .await?
                .into_iter()
                .filter(|session| !local_ids.contains(&session.id)),
        );
    }
    sessions.sort_by_key(|session| session.created_at);
    Ok(sessions)
}

/// Create a new session for a server by name (without connecting)
#[tauri::command]
pub async fn session_create(
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: CreateSessionRequest,
) -> Result<SessionInfo, String> {
    if access_state.is_remote() {
        match ipc_send(IpcMessage::CreateSession {
            server_name: request.server_name,
        })
        .await?
        {
            IpcMessage::SessionCreated { session_id } => {
                return fetch_remote_session(&session_id).await
            }
            IpcMessage::Error { message } => return Err(message),
            other => {
                return Err(format!(
                    "Unexpected IPC response while creating session: {:?}",
                    other
                ));
            }
        }
    }

    let session = manager
        .create_by_name(&request.server_name)
        .await
        .map_err(|e| e.to_string())?;

    Ok(session.get_info().await)
}

/// Create and connect a new SSH session with credentials
#[tauri::command]
pub async fn session_connect(
    app: AppHandle,
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: ConnectRequest,
) -> Result<SessionInfo, String> {
    if access_state.is_remote() {
        if !request.force_new {
            if let Some(session) = find_remote_reusable_session(&request.server_name).await? {
                start_remote_session_forwarder(
                    app,
                    session.id.clone(),
                    access_state.inner().clone(),
                );
                return Ok(session);
            }
        }

        match ipc_send(IpcMessage::CreateSessionWithCredentials {
            server_name: request.server_name,
            auth_type: request.auth_type,
            credential: request.credential,
            passphrase: request.passphrase,
            cols: request.cols,
            rows: request.rows,
        })
        .await?
        {
            IpcMessage::SessionCreated { session_id } => {
                start_remote_session_forwarder(
                    app,
                    session_id.clone(),
                    access_state.inner().clone(),
                );
                return fetch_remote_session(&session_id).await;
            }
            IpcMessage::Error { message } => return Err(message),
            other => {
                return Err(format!(
                    "Unexpected IPC response while connecting session: {:?}",
                    other
                ));
            }
        }
    }

    if !request.force_new {
        if let Some(session) = manager
            .find_reusable_by_server_name(&request.server_name)
            .await
        {
            ensure_session_output_forwarder(app, session.clone()).await;
            return Ok(session.get_info().await);
        }
    }

    // Parse credentials based on auth type
    let ssh_credential = match request.auth_type.as_str() {
        "password" => SshCredential::Password(request.credential),
        "key" => SshCredential::PrivateKey {
            key: request.credential,
            // Treat an empty passphrase as "no passphrase" so unencrypted
            // keys authenticate instead of failing to decode with Some("").
            passphrase: request
                .passphrase
                .filter(|passphrase| !passphrase.is_empty()),
        },
        _ => return Err(format!("Unknown auth type: {}", request.auth_type)),
    };

    // Configure PTY
    let pty_config = Some(PtyConfig {
        term: "xterm-256color".to_string(),
        cols: request.cols.unwrap_or(80),
        rows: request.rows.unwrap_or(24),
        pix_width: 0,
        pix_height: 0,
    });

    // Create and connect session
    let session = manager
        .create_with_credentials(&request.server_name, ssh_credential, pty_config)
        .await
        .map_err(|e| e.to_string())?;

    let info = session.get_info().await;

    // Keep a single long-lived session forwarder alive for future output.
    ensure_session_output_forwarder(app, session).await;

    Ok(info)
}

/// Kill a specific session by ID
#[tauri::command]
pub async fn session_kill(
    manager: State<'_, Arc<SessionManager>>,
    sftp_state: State<'_, Arc<SftpState>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SessionIdRequest,
) -> Result<(), String> {
    // Clean up SFTP session state to prevent memory leaks
    sftp_state.cleanup_session(&request.session_id).await;

    if access_state.is_remote_session(&request.session_id).await {
        return match ipc_send(IpcMessage::KillSession {
            session_id: request.session_id,
        })
        .await?
        {
            IpcMessage::Ok => Ok(()),
            IpcMessage::Error { message } => Err(message),
            other => Err(format!(
                "Unexpected IPC response while killing session: {:?}",
                other
            )),
        };
    }

    manager
        .kill(&request.session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Send input data to a session (as string)
#[tauri::command]
pub async fn session_send_input(
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    input_tracker: State<'_, Arc<SharedAgentInputTracker>>,
    request: SendInputRequest,
) -> Result<(), String> {
    // Human and AI keystrokes share one PTY line. Feed human input through the
    // same tracker so an AI-submitted Enter is classified against the actual
    // mixed line, not only the bytes previously sent by the agent.
    let _session_input_guard = input_tracker.lock_session(&request.session_id).await;
    let (checkpoint, _) = input_tracker
        .checkpoint_and_observe(&request.session_id, &request.data, &[], false)
        .await;

    let result = if access_state.is_remote_session(&request.session_id).await {
        match ipc_send(IpcMessage::SendUserInput {
            session_id: request.session_id,
            data: request.data.into_bytes(),
        })
        .await
        {
            Ok(IpcMessage::Ok) => Ok(()),
            Ok(IpcMessage::Error { message }) => Err(message),
            Ok(other) => Err(format!(
                "Unexpected IPC response while sending input: {:?}",
                other
            )),
            Err(error) => Err(error),
        }
    } else {
        match manager.get(&request.session_id).await {
            Some(session) => session
                .write_to_ssh(request.data.as_bytes())
                .await
                .map_err(|error| error.to_string()),
            None => Err(format!("Session not found: {}", request.session_id)),
        }
    };

    if result.is_err() {
        input_tracker.restore(checkpoint).await;
    }
    result
}

/// Send raw input data to a session (as bytes)
#[tauri::command]
pub async fn session_send_bytes(
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    input_tracker: State<'_, Arc<SharedAgentInputTracker>>,
    request: SendBytesRequest,
) -> Result<(), String> {
    let _session_input_guard = input_tracker.lock_session(&request.session_id).await;
    let input = String::from_utf8_lossy(&request.data);
    let (checkpoint, _) = input_tracker
        .checkpoint_and_observe(&request.session_id, &input, &[], false)
        .await;
    drop(input);

    let result = if access_state.is_remote_session(&request.session_id).await {
        match ipc_send(IpcMessage::SendUserInput {
            session_id: request.session_id,
            data: request.data,
        })
        .await
        {
            Ok(IpcMessage::Ok) => Ok(()),
            Ok(IpcMessage::Error { message }) => Err(message),
            Ok(other) => Err(format!(
                "Unexpected IPC response while sending bytes: {:?}",
                other
            )),
            Err(error) => Err(error),
        }
    } else {
        match manager.get(&request.session_id).await {
            Some(session) => session
                .write_to_ssh(&request.data)
                .await
                .map_err(|error| error.to_string()),
            None => Err(format!("Session not found: {}", request.session_id)),
        }
    };

    if result.is_err() {
        input_tracker.restore(checkpoint).await;
    }
    result
}

/// Resize a session's terminal
#[tauri::command]
pub async fn session_resize(
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: ResizeRequest,
) -> Result<(), String> {
    if access_state.is_remote_session(&request.session_id).await {
        return match ipc_send(IpcMessage::Resize {
            session_id: request.session_id,
            cols: request.cols,
            rows: request.rows,
        })
        .await?
        {
            IpcMessage::Ok => Ok(()),
            IpcMessage::Error { message } => Err(message),
            other => Err(format!(
                "Unexpected IPC response while resizing session: {:?}",
                other
            )),
        };
    }

    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session
        .resize_pty(request.cols, request.rows)
        .await
        .map_err(|e| e.to_string())
}

/// Attach to a session and start receiving output events
#[tauri::command]
pub async fn session_attach(
    app: AppHandle,
    webview: tauri::WebviewWindow,
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SessionIdRequest,
) -> Result<SessionInfo, String> {
    if access_state.is_remote_session(&request.session_id).await {
        start_remote_session_forwarder(
            app,
            request.session_id.clone(),
            access_state.inner().clone(),
        );
        return fetch_remote_session(&request.session_id).await;
    }

    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session.attach().await;

    // Replay buffered output so late listeners still receive the initial prompt/MOTD.
    emit_replay_output(&webview, &session).await;

    // Ensure future output continues flowing to the frontend without duplicate forwarders.
    ensure_session_output_forwarder(app, session.clone()).await;

    Ok(session.get_info().await)
}

/// Detach from a session
#[tauri::command]
pub async fn session_detach(
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SessionIdRequest,
) -> Result<(), String> {
    if access_state.is_remote_session(&request.session_id).await {
        return match ipc_send(IpcMessage::DetachSession {
            session_id: request.session_id,
        })
        .await?
        {
            IpcMessage::Ok => Ok(()),
            IpcMessage::Error { message } => Err(message),
            other => Err(format!(
                "Unexpected IPC response while detaching session: {:?}",
                other
            )),
        };
    }

    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    session.detach().await;
    Ok(())
}

/// Execute on an independent channel, preserving the real exit code. Both
/// primary and daemon-backed GUI paths share 10-second / 1-MiB limits.
pub type SessionExecResult = crate::ssh::client::CommandResult;

#[tauri::command]
pub async fn session_exec_command(
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    session_id: String,
    command: String,
) -> Result<SessionExecResult, String> {
    if access_state.is_remote_session(&session_id).await {
        return match ipc_send(IpcMessage::ExecQuickCommand { session_id, command }).await? {
            IpcMessage::CommandResult { output, exit_code } => Ok(SessionExecResult { output, exit_code }),
            IpcMessage::Error { message } => Err(message),
            _ => Err("Background service does not support command exit status; restart it with the matching VibeShell version".into()),
        };
    }
    let session = manager
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    session
        .exec_quick_command(&command)
        .await
        .map_err(|error| format!("{error:#}"))
}
// =============================================================================
// Server Status Monitoring Types and Commands
// =============================================================================

/// CPU usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo {
    /// Overall CPU usage percentage (0-100)
    pub usage_percent: f64,
    /// Number of CPU cores
    pub core_count: u32,
    /// Load average (1 min, 5 min, 15 min)
    pub load_average: [f64; 3],
}

/// Memory usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    /// Total memory in bytes
    pub total: u64,
    /// Used memory in bytes
    pub used: u64,
    /// Free memory in bytes
    pub free: u64,
    /// Available memory in bytes
    pub available: u64,
    /// Usage percentage (0-100)
    pub usage_percent: f64,
    /// Swap total in bytes
    pub swap_total: u64,
    /// Swap used in bytes
    pub swap_used: u64,
}

/// Disk usage information for a mount point
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    /// Mount point path
    pub mount_point: String,
    /// Filesystem type
    pub filesystem: String,
    /// Total size in bytes
    pub total: u64,
    /// Used space in bytes
    pub used: u64,
    /// Available space in bytes
    pub available: u64,
    /// Usage percentage (0-100)
    pub usage_percent: f64,
}

/// Network interface statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    /// Interface name
    pub interface: String,
    /// Bytes received
    pub rx_bytes: u64,
    /// Bytes transmitted
    pub tx_bytes: u64,
    /// Packets received
    pub rx_packets: u64,
    /// Packets transmitted
    pub tx_packets: u64,
}

/// Complete server status information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    /// Server hostname
    pub hostname: String,
    /// System uptime in seconds
    pub uptime_seconds: u64,
    /// CPU information
    pub cpu: CpuInfo,
    /// Memory information
    pub memory: MemoryInfo,
    /// Disk information (one entry per mount point)
    pub disks: Vec<DiskInfo>,
    /// Network interface information
    pub network: Vec<NetworkInfo>,
    /// Timestamp when this status was collected (Unix timestamp)
    pub collected_at: i64,
}

/// Parse the output of /proc/stat to get CPU usage
fn parse_cpu_usage(stat_output: &str, num_cpus_output: &str) -> CpuInfo {
    let mut usage_percent = 0.0;
    let mut core_count = 1u32;
    let load_average = [0.0, 0.0, 0.0];

    // Parse core count
    if let Ok(count) = num_cpus_output.trim().parse::<u32>() {
        core_count = count;
    }

    // Parse CPU usage from /proc/stat
    // The first line is: cpu  user nice system idle iowait irq softirq steal guest guest_nice
    for line in stat_output.lines() {
        if line.starts_with("cpu ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                let user: u64 = parts[1].parse().unwrap_or(0);
                let nice: u64 = parts[2].parse().unwrap_or(0);
                let system: u64 = parts[3].parse().unwrap_or(0);
                let idle: u64 = parts[4].parse().unwrap_or(0);
                let iowait: u64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);

                let total = user + nice + system + idle + iowait;
                let used = user + nice + system;

                if total > 0 {
                    usage_percent = (used as f64 / total as f64) * 100.0;
                }
            }
            break;
        }
    }

    CpuInfo {
        usage_percent,
        core_count,
        load_average,
    }
}

/// Parse the output of /proc/loadavg
fn parse_load_average(loadavg_output: &str) -> [f64; 3] {
    let parts: Vec<&str> = loadavg_output.split_whitespace().collect();
    let mut load = [0.0, 0.0, 0.0];

    if parts.len() >= 3 {
        load[0] = parts[0].parse().unwrap_or(0.0);
        load[1] = parts[1].parse().unwrap_or(0.0);
        load[2] = parts[2].parse().unwrap_or(0.0);
    }

    load
}

/// Parse the output of /proc/meminfo
fn parse_memory_info(meminfo_output: &str) -> MemoryInfo {
    let mut total = 0u64;
    let mut free = 0u64;
    let mut available = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;

    for line in meminfo_output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let value: u64 = parts[1].parse().unwrap_or(0) * 1024; // Convert kB to bytes
            match parts[0] {
                "MemTotal:" => total = value,
                "MemFree:" => free = value,
                "MemAvailable:" => available = value,
                "Buffers:" => buffers = value,
                "Cached:" => cached = value,
                "SwapTotal:" => swap_total = value,
                "SwapFree:" => swap_free = value,
                _ => {}
            }
        }
    }

    // If MemAvailable is not present (older kernels), estimate it
    if available == 0 {
        available = free + buffers + cached;
    }

    let used = total.saturating_sub(available);
    let usage_percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    MemoryInfo {
        total,
        used,
        free,
        available,
        usage_percent,
        swap_total,
        swap_used: swap_total.saturating_sub(swap_free),
    }
}

/// Parse the output of df command
fn parse_disk_info(df_output: &str) -> Vec<DiskInfo> {
    let mut disks = Vec::new();

    for line in df_output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 6 {
            // Skip pseudo filesystems
            let filesystem = parts[0];
            let mount_point = parts[5];

            // Only include real filesystems
            if filesystem.starts_with('/')
                || filesystem.starts_with("tmpfs")
                || filesystem.starts_with("/dev")
            {
                // Skip tmpfs, devtmpfs, etc. but keep /dev/* partitions
                if mount_point == "/"
                    || mount_point.starts_with("/home")
                    || mount_point.starts_with("/var")
                    || mount_point.starts_with("/mnt")
                    || mount_point.starts_with("/data")
                {
                    let total: u64 = parts[1].parse().unwrap_or(0) * 1024; // Convert 1K blocks to bytes
                    let used: u64 = parts[2].parse().unwrap_or(0) * 1024;
                    let available: u64 = parts[3].parse().unwrap_or(0) * 1024;

                    let usage_percent = if total > 0 {
                        (used as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };

                    disks.push(DiskInfo {
                        mount_point: mount_point.to_string(),
                        filesystem: filesystem.to_string(),
                        total,
                        used,
                        available,
                        usage_percent,
                    });
                }
            }
        }
    }

    // If no disks were found with strict filtering, try to get at least root
    if disks.is_empty() {
        for line in df_output.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 && parts[5] == "/" {
                let total: u64 = parts[1].parse().unwrap_or(0) * 1024;
                let used: u64 = parts[2].parse().unwrap_or(0) * 1024;
                let available: u64 = parts[3].parse().unwrap_or(0) * 1024;

                let usage_percent = if total > 0 {
                    (used as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                disks.push(DiskInfo {
                    mount_point: "/".to_string(),
                    filesystem: parts[0].to_string(),
                    total,
                    used,
                    available,
                    usage_percent,
                });
                break;
            }
        }
    }

    disks
}

/// Parse the output of /proc/net/dev
fn parse_network_info(netdev_output: &str) -> Vec<NetworkInfo> {
    let mut interfaces = Vec::new();

    for line in netdev_output.lines().skip(2) {
        let line = line.trim();
        if let Some(colon_pos) = line.find(':') {
            let interface = line[..colon_pos].trim();
            let stats = line[colon_pos + 1..].trim();
            let parts: Vec<&str> = stats.split_whitespace().collect();

            // Skip loopback interface
            if interface == "lo" {
                continue;
            }

            if parts.len() >= 10 {
                interfaces.push(NetworkInfo {
                    interface: interface.to_string(),
                    rx_bytes: parts[0].parse().unwrap_or(0),
                    rx_packets: parts[1].parse().unwrap_or(0),
                    tx_bytes: parts[8].parse().unwrap_or(0),
                    tx_packets: parts[9].parse().unwrap_or(0),
                });
            }
        }
    }

    interfaces
}

/// Parse uptime from /proc/uptime
fn parse_uptime(uptime_output: &str) -> u64 {
    let parts: Vec<&str> = uptime_output.split_whitespace().collect();
    if !parts.is_empty() {
        parts[0].parse::<f64>().unwrap_or(0.0) as u64
    } else {
        0
    }
}

fn default_cpu_info() -> CpuInfo {
    CpuInfo {
        usage_percent: 0.0,
        core_count: 1,
        load_average: [0.0, 0.0, 0.0],
    }
}

fn default_memory_info() -> MemoryInfo {
    MemoryInfo {
        total: 0,
        used: 0,
        free: 0,
        available: 0,
        usage_percent: 0.0,
        swap_total: 0,
        swap_used: 0,
    }
}

fn run_command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_load_average_macos(vm_loadavg_output: &str) -> [f64; 3] {
    // macOS `sysctl -n vm.loadavg` output example: { 1.20 1.35 1.42 }
    let normalized = vm_loadavg_output.replace(['{', '}'], "").trim().to_string();

    let parts: Vec<&str> = normalized.split_whitespace().collect();
    let mut load = [0.0, 0.0, 0.0];
    if parts.len() >= 3 {
        load[0] = parts[0].parse().unwrap_or(0.0);
        load[1] = parts[1].parse().unwrap_or(0.0);
        load[2] = parts[2].parse().unwrap_or(0.0);
    }

    load
}

fn parse_memory_info_macos(vm_stat_output: &str, memsize_output: &str) -> MemoryInfo {
    let total = memsize_output.trim().parse::<u64>().unwrap_or(0);

    let mut page_size = 4096u64;
    let mut free_pages = 0u64;
    let mut inactive_pages = 0u64;
    let mut speculative_pages = 0u64;

    for raw_line in vm_stat_output.lines() {
        let line = raw_line.trim();

        if let Some(value) = line.strip_prefix("Mach Virtual Memory Statistics: (page size of ") {
            if let Some(bytes_text) = value.strip_suffix(" bytes)") {
                page_size = bytes_text.parse::<u64>().unwrap_or(4096);
            }
            continue;
        }

        let parse_pages = |line: &str, key: &str| -> Option<u64> {
            let value = line.strip_prefix(key)?;
            let cleaned = value.trim().trim_end_matches('.').replace('.', "");
            cleaned.parse::<u64>().ok()
        };

        if let Some(v) = parse_pages(line, "Pages free:") {
            free_pages = v;
        } else if let Some(v) = parse_pages(line, "Pages inactive:") {
            inactive_pages = v;
        } else if let Some(v) = parse_pages(line, "Pages speculative:") {
            speculative_pages = v;
        }
    }

    let free = free_pages.saturating_mul(page_size);
    let available = free_pages
        .saturating_add(inactive_pages)
        .saturating_add(speculative_pages)
        .saturating_mul(page_size);
    let used = total.saturating_sub(available);

    let usage_percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    MemoryInfo {
        total,
        used,
        free,
        available,
        usage_percent,
        swap_total: 0,
        swap_used: 0,
    }
}

fn parse_wmic_value(output: &str, key: &str) -> Option<u64> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix(&format!("{}=", key)) {
            if let Ok(parsed) = value.trim().parse::<u64>() {
                return Some(parsed);
            }
        }
    }
    None
}

fn parse_disk_info_windows(wmic_output: &str) -> Vec<DiskInfo> {
    let mut disks = Vec::new();

    for line in wmic_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Node,") {
            continue;
        }

        let parts: Vec<&str> = trimmed.split(',').collect();
        if parts.len() < 5 {
            continue;
        }

        let filesystem = parts[2].trim().to_string();
        let mount_point = parts[1].trim().to_string();
        let free = parts[3].trim().parse::<u64>().unwrap_or(0);
        let total = parts[4].trim().parse::<u64>().unwrap_or(0);
        let used = total.saturating_sub(free);
        let usage_percent = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        disks.push(DiskInfo {
            mount_point,
            filesystem,
            total,
            used,
            available: free,
            usage_percent,
        });
    }

    disks
}

pub(crate) fn collect_local_server_status() -> ServerStatus {
    let os = std::env::consts::OS;

    let hostname = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| run_command_output("hostname", &[]).map(|s| s.trim().to_string()))
        .unwrap_or_else(|| "localhost".to_string());

    let core_count = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);

    let mut cpu = default_cpu_info();
    cpu.core_count = core_count;

    let mut memory = default_memory_info();
    let mut disks: Vec<DiskInfo> = Vec::new();
    let network: Vec<NetworkInfo> = Vec::new();

    // Local session data collection is OS-specific:
    // - Linux: prefer /proc-based metrics for consistency with remote SSH Linux collection.
    // - macOS: use `sysctl` and `vm_stat` because /proc is unavailable.
    // - Windows: use WMIC CSV/value output where available; otherwise gracefully degrade.
    match os {
        "linux" => {
            if let Ok(loadavg) = std::fs::read_to_string("/proc/loadavg") {
                cpu.load_average = parse_load_average(&loadavg);
            }

            if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
                memory = parse_memory_info(&meminfo);
            }

            if let Some(df_output) = run_command_output("df", &["-P"]) {
                disks = parse_disk_info(&df_output);
            }
        }
        "macos" => {
            if let Some(loadavg) = run_command_output("sysctl", &["-n", "vm.loadavg"]) {
                cpu.load_average = parse_load_average_macos(&loadavg);
            }

            let vm_stat = run_command_output("vm_stat", &[]).unwrap_or_default();
            let memsize = run_command_output("sysctl", &["-n", "hw.memsize"]).unwrap_or_default();
            if !vm_stat.is_empty() || !memsize.trim().is_empty() {
                memory = parse_memory_info_macos(&vm_stat, &memsize);
            }

            if let Some(df_output) = run_command_output("df", &["-P"]) {
                disks = parse_disk_info(&df_output);
            }
        }
        "windows" => {
            if let Some(mem_output) = run_command_output(
                "wmic",
                &[
                    "OS",
                    "get",
                    "FreePhysicalMemory,TotalVisibleMemorySize",
                    "/value",
                ],
            ) {
                let free_kb = parse_wmic_value(&mem_output, "FreePhysicalMemory").unwrap_or(0);
                let total_kb = parse_wmic_value(&mem_output, "TotalVisibleMemorySize").unwrap_or(0);
                let free = free_kb.saturating_mul(1024);
                let total = total_kb.saturating_mul(1024);
                let used = total.saturating_sub(free);
                let usage_percent = if total > 0 {
                    (used as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                memory = MemoryInfo {
                    total,
                    used,
                    free,
                    available: free,
                    usage_percent,
                    swap_total: 0,
                    swap_used: 0,
                };
            }

            if let Some(disk_output) = run_command_output(
                "wmic",
                &[
                    "logicaldisk",
                    "get",
                    "DeviceID,FileSystem,FreeSpace,Size",
                    "/format:csv",
                ],
            ) {
                disks = parse_disk_info_windows(&disk_output);
            }
        }
        _ => {}
    }

    ServerStatus {
        hostname,
        uptime_seconds: 0,
        cpu,
        memory,
        disks,
        network,
        collected_at: chrono::Utc::now().timestamp(),
    }
}

pub(crate) const REMOTE_STATUS_COMMAND: &str = r#"
echo "===HOSTNAME==="; hostname;
echo "===UPTIME==="; cat /proc/uptime;
echo "===LOADAVG==="; cat /proc/loadavg;
echo "===CPUCOUNT==="; nproc;
echo "===CPUSTAT==="; head -1 /proc/stat;
echo "===MEMINFO==="; cat /proc/meminfo;
echo "===DISKINFO==="; df -P;
echo "===NETDEV==="; cat /proc/net/dev
"#;

/// Get server status metrics.
/// - Local session: collect from host OS without SSH exec, with OS-specific fallbacks.
/// - Remote SSH session: keep existing Linux /proc-based collection through SSH exec.
#[tauri::command]
pub async fn get_server_status(
    manager: State<'_, Arc<SessionManager>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    db: State<'_, Arc<Database>>,
    request: SessionIdRequest,
) -> Result<ServerStatus, String> {
    db.plugin_installation_get("server-performance")
        .map_err(|error| error.to_string())?
        .filter(|installation| installation.source == "builtin" && installation.enabled)
        .ok_or_else(|| "Server Performance plugin is not enabled".to_string())?;

    // Local shell sessions are not SSH-backed; collect metrics directly from
    // this machine. The scrape spawns subprocesses (`df`, `sysctl`, `vm_stat`,
    // `wmic`), so keep the blocking IO off the async runtime.
    if local_shell_manager
        .get_session(&request.session_id)
        .await
        .is_some()
    {
        return tokio::task::spawn_blocking(collect_local_server_status)
            .await
            .map_err(|error| format!("Local status collection task failed: {}", error));
    }

    // Remote SSH logic remains Linux-oriented and unchanged.
    let combined_cmd = REMOTE_STATUS_COMMAND;

    let output = if let Some(session) = manager.get(&request.session_id).await {
        session
            .exec_command(combined_cmd)
            .await
            .map_err(|e| format!("Failed to execute status command: {}", e))?
    } else if access_state.is_remote() {
        // This is automatic UI telemetry, not an Agent-issued command.
        // Keep it on the existing guarded UI RPC so it cannot flood the audit.
        match ipc_send(IpcMessage::ExecQuickCommand {
            session_id: request.session_id.clone(),
            command: combined_cmd.to_string(),
        })
        .await?
        {
            IpcMessage::CommandResult {
                output,
                exit_code: 0,
            } => output,
            IpcMessage::CommandResult { exit_code, .. } => {
                return Err(format!("Status collection exited with code {exit_code}"))
            }
            IpcMessage::Error { message } => return Err(message),
            other => {
                return Err(format!(
                    "Unexpected IPC response while collecting server status: {:?}",
                    other
                ));
            }
        }
    } else {
        return Err(format!("Session not found: {}", request.session_id));
    };

    parse_remote_server_status(&output)
}

pub(crate) fn parse_remote_server_status(output: &str) -> Result<ServerStatus, String> {
    // Parse the combined output shared by the UI and native plugin read API.
    let mut hostname = String::from("unknown");
    let mut uptime_seconds = 0u64;
    let mut load_average = [0.0, 0.0, 0.0];
    let mut cpu_count = 1u32;
    let mut cpu_stat = String::new();
    let mut meminfo = String::new();
    let mut df_output = String::new();
    let mut netdev = String::new();

    let mut current_section = "";
    let mut section_buffer = String::new();

    for line in output.lines() {
        if line.starts_with("===") && line.ends_with("===") {
            // Save previous section
            match current_section {
                "HOSTNAME" => hostname = section_buffer.trim().to_string(),
                "UPTIME" => uptime_seconds = parse_uptime(&section_buffer),
                "LOADAVG" => load_average = parse_load_average(&section_buffer),
                "CPUCOUNT" => cpu_count = section_buffer.trim().parse().unwrap_or(1),
                "CPUSTAT" => cpu_stat = section_buffer.clone(),
                "MEMINFO" => meminfo = section_buffer.clone(),
                "DISKINFO" => df_output = section_buffer.clone(),
                "NETDEV" => netdev = section_buffer.clone(),
                _ => {}
            }

            // Start new section
            current_section = line.trim_matches('=');
            section_buffer.clear();
        } else {
            section_buffer.push_str(line);
            section_buffer.push('\n');
        }
    }

    // Don't forget the last section
    match current_section {
        "HOSTNAME" => hostname = section_buffer.trim().to_string(),
        "UPTIME" => uptime_seconds = parse_uptime(&section_buffer),
        "LOADAVG" => load_average = parse_load_average(&section_buffer),
        "CPUCOUNT" => cpu_count = section_buffer.trim().parse().unwrap_or(1),
        "CPUSTAT" => cpu_stat = section_buffer.clone(),
        "MEMINFO" => meminfo = section_buffer.clone(),
        "DISKINFO" => df_output = section_buffer.clone(),
        "NETDEV" => netdev = section_buffer.clone(),
        _ => {}
    }

    // Parse CPU info
    let mut cpu = parse_cpu_usage(&cpu_stat, &cpu_count.to_string());
    cpu.load_average = load_average;
    cpu.core_count = cpu_count;

    // Parse memory info
    let memory = parse_memory_info(&meminfo);

    // Parse disk info
    let disks = parse_disk_info(&df_output);

    // Parse network info
    let network = parse_network_info(&netdev);

    Ok(ServerStatus {
        hostname,
        uptime_seconds,
        cpu,
        memory,
        disks,
        network,
        collected_at: chrono::Utc::now().timestamp(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Float comparison helper: the parsers do f64 division, so exact equality
    /// on hand-computed expectations needs a small tolerance.
    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "expected {}, got {}",
            expected,
            actual
        );
    }

    // =========================================================================
    // parse_cpu_usage (/proc/stat + nproc)
    // =========================================================================

    #[test]
    fn cpu_usage_parses_proc_stat_aggregate_line() {
        let stat_output = "\
cpu  74608 2520 24433 1117073 6176 4054 0 0 0 0
cpu0 18700 630 6108 279268 1544 1013 0 0 0 0
cpu1 16532 448 6052 278945 1508 1015 0 0 0 0
intr 12345678 0 0 0 0
ctxt 98765432
btime 1699999999
";
        let cpu = parse_cpu_usage(stat_output, "2\n");

        // used = 74608+2520+24433 = 101561; total = used+idle+iowait = 1224810
        assert_close(cpu.usage_percent, 8.29198);
        assert_eq!(cpu.core_count, 2);
    }

    #[test]
    fn cpu_usage_all_idle_is_zero() {
        let stat_output = "cpu  0 0 0 5 0 0 0 0 0 0\ncpu0 0 0 0 5 0 0 0 0 0 0\n";
        let cpu = parse_cpu_usage(stat_output, "1\n");
        assert_close(cpu.usage_percent, 0.0);
        assert_eq!(cpu.core_count, 1);
    }

    #[test]
    fn cpu_usage_garbage_stat_output_does_not_panic() {
        // No "cpu " line, unparsable fields: must stay at defaults, never panic.
        let cpu = parse_cpu_usage(
            "cpuX 1 2 3 4 5\ncpu  garbage a b c\ntotally junk \u{0}\u{1}\n",
            "many\n",
        );
        assert_close(cpu.usage_percent, 0.0);
        // Unparsable core count falls back to the default of 1.
        assert_eq!(cpu.core_count, 1);
    }

    // =========================================================================
    // parse_load_average (/proc/loadavg)
    // =========================================================================

    #[test]
    fn load_average_parses_proc_loadavg() {
        let load = parse_load_average("0.52 0.58 0.59 2/1042 23172\n");
        assert_close(load[0], 0.52);
        assert_close(load[1], 0.58);
        assert_close(load[2], 0.59);
    }

    #[test]
    fn load_average_handles_multi_digit_values() {
        let load = parse_load_average("102.54 99.88 75.10 789/12345 98765\n");
        assert_close(load[0], 102.54);
        assert_close(load[1], 99.88);
        assert_close(load[2], 75.10);
    }

    #[test]
    fn load_average_with_fewer_than_three_fields_is_zeroed() {
        let load = parse_load_average("0.10 0.20\n");
        assert_eq!(load, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn load_average_garbage_does_not_panic() {
        assert_eq!(parse_load_average("abc def ghi 1/1 1\n"), [0.0, 0.0, 0.0]);
        assert_eq!(parse_load_average(""), [0.0, 0.0, 0.0]);
    }

    // =========================================================================
    // parse_memory_info (/proc/meminfo, values in kB)
    // =========================================================================

    #[test]
    fn memory_info_parses_proc_meminfo() {
        let meminfo = "\
MemTotal:       16384000 kB
MemFree:         4194304 kB
MemAvailable:    8388608 kB
Buffers:          524288 kB
Cached:          3145728 kB
SwapCached:            0 kB
Active:          6291456 kB
Inactive:        3145728 kB
SwapTotal:       2097152 kB
SwapFree:        1048576 kB
Dirty:               128 kB
HugePages_Total:       0
";
        let mem = parse_memory_info(meminfo);

        assert_eq!(mem.total, 16384000 * 1024);
        assert_eq!(mem.free, 4194304 * 1024);
        assert_eq!(mem.available, 8388608 * 1024);
        assert_eq!(mem.used, (16384000 - 8388608) * 1024);
        assert_close(mem.usage_percent, 48.8);
        assert_eq!(mem.swap_total, 2097152 * 1024);
        assert_eq!(mem.swap_used, 1048576 * 1024);
    }

    #[test]
    fn memory_info_estimates_available_on_old_kernels() {
        // Pre-3.14 kernels have no MemAvailable line.
        let meminfo = "\
MemTotal:       16384000 kB
MemFree:         4194304 kB
Buffers:          524288 kB
Cached:          3145728 kB
";
        let mem = parse_memory_info(meminfo);

        let expected_available = (4194304 + 524288 + 3145728) * 1024;
        assert_eq!(mem.available, expected_available);
        assert_eq!(mem.used, (16384000 - 4194304 - 524288 - 3145728) * 1024);
        assert_close(mem.usage_percent, 52.0);
    }

    #[test]
    fn memory_info_empty_input_is_all_zeros() {
        let mem = parse_memory_info("");
        assert_eq!(mem.total, 0);
        assert_eq!(mem.used, 0);
        assert_eq!(mem.available, 0);
        assert_close(mem.usage_percent, 0.0);
    }

    #[test]
    fn memory_info_garbage_values_do_not_panic() {
        let mem = parse_memory_info("MemTotal: abc kB\nTotal bogus\n:\u{0} 5\n");
        assert_eq!(mem.total, 0);
        assert_close(mem.usage_percent, 0.0);
    }

    // =========================================================================
    // parse_disk_info (df -P, 1024-block columns)
    // =========================================================================

    #[test]
    fn disk_info_parses_df_output_and_filters_pseudo_filesystems() {
        let df_output = "\
Filesystem     1024-blocks      Used Available Capacity Mounted on
/dev/sda1         40188160  20151808  20036352      51% /
tmpfs               996880         0    996880       0% /dev/shm
/dev/sdb1        102400000  51200000  51200000      50% /home
overlay           61255492 35719256   22418344      62% /var/lib/docker/overlay2/abc
/dev/sdc1         20480000  10240000  10240000      50% /mnt/backup
";
        let disks = parse_disk_info(df_output);

        // tmpfs on /dev/shm is filtered by mount point; overlay is filtered by
        // filesystem name even though its mount starts with /var.
        assert_eq!(disks.len(), 3);
        assert_eq!(disks[0].mount_point, "/");
        assert_eq!(disks[0].filesystem, "/dev/sda1");
        assert_eq!(disks[0].total, 40188160 * 1024);
        assert_eq!(disks[0].used, 20151808 * 1024);
        assert_eq!(disks[0].available, 20036352 * 1024);
        assert_close(disks[0].usage_percent, 50.1437);
        assert_eq!(disks[1].mount_point, "/home");
        assert_close(disks[1].usage_percent, 50.0);
        assert_eq!(disks[2].mount_point, "/mnt/backup");
        assert_close(disks[2].usage_percent, 50.0);
    }

    #[test]
    fn disk_info_falls_back_to_root_when_strict_filter_matches_nothing() {
        let df_output = "\
Filesystem   1024-blocks     Used Available Capacity Mounted on
bees:/hive     10000000  1000000   9000000      11% /
";
        let disks = parse_disk_info(df_output);

        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].mount_point, "/");
        assert_eq!(disks[0].filesystem, "bees:/hive");
        assert_eq!(disks[0].total, 10000000 * 1024);
        assert_eq!(disks[0].used, 1000000 * 1024);
        assert_eq!(disks[0].available, 9000000 * 1024);
        assert_close(disks[0].usage_percent, 10.0);
    }

    #[test]
    fn disk_info_zero_total_disk_has_zero_usage() {
        let df_output = "\
Filesystem 1024-blocks Used Available Capacity Mounted on
/dev/sdz1           0    0         0       0% /data
";
        let disks = parse_disk_info(df_output);
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].total, 0);
        assert_close(disks[0].usage_percent, 0.0);
    }

    #[test]
    fn disk_info_empty_and_garbage_input_yield_no_disks() {
        assert!(parse_disk_info("").is_empty());
        assert!(
            parse_disk_info("Filesystem 1024-blocks Used Available Capacity Mounted on\n")
                .is_empty()
        );
        // Short / malformed rows must be skipped without panicking.
        assert!(
            parse_disk_info("short row\nanother x\n/dev/onlyfive 1 2 3 4 5 6 7\n\x00\x01\n")
                .is_empty()
        );
    }

    // =========================================================================
    // parse_network_info (/proc/net/dev)
    // =========================================================================

    #[test]
    fn network_info_parses_proc_net_dev_and_skips_loopback() {
        let netdev_output = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 83948944  122490    0    0    0     0          0         0  83948944  122490    0    0    0     0       0          0
  eth0: 1923456789  987654    0    0    0     0          0         0  123456789  654321    0    0    0     0       0          0
 wlan0:    54321     500    0    0    0     0          0         0    12345     400    0    0    0     0       0          0
";
        let interfaces = parse_network_info(netdev_output);

        assert_eq!(interfaces.len(), 2);
        assert_eq!(interfaces[0].interface, "eth0");
        assert_eq!(interfaces[0].rx_bytes, 1923456789);
        assert_eq!(interfaces[0].rx_packets, 987654);
        assert_eq!(interfaces[0].tx_bytes, 123456789);
        assert_eq!(interfaces[0].tx_packets, 654321);
        assert_eq!(interfaces[1].interface, "wlan0");
        assert_eq!(interfaces[1].rx_bytes, 54321);
        assert_eq!(interfaces[1].tx_packets, 400);
    }

    #[test]
    fn network_info_skips_truncated_rows() {
        let netdev_output = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
 veth1: 100 5
  eth0: 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16
";
        let interfaces = parse_network_info(netdev_output);
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].interface, "eth0");
        assert_eq!(interfaces[0].tx_bytes, 9);
        assert_eq!(interfaces[0].tx_packets, 10);
    }

    #[test]
    fn network_info_garbage_does_not_panic() {
        // Rows without a colon are skipped; rows with unparsable counters
        // degrade to zeros.
        let interfaces = parse_network_info("no colon here\n\x00\x01\neth1: a b c d e f g h i j\n");
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].interface, "eth1");
        assert_eq!(interfaces[0].rx_bytes, 0);
        assert!(parse_network_info("").is_empty());
    }

    // =========================================================================
    // parse_uptime (/proc/uptime)
    // =========================================================================

    #[test]
    fn uptime_parses_first_field() {
        assert_eq!(parse_uptime("86160.25 340123.45\n"), 86160);
        assert_eq!(parse_uptime("1234567.89 99.0\n"), 1234567);
    }

    #[test]
    fn uptime_zero_and_garbage_are_zero() {
        assert_eq!(parse_uptime("0.00 0.00\n"), 0);
        assert_eq!(parse_uptime("up a long time\n"), 0);
        assert_eq!(parse_uptime(""), 0);
    }

    // =========================================================================
    // parse_load_average_macos (sysctl -n vm.loadavg)
    // =========================================================================

    #[test]
    fn macos_load_average_strips_braces() {
        let load = parse_load_average_macos("{ 1.20 1.35 1.42 }\n");
        assert_close(load[0], 1.20);
        assert_close(load[1], 1.35);
        assert_close(load[2], 1.42);
    }

    #[test]
    fn macos_load_average_works_without_braces() {
        let load = parse_load_average_macos("3.10 2.20 1.05\n");
        assert_close(load[0], 3.10);
        assert_close(load[1], 2.20);
        assert_close(load[2], 1.05);
    }

    #[test]
    fn macos_load_average_garbage_and_short_input_is_zeroed() {
        assert_eq!(parse_load_average_macos("{ 1.00 2.00 }\n"), [0.0, 0.0, 0.0]);
        assert_eq!(parse_load_average_macos("{ a b c }\n"), [0.0, 0.0, 0.0]);
        assert_eq!(parse_load_average_macos(""), [0.0, 0.0, 0.0]);
    }

    // =========================================================================
    // parse_memory_info_macos (vm_stat + sysctl -n hw.memsize)
    // =========================================================================

    #[test]
    fn macos_memory_info_parses_vm_stat_with_page_size_header() {
        let vm_stat_output = "\
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                              25128.
Pages active:                           833921.
Pages inactive:                         228978.
Pages speculative:                       63886.
Pages throttled:                             0.
Pages wired down:                       432169.
Pages purgeable:                         89520.
\"Translation faults\":                 35972444.
Pages copy-on-write:                  10128734.
Pages zero filled:                    24550301.
Pages reactivated:                      25377.
Pageins:                                68742.
Pageouts:                                  29.
";
        let mem = parse_memory_info_macos(vm_stat_output, "17179869184\n");

        assert_eq!(mem.total, 17179869184);
        // free = 25128 * 16384
        assert_eq!(mem.free, 411697152);
        // available = (25128 + 228978 + 63886) * 16384
        assert_eq!(mem.available, 5209980928);
        assert_eq!(mem.used, 17179869184 - 5209980928);
        assert_close(mem.usage_percent, 69.6739);
        assert_eq!(mem.swap_total, 0);
        assert_eq!(mem.swap_used, 0);
    }

    #[test]
    fn macos_memory_info_defaults_to_4096_page_size_without_header() {
        let vm_stat_output = "\
Pages free:       2048
Pages inactive:   4096
Pages speculative: 512
";
        let mem = parse_memory_info_macos(vm_stat_output, "8192\n");

        assert_eq!(mem.free, 2048 * 4096);
        assert_eq!(mem.available, (2048 + 4096 + 512) * 4096);
        // available exceeds the tiny total; used must saturate at 0.
        assert_eq!(mem.used, 0);
        assert_close(mem.usage_percent, 0.0);
    }

    #[test]
    fn macos_memory_info_garbage_input_is_all_zeros() {
        let mem = parse_memory_info_macos("hello world\nnot vm_stat\n", "not-a-number\n");
        assert_eq!(mem.total, 0);
        assert_eq!(mem.free, 0);
        assert_eq!(mem.available, 0);
        assert_eq!(mem.used, 0);
        assert_close(mem.usage_percent, 0.0);
    }

    // =========================================================================
    // parse_remote_session_state (CLI IPC session state strings)
    // =========================================================================

    #[test]
    fn remote_session_state_maps_known_states() {
        assert_eq!(
            parse_remote_session_state("connected"),
            crate::session::SessionState::Connected
        );
        assert_eq!(
            parse_remote_session_state("disconnected"),
            crate::session::SessionState::Disconnected
        );
        assert_eq!(
            parse_remote_session_state("error"),
            crate::session::SessionState::Error
        );
    }

    #[test]
    fn remote_session_state_defaults_unknown_to_connecting() {
        assert_eq!(
            parse_remote_session_state("running"),
            crate::session::SessionState::Connecting
        );
        assert_eq!(
            parse_remote_session_state(""),
            crate::session::SessionState::Connecting
        );
        assert_eq!(
            parse_remote_session_state("Connected"),
            crate::session::SessionState::Connecting
        );
    }

    // =========================================================================
    // parse_wmic_value / parse_disk_info_windows (Windows WMIC output)
    // =========================================================================

    #[test]
    fn wmic_value_parses_value_output() {
        let output = "\r\nTotalVisibleMemorySize=16648576\r\nFreePhysicalMemory=6904252\r\n";
        assert_eq!(
            parse_wmic_value(output, "FreePhysicalMemory"),
            Some(6904252)
        );
        assert_eq!(
            parse_wmic_value(output, "TotalVisibleMemorySize"),
            Some(16648576)
        );
    }

    #[test]
    fn wmic_value_returns_none_for_missing_or_non_numeric_keys() {
        let output = "FreePhysicalMemory=6904252\n";
        assert_eq!(parse_wmic_value(output, "Missing"), None);
        assert_eq!(
            parse_wmic_value("FreePhysicalMemory=abc\n", "FreePhysicalMemory"),
            None
        );
        assert_eq!(parse_wmic_value("", "FreePhysicalMemory"), None);
    }

    #[test]
    fn windows_disk_info_parses_csv_output() {
        let csv_output = "\
Node,DeviceID,FileSystem,FreeSpace,Size

,C:,NTFS,50191482880,254722007040
,D:,FAT32,1234567,8388608
";
        let disks = parse_disk_info_windows(csv_output);

        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].mount_point, "C:");
        assert_eq!(disks[0].filesystem, "NTFS");
        assert_eq!(disks[0].total, 254722007040);
        assert_eq!(disks[0].available, 50191482880);
        assert_eq!(disks[0].used, 254722007040 - 50191482880);
        assert_close(disks[0].usage_percent, 80.2956);
        assert_eq!(disks[1].mount_point, "D:");
        assert_eq!(disks[1].used, 8388608 - 1234567);
        assert_close(disks[1].usage_percent, 85.2828);
    }

    #[test]
    fn windows_disk_info_skips_headers_and_short_rows() {
        assert!(parse_disk_info_windows("Node,DeviceID,FileSystem,FreeSpace,Size\n\n").is_empty());
        assert!(parse_disk_info_windows(",C:,NTFS\n").is_empty());
        // Zero-size drive must not divide by zero.
        let disks = parse_disk_info_windows(",E:,FAT32,0,0\n");
        assert_eq!(disks.len(), 1);
        assert_close(disks[0].usage_percent, 0.0);
    }

    // =========================================================================
    // encode_output_payload (base64 terminal output bridge)
    // =========================================================================

    #[test]
    fn output_payload_is_base64_encoded() {
        assert_eq!(encode_output_payload(b"hello"), "aGVsbG8=");
        assert_eq!(encode_output_payload(b""), "");
        // Non-UTF8 terminal bytes must survive the bridge unchanged.
        let encoded = encode_output_payload(&[0xff, 0xfe, 0x1b, 0x5b]);
        assert_eq!(encoded, "//4bWw==");
    }
}
