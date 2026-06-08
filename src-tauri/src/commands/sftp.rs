//! SFTP commands for file transfer operations.
//!
//! This module provides Tauri commands for SFTP file operations including
//! directory listing, file upload/download, and file management.
//! SSH sessions use the real SFTP protocol via russh-sftp.
//! Local shell sessions use direct filesystem operations.

use log::{debug, info};
use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tauri::State;
use tokio::sync::{Mutex as TokioMutex, RwLock};

use crate::commands::session::SessionAccessState;
use crate::ipc::{IpcClient, IpcMessage};
use crate::local_shell::LocalShellManager;
use crate::session::SessionManager;
use crate::sftp::helpers::{
    resolve_remote_path, resolve_remote_upload_path, sftp_mkdir_recursive, sftp_remove_recursive,
    write_remote_file,
};
use crate::sftp::{
    default_upload_ignore_config, effective_directory_transfer_options, load_upload_ignore_config,
    save_upload_ignore_config, transfer_directory_to_local, transfer_directory_to_sftp,
    DirectoryTransferMode, DirectoryTransferSummary, TransferProgress, TransferStatus,
    UploadIgnoreConfig,
};

// ==================== Request Structs ====================
// These structs use camelCase serialization for Tauri 2.x compatibility

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpInitRequest {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpListDirRequest {
    pub session_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpDownloadRequest {
    pub session_id: String,
    pub remote_path: String,
    pub local_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpUploadRequest {
    pub session_id: String,
    pub local_path: String,
    pub remote_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpUploadDirectoryRequest {
    pub session_id: String,
    pub local_path: String,
    pub remote_path: String,
    pub mode: Option<DirectoryTransferMode>,
    pub delete_extra: Option<bool>,
    pub respect_gitignore: Option<bool>,
    pub excluded_paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpMkdirRequest {
    pub session_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpDeleteRequest {
    pub session_id: String,
    pub path: String,
    pub recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpRenameRequest {
    pub session_id: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpPwdRequest {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpStatRequest {
    pub session_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpReadFileRequest {
    pub session_id: String,
    pub path: String,
    /// Maximum bytes to read for text preview (default: 1MB)
    pub max_size: Option<u64>,
    /// Whether to read as binary (base64 encoded) for images
    pub as_binary: Option<bool>,
}

/// Response for file content read
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpFileContent {
    /// File content (plain text or base64 encoded for binary)
    pub content: String,
    /// Whether the content is base64 encoded
    pub is_binary: bool,
    /// File size in bytes
    pub size: u64,
    /// Whether the content was truncated due to size limit
    pub truncated: bool,
    /// MIME type hint based on extension
    pub mime_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpWriteFileRequest {
    pub session_id: String,
    pub path: String,
    /// File content to write (plain text)
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpCompressRequest {
    pub session_id: String,
    /// List of file/directory paths to compress
    pub paths: Vec<String>,
    /// Output archive path (e.g., /path/to/archive.tar.gz)
    pub archive_path: String,
    /// Compression format: "tar.gz" or "zip"
    pub format: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpExtractRequest {
    pub session_id: String,
    /// Path to the archive file
    pub archive_path: String,
    /// Destination directory for extraction
    pub destination_path: String,
}

/// Data for an active SFTP session
pub struct SftpSessionData {
    /// The real SFTP session (only for SSH sessions, None for local)
    pub sftp: Option<SftpSession>,
    /// The user's home directory on the remote server (resolved on init)
    pub home_dir: String,
    /// Current working directory on the remote server
    pub current_path: String,
    /// Whether the SFTP subsystem is connected
    pub connected: bool,
}

/// SFTP session state shared across commands
pub struct SftpState {
    /// Maps session_id to SftpSession data (behind Mutex for safe async access)
    pub sessions: Arc<RwLock<HashMap<String, Arc<TokioMutex<SftpSessionData>>>>>,
}

impl SftpState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Remove SFTP session data for a given session ID (called on session kill)
    pub async fn cleanup_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_some() {
            info!("[SFTP] Cleaned up SFTP session for {}", session_id);
        }
    }

    /// Remove all SFTP session data (called on kill_all)
    pub async fn cleanup_all(&self) {
        let mut sessions = self.sessions.write().await;
        let count = sessions.len();
        sessions.clear();
        if count > 0 {
            info!("[SFTP] Cleaned up {} SFTP sessions", count);
        }
    }
}

impl Default for SftpState {
    fn default() -> Self {
        Self::new()
    }
}

/// SFTP entry returned to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified_at: i64,
    pub permissions: String,
}

// ==================== Helper Functions ====================

/// Get an existing SFTP session data Arc from state
async fn get_sftp_data(
    sftp_state: &Arc<SftpState>,
    session_id: &str,
) -> Result<Arc<TokioMutex<SftpSessionData>>, String> {
    let sessions = sftp_state.sessions.read().await;
    sessions.get(session_id).cloned().ok_or_else(|| {
        format!(
            "SFTP not initialized for session: {}. Call sftp_init first.",
            session_id
        )
    })
}

// ==================== Local filesystem helpers ====================

fn local_home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn normalize_path(path: PathBuf) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn resolve_local_path(path: &str, current_path: &str) -> Result<PathBuf, String> {
    let input = path.trim();
    let resolved = if input.is_empty() || input == "~" {
        local_home_dir()
    } else if let Some(rest) = input
        .strip_prefix("~/")
        .or_else(|| input.strip_prefix("~\\"))
    {
        local_home_dir().join(rest)
    } else {
        let path_buf = PathBuf::from(input);
        if path_buf.is_absolute() {
            path_buf
        } else {
            let base = if current_path.trim().is_empty() {
                local_home_dir()
            } else {
                PathBuf::from(current_path)
            };
            base.join(path_buf)
        }
    };

    Ok(normalize_path(resolved))
}

fn local_permissions_string(metadata: &std::fs::Metadata) -> String {
    let mut s = String::new();
    s.push(if metadata.is_dir() { 'd' } else { '-' });

    if metadata.permissions().readonly() {
        s.push_str("r--r--r--");
    } else {
        s.push_str("rw-rw-rw-");
    }

    s
}

async fn is_local_session(local_shell_manager: &Arc<LocalShellManager>, session_id: &str) -> bool {
    local_shell_manager.get_session(session_id).await.is_some()
}

async fn get_local_current_path(sftp_state: &Arc<SftpState>, session_id: &str) -> String {
    let sessions = sftp_state.sessions.read().await;
    if let Some(data_arc) = sessions.get(session_id) {
        let guard = data_arc.lock().await;
        if !guard.current_path.trim().is_empty() {
            return guard.current_path.clone();
        }
    }
    local_home_dir().to_string_lossy().to_string()
}

async fn set_local_current_path(sftp_state: &Arc<SftpState>, session_id: &str, path: &Path) {
    let sessions = sftp_state.sessions.read().await;
    if let Some(data_arc) = sessions.get(session_id) {
        let mut guard = data_arc.lock().await;
        guard.current_path = path.to_string_lossy().to_string();
    }
}

async fn ipc_send(message: IpcMessage) -> Result<IpcMessage, String> {
    tokio::task::spawn_blocking(move || IpcClient::send(&message).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("IPC worker failed: {}", e))?
}

fn unexpected_ipc_response(context: &str, message: IpcMessage) -> String {
    format!("Unexpected IPC response while {}: {:?}", context, message)
}

fn expect_ipc_ok(context: &str, message: IpcMessage) -> Result<(), String> {
    match message {
        IpcMessage::Ok => Ok(()),
        IpcMessage::Error { message } => Err(message),
        other => Err(unexpected_ipc_response(context, other)),
    }
}

async fn remote_exec_command(session_id: &str, command: String) -> Result<(), String> {
    match ipc_send(IpcMessage::ExecCommand {
        session_id: session_id.to_string(),
        command,
    })
    .await?
    {
        IpcMessage::CommandOutput { .. } => Ok(()),
        IpcMessage::Error { message } => Err(message),
        other => Err(unexpected_ipc_response(
            "executing remote SFTP command",
            other,
        )),
    }
}

/// Get MIME type from file extension
fn get_mime_type(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();

    match ext.as_str() {
        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        // Text
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        // Code
        "js" | "mjs" | "cjs" => "text/javascript",
        "ts" | "tsx" => "text/typescript",
        "jsx" => "text/javascript",
        "json" | "jsonc" | "json5" => "application/json",
        "xml" => "application/xml",
        "yaml" | "yml" => "text/yaml",
        "toml" => "text/toml",
        // Programming languages
        "py" => "text/x-python",
        "rs" => "text/x-rust",
        "go" => "text/x-go",
        "java" => "text/x-java",
        "c" | "h" => "text/x-c",
        "cpp" | "hpp" | "cc" | "cxx" => "text/x-c++",
        "cs" => "text/x-csharp",
        "rb" => "text/x-ruby",
        "php" => "text/x-php",
        "swift" => "text/x-swift",
        "kt" | "kts" => "text/x-kotlin",
        "scala" => "text/x-scala",
        // Shell
        "sh" | "bash" | "zsh" => "text/x-shellscript",
        "ps1" | "psm1" => "text/x-powershell",
        "bat" | "cmd" => "text/x-batch",
        // Config
        "ini" | "conf" | "cfg" => "text/plain",
        "env" => "text/plain",
        // Documents
        "pdf" => "application/pdf",
        // Default
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Escape shell special characters in a path (used only for compress/extract commands)
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Execute an SSH command via exec channel and return the output as a string.
/// Only used for compress/extract which have no SFTP protocol equivalent.
async fn execute_ssh_command(
    session: &std::sync::Arc<crate::session::Session>,
    command: &str,
) -> Result<String, String> {
    session
        .exec_command(command)
        .await
        .map_err(|e| format!("Failed to execute command: {}", e))
}

fn build_compress_command(request: &SftpCompressRequest) -> Result<String, String> {
    if request.paths.is_empty() {
        return Err("No files to compress".to_string());
    }

    match request.format.as_str() {
        "tar.gz" | "tgz" => {
            let escaped_paths: Vec<String> = request
                .paths
                .iter()
                .map(|p| {
                    let name = p.rsplit('/').next().unwrap_or(p);
                    shell_escape(name)
                })
                .collect();

            let first_path = &request.paths[0];
            let parent_dir = if let Some(pos) = first_path.rfind('/') {
                &first_path[..pos]
            } else {
                "."
            };

            Ok(format!(
                "cd {} && tar -czf {} {}",
                shell_escape(parent_dir),
                shell_escape(&request.archive_path),
                escaped_paths.join(" ")
            ))
        }
        "zip" => {
            let escaped_paths: Vec<String> =
                request.paths.iter().map(|p| shell_escape(p)).collect();

            Ok(format!(
                "which zip > /dev/null 2>&1 && zip -r {} {} || (echo 'zip not found, falling back to tar.gz' && tar -czf {} {})",
                shell_escape(&request.archive_path),
                escaped_paths.join(" "),
                shell_escape(&request.archive_path.replace(".zip", ".tar.gz")),
                escaped_paths.join(" ")
            ))
        }
        _ => Err(format!(
            "Unsupported compression format: {}",
            request.format
        )),
    }
}

fn build_extract_command(request: &SftpExtractRequest) -> Result<String, String> {
    let archive = &request.archive_path;
    let dest = &request.destination_path;

    let command = if archive.ends_with(".tar.gz") || archive.ends_with(".tgz") {
        format!(
            "cd {} && tar -xzf {}",
            shell_escape(dest),
            shell_escape(archive)
        )
    } else if archive.ends_with(".tar.bz2") || archive.ends_with(".tbz2") {
        format!(
            "cd {} && tar -xjf {}",
            shell_escape(dest),
            shell_escape(archive)
        )
    } else if archive.ends_with(".tar.xz") || archive.ends_with(".txz") {
        format!(
            "cd {} && tar -xJf {}",
            shell_escape(dest),
            shell_escape(archive)
        )
    } else if archive.ends_with(".tar") {
        format!(
            "cd {} && tar -xf {}",
            shell_escape(dest),
            shell_escape(archive)
        )
    } else if archive.ends_with(".zip") {
        format!(
            "cd {} && unzip -o {}",
            shell_escape(dest),
            shell_escape(archive)
        )
    } else if archive.ends_with(".gz") && !archive.ends_with(".tar.gz") {
        format!(
            "cd {} && gunzip -c {} > {}",
            shell_escape(dest),
            shell_escape(archive),
            shell_escape(
                archive
                    .replace(".gz", "")
                    .rsplit('/')
                    .next()
                    .unwrap_or("output")
            )
        )
    } else if archive.ends_with(".bz2") && !archive.ends_with(".tar.bz2") {
        format!(
            "cd {} && bunzip2 -c {} > {}",
            shell_escape(dest),
            shell_escape(archive),
            shell_escape(
                archive
                    .replace(".bz2", "")
                    .rsplit('/')
                    .next()
                    .unwrap_or("output")
            )
        )
    } else if archive.ends_with(".xz") && !archive.ends_with(".tar.xz") {
        format!(
            "cd {} && xz -dc {} > {}",
            shell_escape(dest),
            shell_escape(archive),
            shell_escape(
                archive
                    .replace(".xz", "")
                    .rsplit('/')
                    .next()
                    .unwrap_or("output")
            )
        )
    } else if archive.ends_with(".7z") {
        format!(
            "cd {} && 7z x {}",
            shell_escape(dest),
            shell_escape(archive)
        )
    } else if archive.ends_with(".rar") {
        format!(
            "cd {} && unrar x {}",
            shell_escape(dest),
            shell_escape(archive)
        )
    } else {
        return Err(format!(
            "Unsupported archive format: {}. Supported formats: tar.gz, tar.bz2, tar.xz, tar, zip, gz, bz2, xz, 7z, rar",
            archive
        ));
    };

    Ok(command)
}

// ==================== Tauri Commands ====================

/// Initialize SFTP for a session.
///
/// For SSH sessions, this opens the SFTP subsystem on the existing SSH connection.
/// For local sessions, this just sets up path tracking state.
#[tauri::command]
pub async fn sftp_init(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpInitRequest,
) -> Result<bool, String> {
    info!(
        "[SFTP] Initializing SFTP for session {}",
        request.session_id
    );

    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        // Local session - no real SFTP needed, just path tracking
        let initial_path = local_home_dir().to_string_lossy().to_string();
        let data = Arc::new(TokioMutex::new(SftpSessionData {
            sftp: None,
            home_dir: initial_path.clone(),
            current_path: initial_path,
            connected: true,
        }));
        let mut sessions = sftp_state.sessions.write().await;
        sessions.insert(request.session_id.clone(), data);
        info!("[SFTP] Local session initialized: {}", request.session_id);
        return Ok(true);
    }

    if access_state.is_remote() {
        return match ipc_send(IpcMessage::SftpInit {
            session_id: request.session_id,
        })
        .await?
        {
            IpcMessage::Ok => Ok(true),
            IpcMessage::Error { message } => Err(message),
            other => Err(unexpected_ipc_response("initializing remote SFTP", other)),
        };
    }

    // SSH session - open real SFTP subsystem
    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    let sftp = session
        .open_sftp_session()
        .await
        .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;

    // Get the home directory via canonicalize(".")
    let home_dir = sftp
        .canonicalize(".")
        .await
        .map_err(|e| format!("Failed to resolve home directory: {}", e))?;

    info!(
        "[SFTP] SFTP session initialized for {}, home={}",
        request.session_id, home_dir
    );

    let data = Arc::new(TokioMutex::new(SftpSessionData {
        sftp: Some(sftp),
        home_dir: home_dir.clone(),
        current_path: home_dir,
        connected: true,
    }));

    let mut sessions = sftp_state.sessions.write().await;
    if sessions.contains_key(&request.session_id) {
        info!(
            "[SFTP] Replacing existing SFTP session for {}",
            request.session_id
        );
    }
    sessions.insert(request.session_id.clone(), data);

    Ok(true)
}

/// List directory contents via SFTP.
#[tauri::command]
pub async fn sftp_list_dir(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpListDirRequest,
) -> Result<Vec<SftpEntry>, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let requested_path = if request.path.trim().is_empty() {
            current_path.clone()
        } else {
            request.path.clone()
        };

        let resolved = resolve_local_path(&requested_path, &current_path)?;
        let metadata = std::fs::metadata(&resolved)
            .map_err(|e| format!("Failed to access path {}: {}", resolved.display(), e))?;

        if !metadata.is_dir() {
            return Err(format!("Not a directory: {}", resolved.display()));
        }

        let mut entries = Vec::new();
        let read_dir = std::fs::read_dir(&resolved)
            .map_err(|e| format!("Failed to read directory {}: {}", resolved.display(), e))?;

        for item in read_dir {
            let item = item.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = item.path();
            let meta = item
                .metadata()
                .map_err(|e| format!("Failed to read metadata for {}: {}", path.display(), e))?;

            let modified_at = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let name = item.file_name().to_string_lossy().to_string();
            let is_directory = meta.is_dir();
            let size = if is_directory { 0 } else { meta.len() };

            entries.push(SftpEntry {
                name,
                path: path.to_string_lossy().to_string(),
                is_directory,
                size,
                modified_at,
                permissions: local_permissions_string(&meta),
            });
        }

        entries.sort_by(|a, b| {
            b.is_directory
                .cmp(&a.is_directory)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        set_local_current_path(sftp_state.inner(), &request.session_id, &resolved).await;
        return Ok(entries);
    }

    if access_state.is_remote() {
        return match ipc_send(IpcMessage::SftpListDir {
            session_id: request.session_id,
            path: request.path,
            preserve_cwd: false,
        })
        .await?
        {
            IpcMessage::SftpEntries { entries } => Ok(entries),
            IpcMessage::Error { message } => Err(message),
            other => Err(unexpected_ipc_response(
                "listing remote SFTP directory",
                other,
            )),
        };
    }

    // SSH session - use real SFTP protocol
    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    let path = if request.path.is_empty() {
        if guard.current_path.is_empty() {
            guard.home_dir.clone()
        } else {
            guard.current_path.clone()
        }
    } else {
        resolve_remote_path(&request.path, &guard.home_dir, &guard.current_path)
    };

    debug!("[SFTP] Listing directory: {}", path);

    let dir_entries = sftp
        .read_dir(&path)
        .await
        .map_err(|e| format!("Failed to list directory {}: {}", path, e))?;

    let mut entries = Vec::new();
    for entry in dir_entries {
        let name = entry.file_name();
        // Explicitly filter . and .. (defensive, in case library doesn't skip them)
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

        let perms = metadata.permissions();
        let permissions = format!("{}{}", if is_directory { "d" } else { "-" }, perms);

        let entry_path = if path.ends_with('/') {
            format!("{}{}", path, name)
        } else {
            format!("{}/{}", path, name)
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

    // Update current path in state
    drop(guard);
    {
        let mut guard = sftp_data.lock().await;
        guard.current_path = path;
    }

    Ok(entries)
}

/// Download a file from the remote server via SFTP.
#[tauri::command]
pub async fn sftp_download_file(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpDownloadRequest,
) -> Result<TransferProgress, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let source_path = resolve_local_path(&request.remote_path, &current_path)?;
        let target_path = resolve_local_path(&request.local_path, &current_path)?;

        let content = std::fs::read(&source_path).map_err(|e| {
            format!(
                "Failed to read local source file {}: {}",
                source_path.display(),
                e
            )
        })?;

        if let Some(parent) = target_path.parent() {
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

        std::fs::write(&target_path, &content).map_err(|e| {
            format!(
                "Failed to write local target file {}: {}",
                target_path.display(),
                e
            )
        })?;

        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut progress = TransferProgress::new(filename, content.len() as u64);
        progress.transferred_bytes = content.len() as u64;
        progress.status = TransferStatus::Completed;
        return Ok(progress);
    }

    if access_state.is_remote() {
        return match ipc_send(IpcMessage::SftpDownloadFile {
            session_id: request.session_id,
            remote_path: request.remote_path,
            local_path: request.local_path,
        })
        .await?
        {
            IpcMessage::SftpTransfer { progress } => Ok(progress),
            IpcMessage::Error { message } => Err(message),
            other => Err(unexpected_ipc_response(
                "downloading remote SFTP file",
                other,
            )),
        };
    }

    // SSH session - use real SFTP protocol for binary-safe download
    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    let remote_path =
        resolve_remote_path(&request.remote_path, &guard.home_dir, &guard.current_path);
    info!(
        "[SFTP] Downloading {} -> {}",
        remote_path, request.local_path
    );

    // Read the entire file via SFTP (binary-safe)
    let content = sftp
        .read(&remote_path)
        .await
        .map_err(|e| format!("Failed to read remote file {}: {}", remote_path, e))?;

    // Write to local file
    if let Some(parent) = Path::new(&request.local_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory: {}", e))?;
        }
    }

    std::fs::write(&request.local_path, &content)
        .map_err(|e| format!("Failed to write local file: {}", e))?;

    let filename = std::path::Path::new(&request.remote_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut progress = TransferProgress::new(filename, content.len() as u64);
    progress.transferred_bytes = content.len() as u64;
    progress.status = TransferStatus::Completed;

    info!(
        "[SFTP] Download complete: {} ({} bytes)",
        remote_path,
        content.len()
    );
    Ok(progress)
}

/// Upload a file to the remote server via SFTP.
#[tauri::command]
pub async fn sftp_upload_file(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpUploadRequest,
) -> Result<TransferProgress, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let source_path = resolve_local_path(&request.local_path, &current_path)?;
        let mut target_path = resolve_local_path(&request.remote_path, &current_path)?;

        let content = std::fs::read(&source_path).map_err(|e| {
            format!(
                "Failed to read local source file {}: {}",
                source_path.display(),
                e
            )
        })?;

        if target_path.is_dir() {
            let filename = source_path.file_name().ok_or_else(|| {
                format!(
                    "Failed to determine filename for local source file {}",
                    source_path.display()
                )
            })?;
            target_path = target_path.join(filename);
        }

        if let Some(parent) = target_path.parent() {
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

        std::fs::write(&target_path, &content).map_err(|e| {
            format!(
                "Failed to write local target file {}: {}",
                target_path.display(),
                e
            )
        })?;

        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut progress = TransferProgress::new(filename, content.len() as u64);
        progress.transferred_bytes = content.len() as u64;
        progress.status = TransferStatus::Completed;
        return Ok(progress);
    }

    if access_state.is_remote() {
        return match ipc_send(IpcMessage::SftpUploadFile {
            session_id: request.session_id,
            local_path: request.local_path,
            remote_path: request.remote_path,
        })
        .await?
        {
            IpcMessage::SftpTransfer { progress } => Ok(progress),
            IpcMessage::Error { message } => Err(message),
            other => Err(unexpected_ipc_response("uploading remote SFTP file", other)),
        };
    }

    // SSH session - use real SFTP protocol for binary-safe upload
    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    // Read local file
    let content = std::fs::read(&request.local_path)
        .map_err(|e| format!("Failed to read local file: {}", e))?;

    let filename = std::path::Path::new(&request.local_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let remote_path =
        resolve_remote_path(&request.remote_path, &guard.home_dir, &guard.current_path);
    let remote_path = resolve_remote_upload_path(sftp, &remote_path, &filename).await;
    info!(
        "[SFTP] Uploading {} -> {} ({} bytes)",
        request.local_path,
        remote_path,
        content.len()
    );

    // Write via SFTP (binary-safe, no size limits)
    write_remote_file(sftp, &remote_path, &content).await?;

    let mut progress = TransferProgress::new(filename, content.len() as u64);
    progress.transferred_bytes = content.len() as u64;
    progress.status = TransferStatus::Completed;

    info!(
        "[SFTP] Upload complete: {} ({} bytes)",
        remote_path,
        content.len()
    );
    Ok(progress)
}

/// Upload or sync a local directory to a remote directory.
#[tauri::command]
pub async fn sftp_upload_directory(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpUploadDirectoryRequest,
) -> Result<DirectoryTransferSummary, String> {
    let mode = request.mode.unwrap_or(DirectoryTransferMode::Upload);
    let options = effective_directory_transfer_options(
        request.excluded_paths.clone(),
        request.respect_gitignore,
        request.delete_extra.unwrap_or(false),
    );

    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let source_path = resolve_local_path(&request.local_path, &current_path)?;
        let target_path = resolve_local_path(&request.remote_path, &current_path)?;

        return transfer_directory_to_local(&source_path, &target_path, mode, &options);
    }

    if access_state.is_remote() {
        return match ipc_send(IpcMessage::SftpUploadDirectory {
            session_id: request.session_id,
            local_path: request.local_path,
            remote_path: request.remote_path,
            mode,
            delete_extra: options.delete_extra,
            respect_gitignore: Some(options.respect_gitignore),
            excluded_paths: options.excluded_paths,
        })
        .await?
        {
            IpcMessage::SftpDirectoryTransfer { summary } => Ok(summary),
            IpcMessage::Error { message } => Err(message),
            other => Err(unexpected_ipc_response(
                "uploading remote SFTP directory",
                other,
            )),
        };
    }

    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    let local_path = PathBuf::from(&request.local_path);
    let remote_path =
        resolve_remote_path(&request.remote_path, &guard.home_dir, &guard.current_path);

    info!(
        "[SFTP] {} directory {} -> {}",
        mode.as_str(),
        local_path.display(),
        remote_path
    );

    transfer_directory_to_sftp(sftp, &local_path, &remote_path, mode, &options).await
}

#[tauri::command]
pub async fn sftp_get_upload_ignore_config() -> Result<UploadIgnoreConfig, String> {
    load_upload_ignore_config().or_else(|_| Ok(default_upload_ignore_config()))
}

#[tauri::command]
pub async fn sftp_save_upload_ignore_config(
    config: UploadIgnoreConfig,
) -> Result<UploadIgnoreConfig, String> {
    save_upload_ignore_config(config)
}

/// Create a directory on the remote server via SFTP.
#[tauri::command]
pub async fn sftp_mkdir(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpMkdirRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let target_path = resolve_local_path(&request.path, &current_path)?;
        std::fs::create_dir_all(&target_path).map_err(|e| {
            format!(
                "Failed to create directory {}: {}",
                target_path.display(),
                e
            )
        })?;
        return Ok(());
    }

    if access_state.is_remote() {
        return expect_ipc_ok(
            "creating remote SFTP directory",
            ipc_send(IpcMessage::SftpMkdir {
                session_id: request.session_id,
                path: request.path,
            })
            .await?,
        );
    }

    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    let path = resolve_remote_path(&request.path, &guard.home_dir, &guard.current_path);
    info!("[SFTP] Creating directory: {}", path);

    sftp_mkdir_recursive(sftp, &path).await?;

    Ok(())
}

/// Delete a file or directory on the remote server via SFTP.
#[tauri::command]
pub async fn sftp_delete(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpDeleteRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let target_path = resolve_local_path(&request.path, &current_path)?;

        let metadata = std::fs::metadata(&target_path)
            .map_err(|e| format!("Failed to access {}: {}", target_path.display(), e))?;

        if metadata.is_dir() {
            if request.recursive.unwrap_or(false) {
                std::fs::remove_dir_all(&target_path).map_err(|e| {
                    format!(
                        "Failed to remove directory {}: {}",
                        target_path.display(),
                        e
                    )
                })?;
            } else {
                std::fs::remove_dir(&target_path).map_err(|e| {
                    format!(
                        "Failed to remove directory {} (set recursive=true for non-empty dirs): {}",
                        target_path.display(),
                        e
                    )
                })?;
            }
        } else {
            std::fs::remove_file(&target_path)
                .map_err(|e| format!("Failed to remove file {}: {}", target_path.display(), e))?;
        }

        return Ok(());
    }

    if access_state.is_remote() {
        return expect_ipc_ok(
            "deleting remote SFTP path",
            ipc_send(IpcMessage::SftpDelete {
                session_id: request.session_id,
                path: request.path,
                recursive: request.recursive.unwrap_or(false),
            })
            .await?,
        );
    }

    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    let path = resolve_remote_path(&request.path, &guard.home_dir, &guard.current_path);
    info!("[SFTP] Deleting: {}", path);

    // Check if it's a directory by trying metadata
    let meta = sftp
        .metadata(&path)
        .await
        .map_err(|e| format!("Failed to stat {}: {}", path, e))?;

    if meta.is_dir() {
        if request.recursive.unwrap_or(false) {
            sftp_remove_recursive(sftp, &path, 0).await?;
        } else {
            sftp.remove_dir(&path)
                .await
                .map_err(|e| format!("Failed to remove directory {}: {}", path, e))?;
        }
    } else {
        sftp.remove_file(&path)
            .await
            .map_err(|e| format!("Failed to remove file {}: {}", path, e))?;
    }

    Ok(())
}

/// Rename or move a file/directory on the remote server via SFTP.
#[tauri::command]
pub async fn sftp_rename(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpRenameRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let old_path = resolve_local_path(&request.old_path, &current_path)?;
        let new_path = resolve_local_path(&request.new_path, &current_path)?;

        if let Some(parent) = new_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        "Failed to create target parent directory {}: {}",
                        parent.display(),
                        e
                    )
                })?;
            }
        }

        std::fs::rename(&old_path, &new_path).map_err(|e| {
            format!(
                "Failed to rename {} to {}: {}",
                old_path.display(),
                new_path.display(),
                e
            )
        })?;

        return Ok(());
    }

    if access_state.is_remote() {
        return expect_ipc_ok(
            "renaming remote SFTP path",
            ipc_send(IpcMessage::SftpRename {
                session_id: request.session_id,
                old_path: request.old_path,
                new_path: request.new_path,
            })
            .await?,
        );
    }

    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    let old_path = resolve_remote_path(&request.old_path, &guard.home_dir, &guard.current_path);
    let new_path = resolve_remote_path(&request.new_path, &guard.home_dir, &guard.current_path);
    info!("[SFTP] Renaming {} -> {}", old_path, new_path);

    sftp.rename(&old_path, &new_path)
        .await
        .map_err(|e| format!("Failed to rename {} to {}: {}", old_path, new_path, e))?;

    Ok(())
}

/// Get the current working directory for a session.
#[tauri::command]
pub async fn sftp_pwd(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpPwdRequest,
) -> Result<String, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        set_local_current_path(sftp_state.inner(), &request.session_id, Path::new(&path)).await;
        return Ok(path);
    }

    if access_state.is_remote() {
        return match ipc_send(IpcMessage::SftpPwd {
            session_id: request.session_id,
        })
        .await?
        {
            IpcMessage::SftpPath { path } => Ok(path),
            IpcMessage::Error { message } => Err(message),
            other => Err(unexpected_ipc_response("reading remote SFTP path", other)),
        };
    }

    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;

    // Return the tracked current working directory
    let path = if guard.current_path.is_empty() {
        guard.home_dir.clone()
    } else {
        guard.current_path.clone()
    };

    Ok(path)
}

/// Get file/directory information via SFTP.
#[tauri::command]
pub async fn sftp_stat(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpStatRequest,
) -> Result<SftpEntry, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let resolved = resolve_local_path(&request.path, &current_path)?;
        let metadata = std::fs::metadata(&resolved)
            .map_err(|e| format!("Failed to stat {}: {}", resolved.display(), e))?;

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let name = resolved
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| resolved.to_string_lossy().to_string());

        return Ok(SftpEntry {
            name,
            path: resolved.to_string_lossy().to_string(),
            is_directory: metadata.is_dir(),
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified_at,
            permissions: local_permissions_string(&metadata),
        });
    }

    if access_state.is_remote() {
        return match ipc_send(IpcMessage::SftpStat {
            session_id: request.session_id,
            path: request.path,
        })
        .await?
        {
            IpcMessage::SftpStatResult { entry } => Ok(entry),
            IpcMessage::Error { message } => Err(message),
            other => Err(unexpected_ipc_response("stating remote SFTP path", other)),
        };
    }

    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    let path = resolve_remote_path(&request.path, &guard.home_dir, &guard.current_path);

    let metadata = sftp
        .metadata(&path)
        .await
        .map_err(|e| format!("Failed to stat {}: {}", path, e))?;

    let is_directory = metadata.is_dir();
    let size = if is_directory { 0 } else { metadata.len() };

    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let perms = metadata.permissions();
    let permissions = format!("{}{}", if is_directory { "d" } else { "-" }, perms);

    let name = path.rsplit('/').next().unwrap_or(&path).to_string();

    Ok(SftpEntry {
        name,
        path,
        is_directory,
        size,
        modified_at,
        permissions,
    })
}

/// Read file content for preview via SFTP.
#[tauri::command]
pub async fn sftp_read_file(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpReadFileRequest,
) -> Result<SftpFileContent, String> {
    let as_binary = request.as_binary.unwrap_or(false);
    let default_max = if as_binary {
        10 * 1024 * 1024
    } else {
        1024 * 1024
    }; // 10MB for binary, 1MB for text
    let max_size = request.max_size.unwrap_or(default_max);

    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let resolved = resolve_local_path(&request.path, &current_path)?;

        let metadata = std::fs::metadata(&resolved)
            .map_err(|e| format!("Failed to access file {}: {}", resolved.display(), e))?;

        if metadata.is_dir() {
            return Err(format!(
                "Cannot read directory as file: {}",
                resolved.display()
            ));
        }

        let file_size = metadata.len();
        let mime_type = get_mime_type(&resolved.to_string_lossy());

        if file_size > max_size && as_binary {
            return Err(format!(
                "File too large for preview: {} bytes (max: {} bytes)",
                file_size, max_size
            ));
        }

        let (content, truncated) = if as_binary {
            let bytes = std::fs::read(&resolved)
                .map_err(|e| format!("Failed to read binary file {}: {}", resolved.display(), e))?;
            let base64_content =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
            (base64_content, false)
        } else {
            let bytes = std::fs::read(&resolved)
                .map_err(|e| format!("Failed to read text file {}: {}", resolved.display(), e))?;
            let read_size = std::cmp::min(file_size, max_size) as usize;
            let truncated = read_size < bytes.len();
            let content = String::from_utf8_lossy(&bytes[..read_size]).to_string();
            (content, truncated)
        };

        return Ok(SftpFileContent {
            content,
            is_binary: as_binary,
            size: file_size,
            truncated,
            mime_type,
        });
    }

    if access_state.is_remote() {
        return match ipc_send(IpcMessage::SftpReadFile {
            session_id: request.session_id,
            path: request.path,
            max_size: request.max_size,
            as_binary: request.as_binary,
        })
        .await?
        {
            IpcMessage::SftpFileContent { content } => Ok(content),
            IpcMessage::Error { message } => Err(message),
            other => Err(unexpected_ipc_response("reading remote SFTP file", other)),
        };
    }

    // SSH session - use real SFTP protocol (binary-safe)
    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    let path = resolve_remote_path(&request.path, &guard.home_dir, &guard.current_path);
    let mime_type = get_mime_type(&path);

    // Get file size via metadata
    let metadata = sftp
        .metadata(&path)
        .await
        .map_err(|e| format!("Failed to stat {}: {}", path, e))?;
    let file_size = metadata.len();

    if file_size > max_size && as_binary {
        return Err(format!(
            "File too large for preview: {} bytes (max: {} bytes)",
            file_size, max_size
        ));
    }

    // Read the file via SFTP (binary-safe!)
    let bytes = sftp
        .read(&path)
        .await
        .map_err(|e| format!("Failed to read file {}: {}", path, e))?;

    let (content, truncated) = if as_binary {
        let base64_content =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
        (base64_content, false)
    } else {
        let read_size = std::cmp::min(file_size as usize, max_size as usize);
        let actual_size = std::cmp::min(read_size, bytes.len());
        let truncated = actual_size < bytes.len();
        let content = String::from_utf8_lossy(&bytes[..actual_size]).to_string();
        (content, truncated)
    };

    Ok(SftpFileContent {
        content,
        is_binary: as_binary,
        size: file_size,
        truncated,
        mime_type,
    })
}

/// Write content to a remote file via SFTP (for text editing).
#[tauri::command]
pub async fn sftp_write_file(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpWriteFileRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let resolved = resolve_local_path(&request.path, &current_path)?;

        std::fs::write(&resolved, &request.content)
            .map_err(|e| format!("Failed to write file {}: {}", resolved.display(), e))?;

        return Ok(());
    }

    if access_state.is_remote() {
        return expect_ipc_ok(
            "writing remote SFTP file",
            ipc_send(IpcMessage::SftpWriteFile {
                session_id: request.session_id,
                path: request.path,
                content: request.content,
            })
            .await?,
        );
    }

    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let guard = sftp_data.lock().await;
    let sftp = guard
        .sftp
        .as_ref()
        .ok_or("SFTP not initialized for this SSH session")?;

    let path = resolve_remote_path(&request.path, &guard.home_dir, &guard.current_path);
    info!(
        "[SFTP] Writing file: {} ({} bytes)",
        path,
        request.content.len()
    );

    write_remote_file(sftp, &path, request.content.as_bytes()).await?;

    Ok(())
}

/// Compress files/directories into an archive.
/// Uses SSH exec channel (not SFTP) since this requires shell commands.
#[tauri::command]
pub async fn sftp_compress(
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpCompressRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        return Err("Local session does not support sftp_compress yet".to_string());
    }

    let command = build_compress_command(&request)?;

    if access_state.is_remote() {
        return remote_exec_command(&request.session_id, command).await;
    }

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    execute_ssh_command(&session, &command).await?;

    Ok(())
}

/// Extract an archive to a destination directory.
/// Uses SSH exec channel (not SFTP) since this requires shell commands.
#[tauri::command]
pub async fn sftp_extract(
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    request: SftpExtractRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        return Err("Local session does not support sftp_extract yet".to_string());
    }

    let command = build_extract_command(&request)?;

    if access_state.is_remote() {
        return remote_exec_command(&request.session_id, command).await;
    }

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    execute_ssh_command(&session, &command).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("test"), "'test'");
        assert_eq!(shell_escape("test file"), "'test file'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }
}
