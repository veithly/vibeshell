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
use tokio::io::AsyncReadExt;

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
    resolve_remote_path, resolve_remote_upload_path, sftp_delete_path, sftp_mkdir_recursive,
    write_remote_file, write_remote_file_with_options, WriteRemoteFileOptions,
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

/// Server metadata returned to CLI for `vibeshell servers`.
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

/// Session metadata returned to CLI for `vibeshell sessions`.
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
    SessionService {
        request: super::runtime_services::RuntimeRequest,
    },
    ServiceResult {
        value: serde_json::Value,
    },
    PluginList {
        installed_only: bool,
    },
    PluginDescribe {
        plugin_id: String,
        reference: bool,
    },
    PluginExecute {
        request: crate::plugins::PluginExecuteRequest,
    },
    PluginData {
        data: serde_json::Value,
    },
    /// Create a new session connecting to the specified server
    CreateSession {
        server_name: String,
    },
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
    AttachSession {
        session_id: String,
    },
    /// Detach from a session (keeps it running)
    DetachSession {
        session_id: String,
    },
    /// Kill/terminate a session
    KillSession {
        session_id: String,
    },
    /// Send input data to a session
    SendInput {
        session_id: String,
        data: Vec<u8>,
    },
    /// GUI keystrokes share input tracking, but are not Agent commands.
    SendUserInput {
        session_id: String,
        data: Vec<u8>,
    },
    /// Secret input is classified normally, but its contents never enter audit.
    SendSensitiveInput {
        session_id: String,
        data: Vec<u8>,
    },
    /// Resize the PTY for a session
    Resize {
        session_id: String,
        cols: u32,
        rows: u32,
    },
    /// Execute a single command on an existing SSH session
    ExecCommand {
        session_id: String,
        command: String,
        #[serde(default)]
        stdin: Option<String>,
    },
    /// Capture a quick command's actual exit status, with GUI execution limits.
    ExecQuickCommand {
        session_id: String,
        command: String,
    },
    /// Initialize SFTP context for a session
    SftpInit {
        session_id: String,
    },
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
    SftpMkdir {
        session_id: String,
        path: String,
    },
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
    SftpPwd {
        session_id: String,
    },
    /// Stat a remote path
    SftpStat {
        session_id: String,
        path: String,
    },
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
    /// Create a remote text file, honoring add-file overwrite and parents semantics
    SftpAddFile {
        session_id: String,
        path: String,
        content: String,
        #[serde(default)]
        overwrite: bool,
        #[serde(default)]
        parents: bool,
    },

    // Responses from GUI to CLI
    /// List of configured servers
    ServerList {
        servers: Vec<IpcServerInfo>,
    },
    /// List of active session IDs
    SessionList {
        sessions: Vec<IpcSessionInfo>,
    },
    /// A new session was created
    SessionCreated {
        session_id: String,
    },
    /// Output data from a session (used in streaming mode)
    SessionOutput {
        session_id: String,
        data: Vec<u8>,
    },
    /// Session has ended (sent during streaming attach)
    SessionEnded {
        reason: String,
    },
    /// Output for a single remote command
    CommandOutput {
        output: String,
    },
    CommandResult {
        output: String,
        exit_code: i32,
    },
    /// SFTP directory entries
    SftpEntries {
        entries: Vec<SftpEntry>,
    },
    /// SFTP path response
    SftpPath {
        path: String,
    },
    /// SFTP stat response
    SftpStatResult {
        entry: SftpEntry,
    },
    /// SFTP file preview response
    SftpFileContent {
        content: SftpFileContent,
    },
    /// SFTP transfer response
    SftpTransfer {
        progress: TransferProgress,
    },
    /// SFTP directory transfer response
    SftpDirectoryTransfer {
        summary: DirectoryTransferSummary,
    },
    /// Error response
    Error {
        message: String,
    },
    /// Success acknowledgment
    Ok,
}

#[cfg(not(windows))]
use sha2::{Digest, Sha256};

/// Socket name type alias for platform-specific implementation.
#[cfg(windows)]
type SocketName = interprocess::local_socket::Name<'static>;
#[cfg(not(windows))]
type SocketName = interprocess::local_socket::Name<'static>;

/// Per-user private directory holding the IPC socket on unix.
///
/// A fixed location such as `/tmp/vibeshell.sock` lets any local user
/// pre-create or squat the path and would expose the endpoint to every
/// account on the machine. The endpoint therefore lives in a directory owned
/// by the current user (mode 0700): `$XDG_RUNTIME_DIR/vibeshell-ipc` when the
/// platform provides a runtime dir (Linux), otherwise
/// `$TMPDIR/vibeshell-ipc-<uid-tag>` keyed by a hash of `$HOME`.
#[cfg(not(windows))]
fn ipc_socket_dir() -> Result<PathBuf> {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let runtime_dir = runtime_dir.trim();
        if !runtime_dir.is_empty() {
            let dir = Path::new(runtime_dir).join("vibeshell-ipc");
            ensure_private_directory(&dir)?;
            return Ok(dir);
        }
    }

    let dir = std::env::temp_dir().join(format!("vibeshell-ipc-{}", ipc_user_tag()));
    ensure_private_directory(&dir)?;
    Ok(dir)
}

#[cfg(not(windows))]
fn ensure_private_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error).context("Cannot create private IPC directory"),
    }
    let metadata = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "IPC directory must not be a symlink or file"
    );
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .context("Cannot secure IPC directory; refusing to expose the service")?;
    Ok(())
}

/// Stable per-user token for the temp-dir fallback. `$HOME` identifies the
/// account; hashing keeps the path short and avoids embedding user names.
/// When `$HOME` is unset, callers share one 0700 directory (first creator
/// wins), which is fine for the same-user daemon/CLI deployment model.
#[cfg(not(windows))]
fn ipc_user_tag() -> String {
    let identity = std::env::var("HOME").unwrap_or_default();
    let digest = Sha256::digest(identity.as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Restrict a file or directory to its owner (no-op on failure so endpoint
/// setup never hard-fails on exotic filesystems).
#[cfg(not(windows))]
fn set_private_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(mode);
        let _ = fs::set_permissions(path, permissions);
    }
}

/// Full filesystem path of the unix IPC socket.
#[cfg(not(windows))]
fn socket_file_path() -> Result<PathBuf> {
    bounded_socket_path(ipc_socket_dir()?, &socket_name_base())
}

#[cfg(not(windows))]
fn bounded_socket_path(mut directory: PathBuf, name: &str) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;
    anyhow::ensure!(
        !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0']),
        "IPC name must be a single file name"
    );
    let path = directory.join(name);
    // Keep existing short endpoints unchanged for GUI/CLI compatibility.
    if path.as_os_str().as_bytes().len() <= 100 {
        return Ok(path);
    }
    let digest = Sha256::digest(path.as_os_str().as_bytes());
    let shortened: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if directory.join(&shortened).as_os_str().as_bytes().len() > 100 {
        directory = PathBuf::from("/tmp").join(format!("vibeshell-ipc-{}", ipc_user_tag()));
        match fs::create_dir(&directory) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error).context("Cannot create short IPC directory"),
        }
        let metadata = fs::symlink_metadata(&directory)?;
        anyhow::ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "IPC directory must not be a symlink or file"
        );
        // Failure is fatal: never expose an IPC endpoint in another user's directory.
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    }
    Ok(directory.join(shortened))
}

/// Get the platform-specific socket name/path.
///
/// On Windows, we use named pipes in the namespaced format.
/// On Unix, we use a Unix domain socket inside a per-user private directory.
fn get_socket_name() -> Result<SocketName> {
    #[cfg(windows)]
    {
        let socket_name = socket_name_base();
        // On Windows, use namespaced name (named pipe)
        socket_name
            .to_ns_name::<GenericNamespaced>()
            .context("Failed to create namespaced socket name")
    }
    #[cfg(not(windows))]
    {
        // On Unix, use a socket file in a per-user private directory
        // (`VIBESHELL_IPC_NAME` still only names the file inside that dir).
        socket_file_path()?
            .to_string_lossy()
            .to_string()
            .to_fs_name::<GenericFilePath>()
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
        socket_file_path()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| format!("<unavailable>/{}", socket_name_base()))
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

/// Copy of an IPC message that is safe to include in debug logs: the secret
/// fields of `CreateSessionWithCredentials` are replaced with placeholders so
/// credentials never reach the log output.
fn redact_for_log(message: &IpcMessage) -> IpcMessage {
    match message {
        IpcMessage::CreateSessionWithCredentials {
            server_name,
            auth_type,
            passphrase,
            cols,
            rows,
            ..
        } => IpcMessage::CreateSessionWithCredentials {
            server_name: server_name.clone(),
            auth_type: auth_type.clone(),
            credential: "[REDACTED]".to_string(),
            passphrase: passphrase.as_ref().map(|_| "[REDACTED]".to_string()),
            cols: *cols,
            rows: *rows,
        },
        IpcMessage::ExecCommand {
            session_id, stdin, ..
        } => IpcMessage::ExecCommand {
            session_id: session_id.clone(),
            command: "[REDACTED]".into(),
            stdin: stdin.as_ref().map(|_| "[REDACTED]".into()),
        },
        IpcMessage::ExecQuickCommand { session_id, .. } => IpcMessage::ExecQuickCommand {
            session_id: session_id.clone(),
            command: "[REDACTED]".into(),
        },
        IpcMessage::SendInput { session_id, .. }
        | IpcMessage::SendUserInput { session_id, .. }
        | IpcMessage::SendSensitiveInput { session_id, .. } => IpcMessage::SendInput {
            session_id: session_id.clone(),
            data: Vec::new(),
        },
        IpcMessage::SftpWriteFile {
            session_id, path, ..
        } => IpcMessage::SftpWriteFile {
            session_id: session_id.clone(),
            path: path.clone(),
            content: "[REDACTED]".into(),
        },
        IpcMessage::SftpAddFile {
            session_id,
            path,
            overwrite,
            parents,
            ..
        } => IpcMessage::SftpAddFile {
            session_id: session_id.clone(),
            path: path.clone(),
            content: "[REDACTED]".into(),
            overwrite: *overwrite,
            parents: *parents,
        },
        other => other.clone(),
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

        #[cfg(not(windows))]
        {
            // Restrict the socket file to its owner so other local accounts
            // cannot connect to (or tamper with) the IPC endpoint.
            if let Ok(path) = socket_file_path() {
                set_private_mode(&path, 0o600);
            }
        }

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

        log::debug!("[IPC] Received: {:?}", redact_for_log(&message));

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
        use crate::mcp::server::{AgentActivityEvent, AgentActivityStatus};
        let user_input = matches!(&message, IpcMessage::SendUserInput { .. });
        let sensitive = matches!(&message, IpcMessage::SendSensitiveInput { .. });
        let message = match message {
            IpcMessage::SendUserInput { session_id, data }
            | IpcMessage::SendSensitiveInput { session_id, data } => {
                IpcMessage::SendInput { session_id, data }
            }
            other => other,
        };
        let mut input_guard = None;
        let mut checkpoint = None;
        let mut denied = None;
        let mut descriptions: Vec<(String, Option<String>, String)> = Vec::new();
        match &message {
            IpcMessage::SendInput { session_id, data } => {
                input_guard =
                    Some(rt.block_on(session_manager.agent_input_tracker.lock_session(session_id)));
                let (saved, commands, redact) = rt.block_on(
                    session_manager
                        .agent_input_tracker
                        .checkpoint_and_observe_sensitive(
                            session_id,
                            &String::from_utf8_lossy(data),
                            &[],
                            false,
                            sensitive,
                        ),
                );
                checkpoint = Some(saved);
                if !user_input {
                    let config = crate::mcp::GuardConfig::from_stored_json(
                        database
                            .get_setting(crate::mcp::guard::GUARD_CONFIG_KEY)
                            .ok()
                            .flatten()
                            .as_deref(),
                    );
                    if commands.is_empty() && !data.is_empty() {
                        descriptions.push(("cli.input_pending".into(), Some(session_id.clone()),
                            if redact { "[sensitive input omitted]".into() } else {
                                String::from_utf8_lossy(data).chars().flat_map(|ch| ch.escape_debug()).collect()
                            }));
                    }
                    for command in commands {
                        if config.enabled
                            && (!command.is_verifiable
                                || crate::mcp::guard::classify_command(&command.command, &config)
                                    .requires_approval)
                        {
                            denied = Some("Terminal command requires approval in the VibeShell Agent Gateway; no input was sent".to_string());
                        }
                        descriptions.push((
                            "cli.input".into(),
                            Some(session_id.clone()),
                            if redact {
                                "[sensitive input omitted]".into()
                            } else {
                                command.command
                            },
                        ));
                    }
                }
            }
            IpcMessage::ExecCommand {
                session_id,
                command,
                ..
            } => descriptions.push(("cli.exec".into(), Some(session_id.clone()), command.clone())),
            IpcMessage::CreateSession { server_name }
            | IpcMessage::CreateSessionWithCredentials { server_name, .. } => {
                descriptions.push(("cli.session_create".into(), None, server_name.clone()))
            }
            IpcMessage::SftpWriteFile { session_id, path, .. } | IpcMessage::SftpAddFile { session_id, path, .. } =>
                descriptions.push(("cli.sftp_write".into(), Some(session_id.clone()), format!("Write {path} (contents omitted)"))),
            IpcMessage::SftpReadFile { session_id, path, .. } =>
                descriptions.push(("cli.sftp_read".into(), Some(session_id.clone()), path.clone())),
            IpcMessage::SftpListDir { session_id, path, .. } =>
                descriptions.push(("cli.sftp_list".into(), Some(session_id.clone()), path.clone())),
            IpcMessage::SftpMkdir { session_id, path } =>
                descriptions.push(("cli.sftp_mkdir".into(), Some(session_id.clone()), path.clone())),
            IpcMessage::SftpDelete { session_id, path, recursive } =>
                descriptions.push(("cli.sftp_delete".into(), Some(session_id.clone()), format!("{path} (recursive={recursive})"))),
            IpcMessage::SftpRename { session_id, old_path, new_path } =>
                descriptions.push(("cli.sftp_rename".into(), Some(session_id.clone()), format!("{old_path} → {new_path}"))),
            IpcMessage::SftpDownloadFile { session_id, remote_path, local_path } =>
                descriptions.push(("cli.sftp_download".into(), Some(session_id.clone()), format!("{remote_path} → {local_path}"))),
            IpcMessage::SftpUploadFile { session_id, local_path, remote_path } =>
                descriptions.push(("cli.sftp_upload".into(), Some(session_id.clone()), format!("{local_path} → {remote_path}"))),
            IpcMessage::SftpUploadDirectory { session_id, local_path, remote_path, mode, delete_extra, excluded_paths, .. } =>
                descriptions.push(("cli.sftp_directory".into(), Some(session_id.clone()), format!("{mode:?}: {local_path} → {remote_path}; delete_extra={delete_extra}; excludes={excluded_paths:?}"))),
            IpcMessage::PluginExecute { request } => descriptions.push(("cli.plugin_request".into(), Some(request.session_id.clone()), format!("{}/{}", request.plugin_id, request.action_id))),
            IpcMessage::KillSession { session_id } => descriptions.push((
                "cli.session_kill".into(),
                Some(session_id.clone()),
                "Close session".into(),
            )),
            _ => {}
        }
        let mut events: Vec<_> = descriptions
            .into_iter()
            .map(|(tool, session_id, summary)| AgentActivityEvent {
                id: uuid::Uuid::new_v4().to_string(),
                tool,
                session_id,
                summary,
                status: AgentActivityStatus::Started,
                timestamp: chrono::Utc::now().timestamp_millis(),
            })
            .collect();
        for event in &events {
            if let Err(error) = database.agent_activity_record(event) {
                if let Some(saved) = checkpoint {
                    rt.block_on(session_manager.agent_input_tracker.restore(saved));
                }
                return IpcMessage::Error { message: format!("Operation was not executed because its audit record could not be saved: {error}") };
            }
        }
        let response = if let Some(message) = denied {
            IpcMessage::Error { message }
        } else {
            Self::dispatch_message(
                message,
                database.clone(),
                session_manager.clone(),
                sftp_contexts,
                rt,
            )
        };
        if matches!(&response, IpcMessage::Error { .. }) {
            if let Some(saved) = checkpoint {
                rt.block_on(session_manager.agent_input_tracker.restore(saved));
            }
        }
        drop(input_guard);
        for event in &mut events {
            event.status = if matches!(&response, IpcMessage::Error { .. }) {
                AgentActivityStatus::Failed
            } else {
                AgentActivityStatus::Succeeded
            };
            event.timestamp = chrono::Utc::now().timestamp_millis();
            if let IpcMessage::SessionCreated { session_id } = &response {
                event.session_id = Some(session_id.clone());
            }
            if let Err(error) = database.agent_activity_record(event) {
                log::error!("Operation completed but completion audit could not be saved: {error}");
            }
        }
        response
    }

    fn dispatch_message(
        message: IpcMessage,
        database: Arc<Database>,
        session_manager: Arc<SessionManager>,
        sftp_contexts: Arc<Mutex<std::collections::HashMap<String, SftpContext>>>,
        rt: &tokio::runtime::Handle,
    ) -> IpcMessage {
        // Normalize both exec entry points into the same approval gate.
        let quick = matches!(&message, IpcMessage::ExecQuickCommand { .. });
        let message = match message {
            IpcMessage::ExecQuickCommand {
                session_id,
                command,
            } => IpcMessage::ExecCommand {
                session_id,
                command,
                stdin: None,
            },
            other => other,
        };
        match message {
            IpcMessage::SessionService { request } => {
                match rt.block_on(super::runtime_services::dispatch(&session_manager, request)) {
                    Ok(value) => IpcMessage::ServiceResult { value },
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::PluginList { installed_only } => {
                match crate::plugins::agent::list(&database, installed_only) {
                    Ok(data) => IpcMessage::PluginData {
                        data: serde_json::json!(data),
                    },
                    Err(message) => IpcMessage::Error { message },
                }
            }
            IpcMessage::PluginDescribe {
                plugin_id,
                reference,
            } => match crate::plugins::agent::describe(&database, &plugin_id, reference) {
                Ok(data) => IpcMessage::PluginData { data },
                Err(message) => IpcMessage::Error { message },
            },
            IpcMessage::PluginExecute { request } => {
                match rt.block_on(crate::plugins::agent::execute(
                    &database,
                    &session_manager,
                    request,
                    "cli.plugin",
                    None,
                )) {
                    Ok(data) => IpcMessage::PluginData {
                        data: serde_json::json!(data),
                    },
                    Err(message) => IpcMessage::Error { message },
                }
            }
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
                        let ssh_cred = match crate::session::SshCredential::from_stored(cred) {
                            Ok(credential) => credential,
                            Err(error) => {
                                return IpcMessage::Error {
                                    message: format!(
                                        "Failed to load credentials for '{}': {}",
                                        server_name, error
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
                    std::result::Result::Ok(None) => IpcMessage::Error {
                        message: format!(
                            "No saved credentials for server '{}'. Save the password or private key in VibeShell, or connect through the GUI. No SSH connection was created.",
                            server_name
                        ),
                    },
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
                stdin,
            } => {
                // IPC callers have no approval UI. Apply the same command-risk
                // classification the MCP agent gateway uses and fail closed on
                // risky commands instead of executing them unattended.
                let guard_cfg = {
                    let stored = database
                        .get_setting(crate::mcp::guard::GUARD_CONFIG_KEY)
                        .ok()
                        .flatten();
                    crate::mcp::guard::GuardConfig::from_stored_json(stored.as_deref())
                };
                if guard_cfg.enabled && guard_cfg.require_for_exec {
                    let decision = crate::mcp::guard::classify_command(&command, &guard_cfg);
                    if decision.requires_approval {
                        log::warn!(
                            "[IPC] Blocked ExecCommand on session {} (requires approval): {}",
                            session_id,
                            decision.reasons.join("; ")
                        );
                        return IpcMessage::Error {
                            message: format!(
                                "Command requires user approval, but no approval UI is \
                                 available over IPC. Blocked for safety: {}",
                                decision.reasons.join("; ")
                            ),
                        };
                    }
                }

                match rt.block_on(async {
                    let session = session_manager
                        .get(&session_id)
                        .await
                        .ok_or_else(|| format!("Session not found: {}", session_id))?;
                    if quick {
                        session
                            .exec_quick_command(&command)
                            .await
                            .map(|result| IpcMessage::CommandResult {
                                output: result.output,
                                exit_code: result.exit_code,
                            })
                            .map_err(|e| format!("Failed to execute command: {e:#}"))
                    } else {
                        session
                            .exec_command_with_stdin(&command, stdin.as_deref())
                            .await
                            .map(|output| IpcMessage::CommandOutput { output })
                            .map_err(|e| format!("Failed to execute command: {e:#}"))
                    }
                }) {
                    std::result::Result::Ok(response) => response,
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
                        crate::sftp::helpers::validate_remote_entry_name(&name)?;
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
                    let read_limit = if binary {
                        file_size
                    } else {
                        max_size.min(file_size)
                    };
                    let file = sftp
                        .open(&resolved)
                        .await
                        .map_err(|e| format!("Failed to open file {}: {}", resolved, e))?;
                    let mut bytes = Vec::with_capacity(read_limit.min(usize::MAX as u64) as usize);
                    file.take(read_limit)
                        .read_to_end(&mut bytes)
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
                        (
                            String::from_utf8_lossy(&bytes).to_string(),
                            file_size > bytes.len() as u64,
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
            IpcMessage::SftpAddFile {
                session_id,
                path,
                content,
                overwrite,
                parents,
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

                    write_remote_file_with_options(
                        &sftp,
                        &resolved,
                        content.as_bytes(),
                        WriteRemoteFileOptions {
                            create_parent_dirs: parents,
                            overwrite,
                        },
                    )
                    .await
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
                // The local target is remote-influenced content: confine it to
                // benign directories so it cannot be planted over dotfiles,
                // LaunchAgents, crontabs, etc.
                let target = match crate::mcp::server::confine_download_target(&local_path) {
                    Ok(target) => target,
                    Err(message) => return IpcMessage::Error { message },
                };
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
                    if let Some(parent) = target.parent() {
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
                    crate::mcp::server::ensure_parent_within_allowlist(&target)?;
                    std::fs::write(&target, &content).map_err(|e| {
                        format!("Failed to write local file {}: {}", target.display(), e)
                    })?;

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
                    sftp_delete_path(&sftp, &resolved, recursive).await
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

    #[cfg(not(windows))]
    #[test]
    fn socket_names_are_bounded_stable_and_confined() {
        use std::os::unix::ffi::OsStrExt;
        let dir = std::env::temp_dir().join("vibeshell-ipc-path-test");
        let name = "long-endpoint-".repeat(30);
        let path = bounded_socket_path(dir.clone(), &name).unwrap();
        assert!(path.as_os_str().as_bytes().len() <= 100);
        assert_eq!(path, bounded_socket_path(dir.clone(), &name).unwrap());
        assert_ne!(
            path,
            bounded_socket_path(dir.clone(), &(name + "other")).unwrap()
        );
        for invalid in [
            "/tmp/public.sock",
            "../escape",
            "a/b",
            ".",
            "..",
            "bad\0name",
        ] {
            assert!(bounded_socket_path(dir.clone(), invalid).is_err());
        }
    }

    #[test]
    fn missing_saved_credentials_never_create_a_phantom_session() {
        let directory = tempfile::tempdir().unwrap();
        let database = Arc::new(Database::new_at(directory.path().join("test.db")).unwrap());
        let manager = Arc::new(SessionManager::new(database.clone()));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let response = IpcServer::handle_message(
            IpcMessage::CreateSession {
                server_name: "no-credentials".into(),
            },
            database,
            manager.clone(),
            Arc::new(Mutex::new(std::collections::HashMap::new())),
            runtime.handle(),
        );
        assert!(
            matches!(response, IpcMessage::Error { message } if message.contains("No saved credentials"))
        );
        assert!(runtime.block_on(manager.list()).is_empty());
    }

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
            stdin: None,
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

        let msg = IpcMessage::SftpAddFile {
            session_id: "abc".to_string(),
            path: "notes.txt".to_string(),
            content: "hello".to_string(),
            overwrite: false,
            parents: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("SftpAddFile"));
        assert!(json.contains("parents"));

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
            stdin: _,
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
        {
            assert!(
                display.contains("vibeshell-ipc"),
                "Unix socket should live in a per-user vibeshell-ipc directory: {}",
                display
            );
            assert!(
                display.ends_with(DEFAULT_SOCKET_NAME),
                "Unix socket display should end with the socket file name: {}",
                display
            );
            // The old fixed `/tmp/vibeshell.sock` path must not be used.
            assert_ne!(display, format!("/tmp/{}", DEFAULT_SOCKET_NAME));
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn test_ipc_socket_dir_is_private() {
        let dir = ipc_socket_dir().expect("IPC socket directory should be creatable");
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "IPC socket directory must be private");
        // Same-user clients resolve the same path deterministically.
        let again = ipc_socket_dir().expect("second call should succeed");
        assert_eq!(dir, again);
    }

    #[cfg(unix)]
    #[test]
    fn private_ipc_directory_rejects_symlinks() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("target");
        let link = temporary.path().join("link");
        fs::create_dir(&target).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(ensure_private_directory(&link).is_err());
        assert!(ensure_private_directory(&target).is_ok());
    }

    #[test]
    fn native_activity_retains_failed_attempts_but_never_authentication_or_file_contents() {
        let temporary = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new_at(temporary.path().join("audit.db")).unwrap());
        let manager = Arc::new(SessionManager::new(db.clone()));
        let contexts = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let send = |message| {
            IpcServer::handle_message(
                message,
                db.clone(),
                manager.clone(),
                contexts.clone(),
                runtime.handle(),
            )
        };
        for _ in 0..2 {
            assert!(matches!(
                send(IpcMessage::ExecCommand {
                    session_id: "missing".into(),
                    command: "printf audited".into(),
                    stdin: Some("stdin-secret".into())
                }),
                IpcMessage::Error { .. }
            ));
        }
        send(IpcMessage::SendSensitiveInput {
            session_id: "missing".into(),
            data: b"input-secret\n".to_vec(),
        });
        send(IpcMessage::SftpWriteFile {
            session_id: "missing".into(),
            path: "/file".into(),
            content: "file-secret".into(),
        });
        let events = db.agent_activity_list(Some(0), None, 100).unwrap();
        assert_eq!(events.len(), 8);
        assert_ne!(events[0].event.id, events[2].event.id);
        assert!(
            events
                .iter()
                .filter(
                    |event| event.event.status == crate::mcp::server::AgentActivityStatus::Failed
                )
                .count()
                == 4
        );
        let text = serde_json::to_string(&events).unwrap();
        for secret in ["stdin-secret", "input-secret", "file-secret"] {
            assert!(!text.contains(secret));
        }
        assert!(text.contains("printf audited") && text.contains("/file"));
        send(IpcMessage::SendUserInput {
            session_id: "missing".into(),
            data: b"human-private-input\n".to_vec(),
        });
        assert_eq!(db.agent_activity_list(Some(0), None, 100).unwrap().len(), 8);
    }

    #[test]
    fn test_redact_for_log_hides_credentials() {
        let msg = IpcMessage::CreateSessionWithCredentials {
            server_name: "web".to_string(),
            auth_type: "password".to_string(),
            credential: "hunter2".to_string(),
            passphrase: Some("secondary-secret".to_string()),
            cols: Some(80),
            rows: Some(24),
        };
        let debug = format!("{:?}", redact_for_log(&msg));
        assert!(!debug.contains("hunter2"), "password leaked to log output");
        assert!(
            !debug.contains("secondary-secret"),
            "passphrase leaked to log output"
        );
        assert!(debug.contains("[REDACTED]"));
        assert!(debug.contains("web"));

        // Non-credential messages pass through unchanged.
        assert!(matches!(
            redact_for_log(&IpcMessage::ListSessions),
            IpcMessage::ListSessions
        ));
    }

    #[test]
    fn command_secrets_are_redacted_and_exit_codes_roundtrip() {
        for message in [
            IpcMessage::ExecCommand {
                session_id: "s".into(),
                command: "secret-command".into(),
                stdin: Some("secret-password".into()),
            },
            IpcMessage::ExecQuickCommand {
                session_id: "s".into(),
                command: "secret-command".into(),
            },
            IpcMessage::SendInput {
                session_id: "s".into(),
                data: b"secret-password".to_vec(),
            },
            IpcMessage::SftpWriteFile {
                session_id: "s".into(),
                path: "/f".into(),
                content: "secret-content".into(),
            },
        ] {
            assert!(!format!("{:?}", redact_for_log(&message)).contains("secret"));
        }
        let message = IpcMessage::CommandResult {
            output: "failure".into(),
            exit_code: 7,
        };
        let decoded: IpcMessage =
            serde_json::from_str(&serde_json::to_string(&message).unwrap()).unwrap();
        assert!(matches!(
            decoded,
            IpcMessage::CommandResult { exit_code: 7, .. }
        ));
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
