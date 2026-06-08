use anyhow::{Context, Result};
use log::info;
use serde::{Deserialize, Serialize};
#[cfg(not(windows))]
use std::fs;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use interprocess::local_socket::traits::{ListenerExt, Stream as StreamTrait};
#[cfg(not(windows))]
use interprocess::local_socket::{GenericFilePath, ListenerOptions, ToFsName};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, ToNsName};
#[cfg(windows)]
use interprocess::os::windows::local_socket::ListenerOptionsExt as _;
#[cfg(windows)]
use interprocess::os::windows::security_descriptor::{
    AsSecurityDescriptorMutExt as _, SecurityDescriptor,
};

use crate::commands::sftp::{SftpEntry, SftpFileContent};
use crate::session::SessionManager;
use crate::sftp::helpers::{
    resolve_remote_path, resolve_remote_upload_path, sftp_mkdir_recursive, sftp_remove_recursive,
    write_remote_file,
};
use crate::sftp::{
    effective_directory_transfer_options, transfer_directory_to_sftp, DirectoryTransferMode,
    DirectoryTransferSummary, TransferProgress,
};
use crate::storage::{AuthType, Database};

const DEFAULT_SOCKET_NAME: &str = "vibeshell.sock";
const SOCKET_NAME_ENV: &str = "VIBESHELL_IPC_NAME";
const SESSION_IDLE_TTL: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const SESSION_REAPER_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Server metadata returned to CLI for `vshell servers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcServerInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub group_id: Option<String>,
    pub jump_host_id: Option<String>,
    pub tags: Vec<String>,
}

/// Session metadata returned to CLI for `vshell sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcSessionInfo {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    pub state: String,
    pub created_at: i64,
    pub clients: usize,
}

fn auth_type_to_string(auth_type: &AuthType) -> &'static str {
    match auth_type {
        AuthType::Password => "password",
        AuthType::Key => "key",
        AuthType::KeyWithPassphrase => "key_with_passphrase",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcEndpointStatus {
    Reachable,
    Occupied,
    NotRunning,
}

#[derive(Debug)]
pub enum IpcServerRunError {
    ListenerSetup(anyhow::Error),
    ListenerBind(io::Error),
    Runtime(anyhow::Error),
}

impl std::fmt::Display for IpcServerRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ListenerSetup(err) => write!(f, "IPC listener setup failed: {}", err),
            Self::ListenerBind(err) => write!(f, "IPC listener bind failed: {}", err),
            Self::Runtime(err) => write!(f, "IPC runtime failure: {}", err),
        }
    }
}

impl std::error::Error for IpcServerRunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ListenerSetup(err) => Some(err.as_ref()),
            Self::ListenerBind(err) => Some(err),
            Self::Runtime(err) => Some(err.as_ref()),
        }
    }
}

/// IPC messages exchanged between CLI and GUI.
///
/// Messages are serialized as JSON for simplicity and debuggability.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcMessage {
    // Requests from CLI to GUI
    /// List all configured servers
    ListServers,
    /// List all active sessions
    ListSessions,
    /// Create a new session connecting to the specified server
    CreateSession { server_name: String },
    /// Create and connect a new session with explicit credentials
    CreateSessionWithCredentials {
        server_name: String,
        auth_type: String,
        credential: String,
        passphrase: Option<String>,
        cols: Option<u32>,
        rows: Option<u32>,
    },
    /// Attach to an existing session (starts streaming output)
    AttachSession { session_id: String },
    /// Detach from a session (keeps it running)
    DetachSession { session_id: String },
    /// Kill/terminate a session
    KillSession { session_id: String },
    /// Send input data to a session
    SendInput { session_id: String, data: Vec<u8> },
    /// Resize the PTY for a session
    Resize {
        session_id: String,
        cols: u32,
        rows: u32,
    },
    /// Execute a single command on an existing SSH session
    ExecCommand { session_id: String, command: String },
    /// Initialize SFTP context for a session
    SftpInit { session_id: String },
    /// List directory contents
    SftpListDir {
        session_id: String,
        path: String,
        #[serde(default)]
        preserve_cwd: bool,
    },
    /// Download a remote file to a local path
    SftpDownloadFile {
        session_id: String,
        remote_path: String,
        local_path: String,
    },
    /// Upload a local file to a remote path
    SftpUploadFile {
        session_id: String,
        local_path: String,
        remote_path: String,
    },
    /// Upload or sync a local directory to a remote directory
    SftpUploadDirectory {
        session_id: String,
        local_path: String,
        remote_path: String,
        mode: DirectoryTransferMode,
        #[serde(default)]
        delete_extra: bool,
        #[serde(default)]
        respect_gitignore: Option<bool>,
        #[serde(default)]
        excluded_paths: Vec<String>,
    },
    /// Create a remote directory
    SftpMkdir { session_id: String, path: String },
    /// Delete a remote file or directory
    SftpDelete {
        session_id: String,
        path: String,
        recursive: bool,
    },
    /// Rename or move a remote file or directory
    SftpRename {
        session_id: String,
        old_path: String,
        new_path: String,
    },
    /// Return the current SFTP working directory
    SftpPwd { session_id: String },
    /// Stat a remote path
    SftpStat { session_id: String, path: String },
    /// Read a remote file for preview
    SftpReadFile {
        session_id: String,
        path: String,
        max_size: Option<u64>,
        as_binary: Option<bool>,
    },
    /// Write text content to a remote file
    SftpWriteFile {
        session_id: String,
        path: String,
        content: String,
    },

    // Responses from GUI to CLI
    /// List of configured servers
    ServerList { servers: Vec<IpcServerInfo> },
    /// List of active session IDs
    SessionList { sessions: Vec<IpcSessionInfo> },
    /// A new session was created
    SessionCreated { session_id: String },
    /// Output data from a session (used in streaming mode)
    SessionOutput { session_id: String, data: Vec<u8> },
    /// Session has ended (sent during streaming attach)
    SessionEnded { reason: String },
    /// Output for a single remote command
    CommandOutput { output: String },
    /// SFTP directory entries
    SftpEntries { entries: Vec<SftpEntry> },
    /// SFTP path response
    SftpPath { path: String },
    /// SFTP stat response
    SftpStatResult { entry: SftpEntry },
    /// SFTP file preview response
    SftpFileContent { content: SftpFileContent },
    /// SFTP transfer response
    SftpTransfer { progress: TransferProgress },
    /// SFTP directory transfer response
    SftpDirectoryTransfer { summary: DirectoryTransferSummary },
    /// Error response
    Error { message: String },
    /// Success acknowledgment
    Ok,
}

/// Socket name type alias for platform-specific implementation.
#[cfg(windows)]
type SocketName = interprocess::local_socket::Name<'static>;
#[cfg(not(windows))]
type SocketName = interprocess::local_socket::Name<'static>;

/// Get the platform-specific socket name/path.
///
/// On Windows, we use named pipes in the namespaced format.
/// On Unix, we use Unix domain sockets in `/tmp/`.
fn get_socket_name() -> Result<SocketName> {
    let socket_name = socket_name_base();

    #[cfg(windows)]
    {
        // On Windows, use namespaced name (named pipe)
        socket_name
            .to_ns_name::<GenericNamespaced>()
            .context("Failed to create namespaced socket name")
    }
    #[cfg(not(windows))]
    {
        // On Unix, use a socket file in /tmp
        let path = format!("/tmp/{}", socket_name);
        path.to_fs_name::<GenericFilePath>()
            .context("Failed to create filesystem socket name")
    }
}

fn socket_name_base() -> String {
    std::env::var(SOCKET_NAME_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SOCKET_NAME.to_string())
}

fn socket_name_display() -> String {
    #[cfg(windows)]
    {
        format!("\\\\.\\pipe\\{}", socket_name_base())
    }
    #[cfg(not(windows))]
    {
        format!("/tmp/{}", socket_name_base())
    }
}

#[cfg(windows)]
fn is_recoverable_listener_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::WouldBlock
    ) || matches!(err.raw_os_error(), Some(109 | 232 | 233 | 234))
}

#[cfg(not(windows))]
fn is_recoverable_listener_error(_err: &io::Error) -> bool {
    false
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaleSocketCleanup {
    Removed,
    BecameReachable,
    NotRemoved,
}

#[cfg(not(windows))]
fn is_stale_socket_bind_error(bind_kind: io::ErrorKind, endpoint_exists: bool) -> bool {
    endpoint_exists && matches!(bind_kind, io::ErrorKind::AddrInUse)
}

#[cfg(not(windows))]
fn cleanup_stale_socket_file(
    bind_kind: io::ErrorKind,
    endpoint_display: &str,
    endpoint_exists: bool,
) -> StaleSocketCleanup {
    if !is_stale_socket_bind_error(bind_kind, endpoint_exists) {
        return StaleSocketCleanup::NotRemoved;
    }

    let socket_name = match get_socket_name() {
        Ok(socket_name) => socket_name,
        Err(err) => {
            log::warn!(
                "[IPC] Could not re-check stale endpoint {} before cleanup: {}",
                endpoint_display,
                err
            );
            return StaleSocketCleanup::NotRemoved;
        }
    };

    match interprocess::local_socket::Stream::connect(socket_name) {
        Ok(_) => StaleSocketCleanup::BecameReachable,
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            match fs::remove_file(endpoint_display) {
                Ok(()) => {
                    log::warn!(
                        "[IPC] Removed stale IPC socket file at {} after failed reachability probe",
                        endpoint_display
                    );
                    StaleSocketCleanup::Removed
                }
                Err(remove_err) if remove_err.kind() == io::ErrorKind::NotFound => {
                    StaleSocketCleanup::Removed
                }
                Err(remove_err) => {
                    log::warn!(
                        "[IPC] Could not remove stale IPC socket file at {}: {}",
                        endpoint_display,
                        remove_err
                    );
                    StaleSocketCleanup::NotRemoved
                }
            }
        }
        Err(err) => {
            log::debug!(
                "[IPC] Stale endpoint re-check on {} failed with {:?}; leaving socket file in place",
                endpoint_display,
                err.kind()
            );
            StaleSocketCleanup::NotRemoved
        }
    }
}

fn listener_options(socket_name: SocketName) -> Result<ListenerOptions<'static>> {
    #[cfg(windows)]
    {
        let mut security_descriptor =
            SecurityDescriptor::new().context("Failed to create IPC security descriptor")?;
        unsafe {
            security_descriptor
                .borrow_mut()
                .set_dacl(std::ptr::null_mut(), false)
                .context("Failed to configure IPC security descriptor")?;
        }

        Ok(ListenerOptions::new()
            .name(socket_name)
            .security_descriptor(security_descriptor))
    }
    #[cfg(not(windows))]
    {
        Ok(ListenerOptions::new().name(socket_name))
    }
}

/// IPC server that runs in the GUI application.
///
/// The GUI app starts this server on launch to accept connections
/// from CLI instances that want to interact with sessions.
///
/// Owns a persistent tokio runtime so that async tasks spawned by
/// IPC handlers (e.g. SSH I/O bridge tasks in `create_with_credentials`)
/// survive beyond the lifetime of a single IPC request.
pub struct IpcServer {
    database: Arc<Database>,
    session_manager: Arc<SessionManager>,
    sftp_contexts: Arc<Mutex<std::collections::HashMap<String, SftpContext>>>,
    /// A long-lived tokio runtime handle shared across all IPC connections.
    /// Tasks spawned via this handle persist until the IPC server shuts down.
    rt_handle: tokio::runtime::Handle,
    /// Owned runtime kept alive for the server's lifetime.
    _runtime: tokio::runtime::Runtime,
}

#[derive(Debug, Clone)]
struct SftpContext {
    home_dir: String,
    current_path: String,
}

impl IpcServer {
    /// Create a new IPC server instance.
    pub fn new(database: Arc<Database>, session_manager: Arc<SessionManager>) -> Self {
        let runtime =
            tokio::runtime::Runtime::new().expect("Failed to create tokio runtime for IPC server");
        let rt_handle = runtime.handle().clone();
        let sftp_contexts = Arc::new(Mutex::new(std::collections::HashMap::new()));

        {
            let session_manager = session_manager.clone();
            let sftp_contexts = sftp_contexts.clone();
            rt_handle.spawn(async move {
                let mut interval = tokio::time::interval(SESSION_REAPER_INTERVAL);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

                loop {
                    interval.tick().await;
                    match session_manager
                        .reap_inactive_sessions(SESSION_IDLE_TTL)
                        .await
                    {
                        Ok(reaped_ids) => {
                            if reaped_ids.is_empty() {
                                continue;
                            }

                            let mut contexts = sftp_contexts.lock().unwrap();
                            for session_id in reaped_ids {
                                contexts.remove(&session_id);
                            }
                        }
                        Err(err) => {
                            log::warn!("[IPC] Session reaper failed: {}", err);
                        }
                    }
                }
            });
        }

        Self {
            database,
            session_manager,
            sftp_contexts,
            rt_handle,
            _runtime: runtime,
        }
    }

    /// Get a human-readable description of the socket name for this server.
    #[allow(dead_code)]
    pub fn socket_name_display() -> String {
        socket_name_display()
    }

    /// Start the IPC server and listen for connections.
    /// This should be run in a separate thread.
    pub fn run(&self) -> std::result::Result<(), IpcServerRunError> {
        let socket_name = get_socket_name().map_err(IpcServerRunError::ListenerSetup)?;
        let options = listener_options(socket_name).map_err(IpcServerRunError::ListenerSetup)?;

        log::debug!("[IPC] Creating listener on {}", Self::socket_name_display());

        // Create listener with options
        let listener = match options.create_sync() {
            Ok(l) => l,
            Err(e) => {
                #[cfg(not(windows))]
                {
                    let endpoint_display = Self::socket_name_display();
                    match cleanup_stale_socket_file(
                        e.kind(),
                        &endpoint_display,
                        Path::new(&endpoint_display).exists(),
                    ) {
                        StaleSocketCleanup::Removed => {
                            let socket_name =
                                get_socket_name().map_err(IpcServerRunError::ListenerSetup)?;
                            let options = listener_options(socket_name)
                                .map_err(IpcServerRunError::ListenerSetup)?;
                            match options.create_sync() {
                                Ok(listener) => listener,
                                Err(retry_err) => {
                                    log::error!(
                                        "[IPC] Listener creation failed on {} after stale socket cleanup: {:?}",
                                        Self::socket_name_display(),
                                        retry_err
                                    );
                                    return Err(IpcServerRunError::ListenerBind(retry_err));
                                }
                            }
                        }
                        StaleSocketCleanup::BecameReachable => {
                            return Err(IpcServerRunError::ListenerBind(e));
                        }
                        StaleSocketCleanup::NotRemoved => {
                            log::error!(
                                "[IPC] Listener creation failed on {}: {:?}",
                                Self::socket_name_display(),
                                e
                            );
                            return Err(IpcServerRunError::ListenerBind(e));
                        }
                    }
                }
                #[cfg(windows)]
                {
                    log::error!(
                        "[IPC] Listener creation failed on {}: {:?}",
                        Self::socket_name_display(),
                        e
                    );
                    return Err(IpcServerRunError::ListenerBind(e));
                }
            }
        };

        log::info!("[IPC] Server listening on {}", Self::socket_name_display());

        // Accept connections in a loop
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let db = self.database.clone();
                    let sm = self.session_manager.clone();
                    let sftp_contexts = self.sftp_contexts.clone();
                    let rt = self.rt_handle.clone();

                    // Handle each connection in a thread
                    std::thread::spawn(move || {
                        if let Err(e) = Self::handle_connection(stream, db, sm, sftp_contexts, rt) {
                            log::error!("[IPC] Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    if is_recoverable_listener_error(&e) {
                        log::warn!(
                            "[IPC] Recoverable accept error on {}: {}",
                            Self::socket_name_display(),
                            e
                        );
                        continue;
                    }

                    log::error!(
                        "[IPC] Accept error on {}: {}",
                        Self::socket_name_display(),
                        e
                    );
                    return Err(IpcServerRunError::Runtime(anyhow::anyhow!(
                        "IPC accept loop aborted on {}: {}",
                        Self::socket_name_display(),
                        e
                    )));
                }
            }
        }

        Ok(())
    }

    /// Handle a single IPC connection
    fn handle_connection(
        stream: interprocess::local_socket::Stream,
        database: Arc<Database>,
        session_manager: Arc<SessionManager>,
        sftp_contexts: Arc<Mutex<std::collections::HashMap<String, SftpContext>>>,
        rt_handle: tokio::runtime::Handle,
    ) -> Result<()> {
        let mut reader = BufReader::new(&stream);
        let mut writer = &stream;

        // Read the request
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .context("Failed to read IPC message")?;

        let message: IpcMessage =
            serde_json::from_str(line.trim()).context("Failed to parse IPC message")?;

        log::debug!("[IPC] Received: {:?}", message);

        // Check for streaming attach — handled specially (keeps connection alive)
        if let IpcMessage::AttachSession { ref session_id } = message {
            let sid = session_id.clone();
            // Release borrows on `stream` so it can be moved into handle_streaming_attach
            drop(reader);
            let _ = writer;
            return Self::handle_streaming_attach(stream, sid, session_manager, &rt_handle);
        }

        // Handle normal request-response
        let response = Self::handle_message(
            message,
            database,
            session_manager,
            sftp_contexts,
            &rt_handle,
        );

        // Send response
        let mut json = serde_json::to_string(&response).context("Failed to serialize response")?;
        json.push('\n');
        writer
            .write_all(json.as_bytes())
            .context("Failed to send response")?;
        writer.flush()?;

        Ok(())
    }

    /// Handle a streaming attach session.
    ///
    /// After verifying the session exists, the server streams `SessionOutput`
    /// messages over the persistent connection until the session ends or the
    /// CLI disconnects. Input is sent by the CLI via *separate* one-shot
    /// IPC connections using `SendInput`.
    fn handle_streaming_attach(
        stream: interprocess::local_socket::Stream,
        session_id: String,
        session_manager: Arc<SessionManager>,
        rt_handle: &tokio::runtime::Handle,
    ) -> Result<()> {
        let session = rt_handle
            .block_on(session_manager.get(&session_id))
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
        rt_handle.block_on(session.attach());

        // Send initial Ok acknowledgment
        let mut writer = &stream;
        let mut ok_json = serde_json::to_string(&IpcMessage::Ok)?;
        ok_json.push('\n');
        writer.write_all(ok_json.as_bytes())?;
        writer.flush()?;

        log::info!("[IPC] Streaming attach started for session {}", session_id);

        for data in rt_handle.block_on(session.replay_output()) {
            let msg = IpcMessage::SessionOutput {
                session_id: session_id.clone(),
                data,
            };
            let mut json = serde_json::to_string(&msg)?;
            json.push('\n');
            if writer.write_all(json.as_bytes()).is_err() || writer.flush().is_err() {
                rt_handle.block_on(session.detach());
                return Ok(());
            }
        }

        // Subscribe to session output
        let mut receiver = session.subscribe();

        // Stream output to the CLI until the session ends or the CLI disconnects
        loop {
            match rt_handle.block_on(receiver.recv()) {
                std::result::Result::Ok(data) => {
                    let msg = IpcMessage::SessionOutput {
                        session_id: session_id.clone(),
                        data,
                    };
                    let mut json = match serde_json::to_string(&msg) {
                        std::result::Result::Ok(j) => j,
                        Err(e) => {
                            log::error!("[IPC] Failed to serialize output: {}", e);
                            break;
                        }
                    };
                    json.push('\n');
                    if writer.write_all(json.as_bytes()).is_err() {
                        // CLI disconnected
                        log::info!(
                            "[IPC] CLI disconnected from streaming session {}",
                            session_id
                        );
                        break;
                    }
                    if writer.flush().is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    // Session ended — notify CLI
                    let end_msg = IpcMessage::SessionEnded {
                        reason: "Session closed".to_string(),
                    };
                    if let std::result::Result::Ok(mut json) = serde_json::to_string(&end_msg) {
                        json.push('\n');
                        let _ = writer.write_all(json.as_bytes());
                        let _ = writer.flush();
                    }
                    log::info!("[IPC] Session {} ended, closing stream", session_id);
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("[IPC] Streaming lagged by {} messages", n);
                    // Continue receiving
                }
            }
        }

        rt_handle.block_on(session.detach());

        Ok(())
    }

    fn get_sftp_context(
        sftp_contexts: &Arc<Mutex<std::collections::HashMap<String, SftpContext>>>,
        session_id: &str,
    ) -> std::result::Result<SftpContext, String> {
        sftp_contexts
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("SFTP not initialized for session {}", session_id))
    }

    fn set_sftp_context(
        sftp_contexts: &Arc<Mutex<std::collections::HashMap<String, SftpContext>>>,
        session_id: &str,
        context: SftpContext,
    ) {
        sftp_contexts
            .lock()
            .unwrap()
            .insert(session_id.to_string(), context);
    }

    fn clear_sftp_context(
        sftp_contexts: &Arc<Mutex<std::collections::HashMap<String, SftpContext>>>,
        session_id: &str,
    ) {
        sftp_contexts.lock().unwrap().remove(session_id);
    }

    /// Handle an IPC message and return a response
    fn handle_message(
        message: IpcMessage,
        database: Arc<Database>,
        session_manager: Arc<SessionManager>,
        sftp_contexts: Arc<Mutex<std::collections::HashMap<String, SftpContext>>>,
        rt: &tokio::runtime::Handle,
    ) -> IpcMessage {
        match message {
            IpcMessage::ListServers => match database.server_list(None, None) {
                std::result::Result::Ok(servers) => {
                    let servers = servers
                        .into_iter()
                        .map(|server| IpcServerInfo {
                            id: server.id,
                            name: server.name,
                            host: server.host,
                            port: server.port,
                            username: server.username,
                            auth_type: auth_type_to_string(&server.auth_type).to_string(),
                            group_id: server.group_id,
                            jump_host_id: server.jump_host_id,
                            tags: server.tags,
                        })
                        .collect();

                    IpcMessage::ServerList { servers }
                }
                Err(e) => IpcMessage::Error {
                    message: format!("Failed to list servers: {}", e),
                },
            },
            IpcMessage::ListSessions => {
                let sessions = rt.block_on(async {
                    session_manager
                        .list()
                        .await
                        .into_iter()
                        .map(|s| IpcSessionInfo {
                            id: s.id.clone(),
                            server_id: s.server_id.clone(),
                            server_name: s.server_name.clone(),
                            state: format!("{:?}", s.state).to_lowercase(),
                            created_at: s.created_at,
                            clients: s.clients,
                        })
                        .collect::<Vec<_>>()
                });
                IpcMessage::SessionList { sessions }
            }
            IpcMessage::CreateSession { server_name } => {
                // Look up saved credentials for the server and connect
                match database.credential_get(&server_name) {
                    std::result::Result::Ok(Some(cred)) => {
                        let ssh_cred = match cred.auth_type.as_str() {
                            "password" => crate::session::SshCredential::Password(cred.credential),
                            "key" | "key_with_passphrase" => {
                                crate::session::SshCredential::PrivateKey {
                                    key: cred.credential,
                                    passphrase: cred.passphrase,
                                }
                            }
                            other => {
                                return IpcMessage::Error {
                                    message: format!(
                                        "Unknown auth type '{}' for server '{}'",
                                        other, server_name
                                    ),
                                };
                            }
                        };

                        let pty_config = Some(crate::ssh::PtyConfig {
                            term: "xterm-256color".to_string(),
                            cols: 80,
                            rows: 24,
                            pix_width: 0,
                            pix_height: 0,
                        });

                        match rt.block_on(session_manager.create_with_credentials(
                            &server_name,
                            ssh_cred,
                            pty_config,
                        )) {
                            std::result::Result::Ok(session) => IpcMessage::SessionCreated {
                                session_id: session.id.clone(),
                            },
                            Err(e) => IpcMessage::Error {
                                message: format!("Failed to connect to '{}': {}", server_name, e),
                            },
                        }
                    }
                    std::result::Result::Ok(None) => {
                        match rt.block_on(session_manager.create_by_name(&server_name)) {
                            std::result::Result::Ok(session) => IpcMessage::SessionCreated {
                                session_id: session.id.clone(),
                            },
                            Err(e) => IpcMessage::Error {
                                message: format!(
                                    "No saved credentials for server '{}'. \
                                     Please save credentials in the VibeShell GUI first, \
                                     or connect through the GUI. ({})",
                                    server_name, e
                                ),
                            },
                        }
                    }
                    Err(e) => IpcMessage::Error {
                        message: format!(
                            "Failed to look up credentials for '{}': {}",
                            server_name, e
                        ),
                    },
                }
            }
            IpcMessage::CreateSessionWithCredentials {
                server_name,
                auth_type,
                credential,
                passphrase,
                cols,
                rows,
            } => {
                let ssh_cred = match auth_type.as_str() {
                    "password" => crate::session::SshCredential::Password(credential),
                    "key" | "key_with_passphrase" => crate::session::SshCredential::PrivateKey {
                        key: credential,
                        passphrase,
                    },
                    other => {
                        return IpcMessage::Error {
                            message: format!(
                                "Unknown auth type '{}' for server '{}'",
                                other, server_name
                            ),
                        };
                    }
                };

                let pty_config = Some(crate::ssh::PtyConfig {
                    term: "xterm-256color".to_string(),
                    cols: cols.unwrap_or(80),
                    rows: rows.unwrap_or(24),
                    pix_width: 0,
                    pix_height: 0,
                });

                match rt.block_on(session_manager.create_with_credentials(
                    &server_name,
                    ssh_cred,
                    pty_config,
                )) {
                    std::result::Result::Ok(session) => IpcMessage::SessionCreated {
                        session_id: session.id.clone(),
                    },
                    Err(e) => IpcMessage::Error {
                        message: format!("Failed to connect to '{}': {}", server_name, e),
                    },
                }
            }
            IpcMessage::KillSession { session_id } => {
                Self::clear_sftp_context(&sftp_contexts, &session_id);
                match rt.block_on(session_manager.kill(&session_id)) {
                    std::result::Result::Ok(_) => IpcMessage::Ok,
                    Err(e) => IpcMessage::Error {
                        message: format!("Failed to kill session: {}", e),
                    },
                }
            }
            IpcMessage::DetachSession { session_id } => {
                match rt.block_on(session_manager.get(&session_id)) {
                    Some(session) => {
                        rt.block_on(session.detach());
                        IpcMessage::Ok
                    }
                    None => IpcMessage::Error {
                        message: format!("Session not found: {}", session_id),
                    },
                }
            }
            IpcMessage::SendInput { session_id, data } => {
                log::debug!("[IPC] SendInput to {}: {} bytes", session_id, data.len());
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    session
                        .write_to_ssh(&data)
                        .await
                        .map_err(|e| format!("Failed to send input: {}", e))
                }) {
                    std::result::Result::Ok(_) => IpcMessage::Ok,
                    Err(msg) => IpcMessage::Error { message: msg },
                }
            }
            IpcMessage::Resize {
                session_id,
                cols,
                rows,
            } => {
                log::debug!("[IPC] Resize {} to {}x{}", session_id, cols, rows);
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    session
                        .resize_pty(cols, rows)
                        .await
                        .map_err(|e| format!("Failed to resize: {}", e))
                }) {
                    std::result::Result::Ok(_) => IpcMessage::Ok,
                    Err(msg) => IpcMessage::Error { message: msg },
                }
            }
            IpcMessage::ExecCommand {
                session_id,
                command,
            } => {
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    session
                        .exec_command(&command)
                        .await
                        .map_err(|e| format!("Failed to execute command: {}", e))
                }) {
                    std::result::Result::Ok(output) => IpcMessage::CommandOutput { output },
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpInit { session_id } => {
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    let home_dir = sftp
                        .canonicalize(".")
                        .await
                        .map_err(|e| format!("Failed to resolve home directory: {}", e))?;
                    Ok::<String, String>(home_dir)
                }) {
                    std::result::Result::Ok(home_dir) => {
                        Self::set_sftp_context(
                            &sftp_contexts,
                            &session_id,
                            SftpContext {
                                home_dir: home_dir.clone(),
                                current_path: home_dir.clone(),
                            },
                        );
                        info!("[IPC] Initialized SFTP context for {}", session_id);
                        IpcMessage::Ok
                    }
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpListDir {
                session_id,
                path,
                preserve_cwd,
            } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let resolved = resolve_remote_path(&path, &context.home_dir, &context.current_path);

                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    let dir_entries = sftp
                        .read_dir(&resolved)
                        .await
                        .map_err(|e| format!("Failed to list directory {}: {}", resolved, e))?;

                    let mut entries = Vec::new();
                    for entry in dir_entries {
                        let name = entry.file_name();
                        if name == "." || name == ".." {
                            continue;
                        }
                        let file_type = entry.file_type();
                        let is_directory = file_type.is_dir();
                        let metadata = entry.metadata();
                        let size = if is_directory { 0 } else { metadata.len() };
                        let modified_at = metadata
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        let permissions = format!(
                            "{}{}",
                            if is_directory { "d" } else { "-" },
                            metadata.permissions()
                        );
                        let entry_path = if resolved.ends_with('/') {
                            format!("{}{}", resolved, name)
                        } else {
                            format!("{}/{}", resolved, name)
                        };
                        entries.push(SftpEntry {
                            name,
                            path: entry_path,
                            is_directory,
                            size,
                            modified_at,
                            permissions,
                        });
                    }

                    entries.sort_by(|a, b| {
                        b.is_directory
                            .cmp(&a.is_directory)
                            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    });

                    Ok::<Vec<SftpEntry>, String>(entries)
                }) {
                    std::result::Result::Ok(entries) => {
                        if preserve_cwd {
                            return IpcMessage::SftpEntries { entries };
                        }

                        Self::set_sftp_context(
                            &sftp_contexts,
                            &session_id,
                            SftpContext {
                                home_dir: context.home_dir,
                                current_path: resolved,
                            },
                        );
                        IpcMessage::SftpEntries { entries }
                    }
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpPwd { session_id } => {
                match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => IpcMessage::SftpPath {
                        path: context.current_path,
                    },
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpStat { session_id, path } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let resolved = resolve_remote_path(&path, &context.home_dir, &context.current_path);
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    let meta = sftp
                        .metadata(&resolved)
                        .await
                        .map_err(|e| format!("Failed to stat {}: {}", resolved, e))?;
                    let is_directory = meta.is_dir();
                    let modified_at = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let name = Path::new(&resolved)
                        .file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or(&resolved)
                        .to_string();
                    Ok::<SftpEntry, String>(SftpEntry {
                        name,
                        path: resolved.clone(),
                        is_directory,
                        size: if is_directory { 0 } else { meta.len() },
                        modified_at,
                        permissions: format!(
                            "{}{}",
                            if is_directory { "d" } else { "-" },
                            meta.permissions()
                        ),
                    })
                }) {
                    std::result::Result::Ok(entry) => IpcMessage::SftpStatResult { entry },
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpReadFile {
                session_id,
                path,
                max_size,
                as_binary,
            } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let resolved = resolve_remote_path(&path, &context.home_dir, &context.current_path);
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    let metadata = sftp
                        .metadata(&resolved)
                        .await
                        .map_err(|e| format!("Failed to stat {}: {}", resolved, e))?;
                    if metadata.is_dir() {
                        return Err(format!("Cannot read directory as file: {}", resolved));
                    }
                    let binary = as_binary.unwrap_or(false);
                    let max_size = max_size.unwrap_or(if binary {
                        10 * 1024 * 1024
                    } else {
                        1024 * 1024
                    });
                    let file_size = metadata.len();
                    if file_size > max_size && binary {
                        return Err(format!(
                            "File too large for preview: {} bytes (max: {} bytes)",
                            file_size, max_size
                        ));
                    }
                    let bytes = sftp
                        .read(&resolved)
                        .await
                        .map_err(|e| format!("Failed to read file {}: {}", resolved, e))?;
                    let (content, truncated) = if binary {
                        (
                            base64::Engine::encode(
                                &base64::engine::general_purpose::STANDARD,
                                &bytes,
                            ),
                            false,
                        )
                    } else {
                        let read_size = std::cmp::min(bytes.len(), max_size as usize);
                        (
                            String::from_utf8_lossy(&bytes[..read_size]).to_string(),
                            read_size < bytes.len(),
                        )
                    };
                    Ok::<SftpFileContent, String>(SftpFileContent {
                        content,
                        is_binary: binary,
                        size: file_size,
                        truncated,
                        mime_type: mime_type(&resolved),
                    })
                }) {
                    std::result::Result::Ok(content) => IpcMessage::SftpFileContent { content },
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpWriteFile {
                session_id,
                path,
                content,
            } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let resolved = resolve_remote_path(&path, &context.home_dir, &context.current_path);
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    write_remote_file(&sftp, &resolved, content.as_bytes()).await
                }) {
                    std::result::Result::Ok(_) => IpcMessage::Ok,
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpDownloadFile {
                session_id,
                remote_path,
                local_path,
            } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let resolved =
                    resolve_remote_path(&remote_path, &context.home_dir, &context.current_path);
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    let content = sftp
                        .read(&resolved)
                        .await
                        .map_err(|e| format!("Failed to read remote file {}: {}", resolved, e))?;
                    if let Some(parent) = Path::new(&local_path).parent() {
                        if !parent.as_os_str().is_empty() {
                            std::fs::create_dir_all(parent).map_err(|e| {
                                format!(
                                    "Failed to create parent directory {}: {}",
                                    parent.display(),
                                    e
                                )
                            })?;
                        }
                    }
                    std::fs::write(&local_path, &content)
                        .map_err(|e| format!("Failed to write local file {}: {}", local_path, e))?;

                    let filename = Path::new(&resolved)
                        .file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let mut progress = TransferProgress::new(filename, content.len() as u64);
                    progress.transferred_bytes = content.len() as u64;
                    progress.status = crate::sftp::TransferStatus::Completed;
                    Ok::<TransferProgress, String>(progress)
                }) {
                    std::result::Result::Ok(progress) => IpcMessage::SftpTransfer { progress },
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpUploadFile {
                session_id,
                local_path,
                remote_path,
            } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let resolved =
                    resolve_remote_path(&remote_path, &context.home_dir, &context.current_path);
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    let content = std::fs::read(&local_path)
                        .map_err(|e| format!("Failed to read local file {}: {}", local_path, e))?;
                    let filename = Path::new(&local_path)
                        .file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let resolved = resolve_remote_upload_path(&sftp, &resolved, &filename).await;
                    write_remote_file(&sftp, &resolved, &content).await?;

                    let mut progress = TransferProgress::new(filename, content.len() as u64);
                    progress.transferred_bytes = content.len() as u64;
                    progress.status = crate::sftp::TransferStatus::Completed;
                    Ok::<TransferProgress, String>(progress)
                }) {
                    std::result::Result::Ok(progress) => IpcMessage::SftpTransfer { progress },
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpUploadDirectory {
                session_id,
                local_path,
                remote_path,
                mode,
                delete_extra,
                respect_gitignore,
                excluded_paths,
            } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let resolved =
                    resolve_remote_path(&remote_path, &context.home_dir, &context.current_path);
                let options = effective_directory_transfer_options(
                    Some(excluded_paths),
                    respect_gitignore,
                    delete_extra,
                );

                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    transfer_directory_to_sftp(
                        &sftp,
                        &PathBuf::from(local_path),
                        &resolved,
                        mode,
                        &options,
                    )
                    .await
                }) {
                    std::result::Result::Ok(summary) => {
                        IpcMessage::SftpDirectoryTransfer { summary }
                    }
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpMkdir { session_id, path } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let resolved = resolve_remote_path(&path, &context.home_dir, &context.current_path);
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    sftp_mkdir_recursive(&sftp, &resolved).await
                }) {
                    std::result::Result::Ok(_) => IpcMessage::Ok,
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpDelete {
                session_id,
                path,
                recursive,
            } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let resolved = resolve_remote_path(&path, &context.home_dir, &context.current_path);
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    let meta = sftp
                        .metadata(&resolved)
                        .await
                        .map_err(|e| format!("Failed to stat {}: {}", resolved, e))?;
                    if meta.is_dir() {
                        if recursive {
                            sftp_remove_recursive(&sftp, &resolved, 0).await
                        } else {
                            sftp.remove_dir(&resolved).await.map_err(|e| {
                                format!("Failed to remove directory {}: {}", resolved, e)
                            })
                        }
                    } else {
                        sftp.remove_file(&resolved)
                            .await
                            .map_err(|e| format!("Failed to remove file {}: {}", resolved, e))
                    }
                }) {
                    std::result::Result::Ok(_) => IpcMessage::Ok,
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::SftpRename {
                session_id,
                old_path,
                new_path,
            } => {
                let context = match Self::get_sftp_context(&sftp_contexts, &session_id) {
                    Ok(context) => context,
                    Err(message) => return IpcMessage::Error { message },
                };
                let old_resolved =
                    resolve_remote_path(&old_path, &context.home_dir, &context.current_path);
                let new_resolved =
                    resolve_remote_path(&new_path, &context.home_dir, &context.current_path);
                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    let sftp = session
                        .open_sftp_session()
                        .await
                        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
                    sftp.rename(&old_resolved, &new_resolved)
                        .await
                        .map_err(|e| {
                            format!(
                                "Failed to rename {} to {}: {}",
                                old_resolved, new_resolved, e
                            )
                        })
                }) {
                    std::result::Result::Ok(_) => IpcMessage::Ok,
                    Err(message) => IpcMessage::Error { message },
                }
            }
            // AttachSession is handled in handle_connection before reaching here
            _ => IpcMessage::Error {
                message: "Unexpected message type".to_string(),
            },
        }
    }
}

fn mime_type(path: &str) -> String {
    let ext = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "txt" | "md" | "rs" | "ts" | "tsx" | "js" | "json" | "toml" | "yaml" | "yml" => {
            "text/plain"
        }
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// IPC client used by the CLI to communicate with the GUI.
///
/// The CLI uses this client to send commands to the GUI server
/// and receive responses about session operations.
pub struct IpcClient;

impl IpcClient {
    fn connect_error_message() -> String {
        format!(
            "Failed to connect to the VibeShell IPC service on {}",
            socket_name_display()
        )
    }

    fn connect_stream() -> Result<interprocess::local_socket::Stream> {
        use interprocess::local_socket::Stream;

        const RETRY_DELAYS_MS: [u64; 3] = [0, 100, 250];
        let mut last_error: Option<anyhow::Error> = None;

        for delay_ms in RETRY_DELAYS_MS {
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }

            let socket_name = get_socket_name()?;
            match Stream::connect(socket_name) {
                Ok(stream) => return Ok(stream),
                Err(err) => {
                    last_error = Some(anyhow::Error::new(err));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!(Self::connect_error_message())))
            .with_context(Self::connect_error_message)
    }

    #[cfg_attr(windows, allow(dead_code))]
    fn classify_non_windows_probe(
        connect_kind: io::ErrorKind,
        bind_outcome: std::result::Result<(), io::ErrorKind>,
        endpoint_exists: bool,
    ) -> IpcEndpointStatus {
        match bind_outcome {
            Ok(()) => IpcEndpointStatus::NotRunning,
            Err(io::ErrorKind::AddrInUse | io::ErrorKind::PermissionDenied) => {
                IpcEndpointStatus::Occupied
            }
            Err(_) => {
                if endpoint_exists
                    || !matches!(
                        connect_kind,
                        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                    )
                {
                    IpcEndpointStatus::Occupied
                } else {
                    IpcEndpointStatus::NotRunning
                }
            }
        }
    }

    fn probe_endpoint() -> Result<IpcEndpointStatus> {
        use interprocess::local_socket::Stream;

        let socket_name = get_socket_name()?;

        match Stream::connect(socket_name) {
            Ok(_) => Ok(IpcEndpointStatus::Reachable),
            Err(connect_err) => {
                #[cfg(windows)]
                {
                    let bind_check = get_socket_name()
                        .context("Failed to create namespaced socket name for probe")?;
                    match ListenerOptions::new().name(bind_check).create_sync() {
                        Ok(listener) => {
                            drop(listener);
                            Ok(IpcEndpointStatus::NotRunning)
                        }
                        Err(bind_err)
                            if matches!(
                                bind_err.kind(),
                                io::ErrorKind::PermissionDenied | io::ErrorKind::AddrInUse
                            ) =>
                        {
                            log::warn!(
                                "[IPC] Endpoint probe connect failed with {:?}, bind check failed with {:?} on {}",
                                connect_err.kind(),
                                bind_err.kind(),
                                socket_name_display()
                            );
                            Ok(IpcEndpointStatus::Occupied)
                        }
                        Err(bind_err) => {
                            log::debug!(
                                "[IPC] Endpoint probe connect failed with {:?}, bind check failed with {:?} on {}",
                                connect_err.kind(),
                                bind_err.kind(),
                                socket_name_display()
                            );
                            Ok(IpcEndpointStatus::NotRunning)
                        }
                    }
                }
                #[cfg(not(windows))]
                {
                    let endpoint_display = socket_name_display();
                    let endpoint_exists = Path::new(&endpoint_display).exists();

                    let mut bind_outcome = match get_socket_name()
                        .context("Failed to create filesystem socket name for probe")
                        .and_then(|bind_check| {
                            ListenerOptions::new()
                                .name(bind_check)
                                .create_sync()
                                .map(|listener| {
                                    drop(listener);
                                    let _ = fs::remove_file(&endpoint_display);
                                })
                                .map_err(anyhow::Error::from)
                        }) {
                        Ok(()) => Ok(()),
                        Err(bind_err) => Err(bind_err
                            .downcast_ref::<io::Error>()
                            .map(|err| err.kind())
                            .unwrap_or(io::ErrorKind::Other)),
                    };

                    if let Err(bind_kind) = bind_outcome {
                        match cleanup_stale_socket_file(
                            bind_kind,
                            &endpoint_display,
                            endpoint_exists,
                        ) {
                            StaleSocketCleanup::Removed => {
                                bind_outcome = Ok(());
                            }
                            StaleSocketCleanup::BecameReachable => {
                                return Ok(IpcEndpointStatus::Reachable);
                            }
                            StaleSocketCleanup::NotRemoved => {}
                        }
                    }

                    let status = Self::classify_non_windows_probe(
                        connect_err.kind(),
                        bind_outcome,
                        endpoint_exists,
                    );

                    if status == IpcEndpointStatus::Occupied {
                        log::warn!(
                            "[IPC] Endpoint probe connect failed with {:?}, endpoint_exists={}, classified as occupied on {}",
                            connect_err.kind(),
                            endpoint_exists,
                            endpoint_display
                        );
                    } else {
                        log::debug!(
                            "[IPC] Endpoint probe connect failed with {:?}, endpoint_exists={}, classified as not running on {}",
                            connect_err.kind(),
                            endpoint_exists,
                            endpoint_display
                        );
                    }

                    Ok(status)
                }
            }
        }
    }
    pub fn endpoint_status() -> IpcEndpointStatus {
        match Self::probe_endpoint() {
            Ok(status) => status,
            Err(err) => {
                #[cfg(windows)]
                let platform_branch = "windows-namespaced";
                #[cfg(not(windows))]
                let platform_branch = "non-windows-filesystem";

                let mut root_source: &(dyn std::error::Error + 'static) = err.as_ref();
                while let Some(source) = root_source.source() {
                    root_source = source;
                }

                let root_kind = root_source
                    .downcast_ref::<io::Error>()
                    .map(|io_err| format!("{:?}", io_err.kind()))
                    .unwrap_or_else(|| "Unknown".to_string());

                log::warn!(
                    "[IPC] Endpoint probe failed (platform={}, endpoint={}, root_kind={}, error={})",
                    platform_branch,
                    socket_name_display(),
                    root_kind,
                    err
                );
                IpcEndpointStatus::NotRunning
            }
        }
    }

    /// Send a message to the IPC server and wait for a response.
    ///
    /// This is the primary method for CLI-GUI communication.
    /// Returns an error if the GUI is not running.
    pub fn send(message: &IpcMessage) -> Result<IpcMessage> {
        // Connect to the IPC server
        let mut stream = Self::connect_stream()?;

        // Serialize the message as JSON with newline delimiter
        let mut json = serde_json::to_string(message).context("Failed to serialize IPC message")?;
        json.push('\n');

        // Send the message
        stream
            .write_all(json.as_bytes())
            .context("Failed to send IPC message")?;
        stream.flush().context("Failed to flush IPC stream")?;

        // Read the response (newline-delimited JSON)
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .context("Failed to read IPC response")?;

        // Deserialize the response
        let response: IpcMessage =
            serde_json::from_str(response_line.trim()).context("Failed to parse IPC response")?;

        Ok(response)
    }

    /// Open a persistent IPC connection and send a message.
    ///
    /// Returns the stream (wrapped in a BufReader) for continued reading.
    /// Used by the streaming attach protocol.
    pub fn connect_streaming(
        message: &IpcMessage,
    ) -> Result<BufReader<interprocess::local_socket::Stream>> {
        let mut stream = Self::connect_stream()?;

        let mut json = serde_json::to_string(message).context("Failed to serialize IPC message")?;
        json.push('\n');
        stream
            .write_all(json.as_bytes())
            .context("Failed to send IPC message")?;
        stream.flush().context("Failed to flush IPC stream")?;

        Ok(BufReader::new(stream))
    }

    /// Check if the IPC server is running.
    ///
    /// Used to determine whether to use IPC or fall back to direct operations.
    pub fn is_server_running() -> bool {
        matches!(Self::endpoint_status(), IpcEndpointStatus::Reachable)
    }

    /// Get a human-readable description of the socket name.
    #[allow(dead_code)]
    pub fn socket_name_display() -> String {
        socket_name_display()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_message_serialization() {
        // Test that messages can be serialized to JSON
        let msg = IpcMessage::ListServers;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("ListServers"));

        let msg = IpcMessage::ListSessions;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("ListSessions"));

        let msg = IpcMessage::CreateSession {
            server_name: "test-server".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("CreateSession"));
        assert!(json.contains("test-server"));

        let msg = IpcMessage::SessionOutput {
            session_id: "abc123".to_string(),
            data: vec![72, 101, 108, 108, 111], // "Hello"
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("SessionOutput"));

        let msg = IpcMessage::Resize {
            session_id: "abc".to_string(),
            cols: 120,
            rows: 40,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Resize"));

        let msg = IpcMessage::ExecCommand {
            session_id: "abc".to_string(),
            command: "hostname".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("ExecCommand"));

        let msg = IpcMessage::SessionEnded {
            reason: "done".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("SessionEnded"));

        let msg = IpcMessage::SftpPwd {
            session_id: "abc".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("SftpPwd"));

        let msg = IpcMessage::SftpListDir {
            session_id: "abc".to_string(),
            path: ".".to_string(),
            preserve_cwd: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("SftpListDir"));
        assert!(json.contains("preserve_cwd"));

        let msg = IpcMessage::SftpWriteFile {
            session_id: "abc".to_string(),
            path: "notes.txt".to_string(),
            content: "hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("SftpWriteFile"));

        let msg = IpcMessage::SftpUploadDirectory {
            session_id: "abc".to_string(),
            local_path: "dist".to_string(),
            remote_path: "/var/www".to_string(),
            mode: DirectoryTransferMode::Sync,
            delete_extra: true,
            respect_gitignore: None,
            excluded_paths: vec!["node_modules/".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("SftpUploadDirectory"));
        assert!(json.contains("delete_extra"));
    }

    #[test]
    fn test_ipc_message_deserialization() {
        let json = r#"{"type":"ListSessions"}"#;
        let msg: IpcMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IpcMessage::ListSessions));

        let json = r#"{"type":"ListServers"}"#;
        let msg: IpcMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, IpcMessage::ListServers));

        let json = r#"{"type":"CreateSession","payload":{"server_name":"my-server"}}"#;
        let msg: IpcMessage = serde_json::from_str(json).unwrap();
        if let IpcMessage::CreateSession { server_name } = msg {
            assert_eq!(server_name, "my-server");
        } else {
            panic!("Expected CreateSession message");
        }

        let json = r#"{"type":"SftpPwd","payload":{"session_id":"s1"}}"#;
        let msg: IpcMessage = serde_json::from_str(json).unwrap();
        if let IpcMessage::SftpPwd { session_id } = msg {
            assert_eq!(session_id, "s1");
        } else {
            panic!("Expected SftpPwd message");
        }

        let json = r#"{"type":"SftpListDir","payload":{"session_id":"s1","path":"."}}"#;
        let msg: IpcMessage = serde_json::from_str(json).unwrap();
        if let IpcMessage::SftpListDir {
            session_id,
            path,
            preserve_cwd,
        } = msg
        {
            assert_eq!(session_id, "s1");
            assert_eq!(path, ".");
            assert!(!preserve_cwd);
        } else {
            panic!("Expected SftpListDir message");
        }

        let json = r#"{"type":"ExecCommand","payload":{"session_id":"s1","command":"hostname"}}"#;
        let msg: IpcMessage = serde_json::from_str(json).unwrap();
        if let IpcMessage::ExecCommand {
            session_id,
            command,
        } = msg
        {
            assert_eq!(session_id, "s1");
            assert_eq!(command, "hostname");
        } else {
            panic!("Expected ExecCommand message");
        }
    }

    #[test]
    fn test_socket_name() {
        // Test that socket name can be created successfully
        let result = get_socket_name();
        assert!(result.is_ok(), "Should be able to create socket name");
    }

    #[test]
    fn test_socket_name_display() {
        let display = IpcClient::socket_name_display();
        assert!(
            display.contains("vibeshell"),
            "Socket name should contain vibeshell"
        );
        #[cfg(windows)]
        assert!(
            display.contains("pipe"),
            "Windows socket should be a named pipe"
        );
        #[cfg(not(windows))]
        assert!(
            display.starts_with("/tmp/"),
            "Unix socket should be in /tmp"
        );
    }

    #[test]
    fn test_ipc_client_not_connected() {
        if IpcClient::is_server_running() {
            return;
        }

        // When no server is running, send should fail with connection error
        let result = IpcClient::send(&IpcMessage::ListSessions);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // The error should mention the IPC service endpoint.
        assert!(
            err.contains("connect") || err.contains("IPC service"),
            "Error should mention connection issue: {}",
            err
        );
    }

    #[test]
    fn test_ipc_server_not_running() {
        let _ = IpcClient::is_server_running();
    }

    #[test]
    fn test_non_windows_probe_classifies_occupied_when_bind_in_use() {
        let status = IpcClient::classify_non_windows_probe(
            io::ErrorKind::ConnectionRefused,
            Err(io::ErrorKind::AddrInUse),
            true,
        );
        assert_eq!(status, IpcEndpointStatus::Occupied);
    }

    #[test]
    fn test_non_windows_probe_classifies_not_running_when_bind_succeeds() {
        let status = IpcClient::classify_non_windows_probe(io::ErrorKind::NotFound, Ok(()), false);
        assert_eq!(status, IpcEndpointStatus::NotRunning);
    }
    #[test]
    fn test_non_windows_probe_classifies_occupied_when_bind_permission_denied() {
        let status = IpcClient::classify_non_windows_probe(
            io::ErrorKind::NotFound,
            Err(io::ErrorKind::PermissionDenied),
            false,
        );
        assert_eq!(status, IpcEndpointStatus::Occupied);
    }

    #[test]
    fn test_non_windows_probe_classifies_occupied_when_bind_other_and_endpoint_exists() {
        let status = IpcClient::classify_non_windows_probe(
            io::ErrorKind::ConnectionRefused,
            Err(io::ErrorKind::Other),
            true,
        );
        assert_eq!(status, IpcEndpointStatus::Occupied);
    }

    #[cfg(not(windows))]
    #[test]
    fn test_non_windows_stale_socket_cleanup_is_limited_to_addr_in_use_files() {
        assert!(is_stale_socket_bind_error(io::ErrorKind::AddrInUse, true));
        assert!(!is_stale_socket_bind_error(
            io::ErrorKind::PermissionDenied,
            true
        ));
        assert!(!is_stale_socket_bind_error(io::ErrorKind::AddrInUse, false));
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_listener_error_232_is_recoverable() {
        let err = io::Error::from_raw_os_error(232);
        assert!(is_recoverable_listener_error(&err));
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_permission_denied_listener_error_is_not_recoverable() {
        let err = io::Error::from_raw_os_error(5);
        assert!(!is_recoverable_listener_error(&err));
    }
}
