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
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tauri::State;
use tokio::io::AsyncReadExt;
use tokio::sync::{Mutex as TokioMutex, RwLock};

use crate::commands::session::SessionAccessState;
use crate::ipc::{IpcClient, IpcMessage};
use crate::local_shell::LocalShellManager;
use crate::session::SessionManager;
use crate::sftp::helpers::{
    resolve_remote_path, resolve_remote_upload_path, sftp_delete_path, sftp_mkdir_recursive,
    write_remote_file,
};
use crate::sftp::sync::{download_remote_file_streaming, upload_local_file_streaming};
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
    /// The real SFTP session (only for SSH sessions, None for local).
    ///
    /// Wrapped in an `Arc` so commands can clone the handle out of the session
    /// mutex, release the lock, and then run long transfers. Concurrent
    /// requests on one `SftpSession` are safe: russh-sftp allocates request
    /// IDs atomically and routes responses per request, and per-command
    /// operations (list, rename, transfer, ...) each open their own remote
    /// file handles.
    pub sftp: Option<Arc<SftpSession>>,
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

fn ensure_native_path_transfer_supported() -> Result<(), String> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        Err("Path-based file transfer is unavailable on mobile until native document pickers are implemented".to_string())
    } else {
        Ok(())
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

/// Narrow-scope preparation for a remote file operation.
///
/// Pure function over the session data so callers can hold the session mutex
/// only for the duration of this call (clone the `Arc<SftpSession>` handle and
/// resolve the effective remote path) and then run arbitrarily long transfers
/// without blocking other SFTP commands on the same session.
pub(crate) fn prepare_remote_file_access(
    data: &SftpSessionData,
    requested_path: &str,
) -> Result<(Arc<SftpSession>, String), String> {
    let sftp = data
        .sftp
        .clone()
        .ok_or("SFTP not initialized for this SSH session")?;
    let path = resolve_remote_path(requested_path, &data.home_dir, &data.current_path);
    Ok((sftp, path))
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
        stdin: None,
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
pub(super) fn get_mime_type(path: &str) -> String {
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
        // Video
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "ogv" => "video/ogg",
        "3gp" => "video/3gpp",
        // Audio
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
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
        // Archives
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" | "tgz" => "application/gzip",
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

/// Quoting prevents shell injection; a ./ prefix separately prevents utility
/// option injection (including tar checkpoint options and a lone '-' stream).
fn archive_operand(path: &str) -> Result<String, String> {
    if path.is_empty() || path.contains('\0') {
        return Err("Archive paths must not be empty or contain NUL".into());
    }
    Ok(if path.starts_with('-') {
        format!("./{path}")
    } else {
        path.to_string()
    })
}

fn build_compress_command(request: &SftpCompressRequest) -> Result<String, String> {
    if request.paths.is_empty() {
        return Err("No files to compress".into());
    }
    let mut parent: Option<&str> = None;
    let mut names = Vec::new();
    for source in &request.paths {
        let source = source.trim_end_matches('/');
        let (directory, name) = source
            .rsplit_once('/')
            .map(|(dir, name)| (if dir.is_empty() { "/" } else { dir }, name))
            .unwrap_or((".", source));
        crate::sftp::helpers::validate_remote_entry_name(name)?;
        if parent.is_some_and(|old| old != directory) {
            return Err(
                "Select files from the same directory to avoid ambiguous archive members".into(),
            );
        }
        parent = Some(directory);
        names.push(shell_escape(&format!("./{name}")));
    }
    let directory = shell_escape(&archive_operand(parent.unwrap_or("."))?);
    let archive = shell_escape(&archive_operand(&request.archive_path)?);
    let names = names.join(" ");
    match request.format.as_str() {
        "tar.gz" | "tgz" => Ok(format!("cd {directory} && tar -czf {archive} {names}")),
        // A failed ZIP command must not silently produce a different format.
        "zip" => Ok(format!("cd {directory} && zip -r {archive} {names}")),
        _ => Err(format!(
            "Unsupported compression format: {}",
            request.format
        )),
    }
}

#[cfg(test)]
mod archive_safety_tests {
    use super::*;
    #[test]
    fn archive_names_cannot_be_utility_options_and_zip_has_no_fallback() {
        let mut request = SftpCompressRequest {
            session_id: "test".into(),
            paths: vec!["/tmp/--checkpoint-action=exec=command".into()],
            archive_path: "-archive.tar.gz".into(),
            format: "tar.gz".into(),
        };
        let command = build_compress_command(&request).unwrap();
        assert!(command.contains("'./--checkpoint-action=exec=command'"));
        assert!(command.contains("'./-archive.tar.gz'"));
        request.format = "zip".into();
        assert!(!build_compress_command(&request).unwrap().contains("||"));
        request.paths.push("/another/file".into());
        assert!(build_compress_command(&request).is_err());
        assert!(archive_operand("").is_err());
        assert!(archive_operand("bad\0path").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn real_tar_preserves_quoted_unicode_and_dash_prefixed_members() {
        let directory = tempfile::tempdir().unwrap();
        let name = "-中文 ' quoted.txt";
        let source = directory.path().join(name);
        std::fs::write(&source, b"archive-content").unwrap();
        let archive = directory.path().join("archive.tar.gz");
        let request = SftpCompressRequest {
            session_id: "test".into(),
            paths: vec![source.to_string_lossy().into_owned()],
            archive_path: archive.to_string_lossy().into_owned(),
            format: "tar.gz".into(),
        };
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(build_compress_command(&request).unwrap())
            .status()
            .unwrap();
        assert!(status.success());
        let output = std::process::Command::new("tar")
            .arg("-xOzf")
            .arg(archive)
            .arg(format!("./{name}"))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"archive-content");
    }
}

fn build_extract_command(request: &SftpExtractRequest) -> Result<String, String> {
    let archive_path = archive_operand(&request.archive_path)?;
    let destination = archive_operand(&request.destination_path)?;
    let archive = &archive_path;
    let dest = &destination;

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

    if access_state.is_remote_session(&request.session_id).await {
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
        sftp: Some(Arc::new(sftp)),
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

        // Directory scans can touch thousands of entries; keep them off the
        // async workers (same pattern as commands/local_files.rs).
        let scan_path = resolved.clone();
        let entries =
            tauri::async_runtime::spawn_blocking(move || -> Result<Vec<SftpEntry>, String> {
                let resolved = scan_path;
                let metadata = std::fs::metadata(&resolved)
                    .map_err(|e| format!("Failed to access path {}: {}", resolved.display(), e))?;

                if !metadata.is_dir() {
                    return Err(format!("Not a directory: {}", resolved.display()));
                }

                let mut entries = Vec::new();
                let read_dir = std::fs::read_dir(&resolved).map_err(|e| {
                    format!("Failed to read directory {}: {}", resolved.display(), e)
                })?;

                for item in read_dir {
                    let item =
                        item.map_err(|e| format!("Failed to read directory entry: {}", e))?;
                    let path = item.path();
                    let meta = item.metadata().map_err(|e| {
                        format!("Failed to read metadata for {}: {}", path.display(), e)
                    })?;

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

                Ok(entries)
            })
            .await
            .map_err(|e| format!("Local directory scan failed: {}", e))??;

        set_local_current_path(sftp_state.inner(), &request.session_id, &resolved).await;
        return Ok(entries);
    }

    if access_state.is_remote_session(&request.session_id).await {
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
    ensure_native_path_transfer_supported()?;

    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let source_path = resolve_local_path(&request.remote_path, &current_path)?;
        let target_path = resolve_local_path(&request.local_path, &current_path)?;

        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Local-to-local copies stream via std::fs::copy on a blocking thread:
        // no whole-file buffer, no blocked async worker.
        let transferred = tauri::async_runtime::spawn_blocking(move || -> Result<u64, String> {
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

            std::fs::copy(&source_path, &target_path).map_err(|e| {
                format!(
                    "Failed to copy {} -> {}: {}",
                    source_path.display(),
                    target_path.display(),
                    e
                )
            })
        })
        .await
        .map_err(|e| format!("Local file copy failed: {}", e))??;

        let mut progress = TransferProgress::new(filename, transferred);
        progress.transferred_bytes = transferred;
        progress.status = TransferStatus::Completed;
        return Ok(progress);
    }

    if access_state.is_remote_session(&request.session_id).await {
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

    // SSH session - use real SFTP protocol for binary-safe download.
    // The session mutex is held only to clone the SFTP handle and resolve the
    // path; the streaming transfer runs without the lock so listings, renames
    // and other commands on this session stay responsive during big transfers.
    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let (sftp, remote_path) = {
        let guard = sftp_data.lock().await;
        prepare_remote_file_access(&guard, &request.remote_path)?
    };
    info!(
        "[SFTP] Downloading {} -> {}",
        remote_path, request.local_path
    );

    // Stream to disk in fixed-size chunks (binary-safe, bounded memory)
    let transferred =
        download_remote_file_streaming(&sftp, &remote_path, Path::new(&request.local_path)).await?;

    let filename = std::path::Path::new(&request.remote_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut progress = TransferProgress::new(filename, transferred);
    progress.transferred_bytes = transferred;
    progress.status = TransferStatus::Completed;

    info!(
        "[SFTP] Download complete: {} ({} bytes)",
        remote_path, transferred
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
    ensure_native_path_transfer_supported()?;

    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_local_current_path(sftp_state.inner(), &request.session_id).await;
        let source_path = resolve_local_path(&request.local_path, &current_path)?;
        let mut target_path = resolve_local_path(&request.remote_path, &current_path)?;

        if target_path.is_dir() {
            let filename = source_path.file_name().ok_or_else(|| {
                format!(
                    "Failed to determine filename for local source file {}",
                    source_path.display()
                )
            })?;
            target_path = target_path.join(filename);
        }

        let filename = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Local-to-local copies stream via std::fs::copy on a blocking thread.
        let transferred = tauri::async_runtime::spawn_blocking(move || -> Result<u64, String> {
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

            std::fs::copy(&source_path, &target_path).map_err(|e| {
                format!(
                    "Failed to copy {} -> {}: {}",
                    source_path.display(),
                    target_path.display(),
                    e
                )
            })
        })
        .await
        .map_err(|e| format!("Local file copy failed: {}", e))??;

        let mut progress = TransferProgress::new(filename, transferred);
        progress.transferred_bytes = transferred;
        progress.status = TransferStatus::Completed;
        return Ok(progress);
    }

    if access_state.is_remote_session(&request.session_id).await {
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

    // SSH session - use real SFTP protocol for binary-safe upload.
    // Same narrow lock scope as the download path: grab the session handle and
    // resolve paths, then stream without holding the session mutex.
    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let (sftp, resolved_remote_path) = {
        let guard = sftp_data.lock().await;
        prepare_remote_file_access(&guard, &request.remote_path)?
    };

    let filename = std::path::Path::new(&request.local_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let remote_path = resolve_remote_upload_path(&sftp, &resolved_remote_path, &filename).await;

    let file_size = tokio::fs::metadata(&request.local_path)
        .await
        .map(|meta| meta.len())
        .map_err(|e| format!("Failed to read local file: {}", e))?;
    info!(
        "[SFTP] Uploading {} -> {} ({} bytes)",
        request.local_path, remote_path, file_size
    );

    // Stream in fixed-size chunks (binary-safe, no size limits)
    let transferred =
        upload_local_file_streaming(&sftp, Path::new(&request.local_path), &remote_path).await?;

    let mut progress = TransferProgress::new(filename, transferred);
    progress.transferred_bytes = transferred;
    progress.status = TransferStatus::Completed;

    info!(
        "[SFTP] Upload complete: {} ({} bytes)",
        remote_path, transferred
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
    ensure_native_path_transfer_supported()?;

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

        // Local directory transfers walk and copy the whole tree synchronously.
        return tauri::async_runtime::spawn_blocking(move || {
            transfer_directory_to_local(&source_path, &target_path, mode, &options)
        })
        .await
        .map_err(|e| format!("Local directory transfer failed: {}", e))?;
    }

    if access_state.is_remote_session(&request.session_id).await {
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

    // Narrow lock scope: clone the handle and resolve the root, then run the
    // whole directory transfer without holding the session mutex.
    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let (sftp, remote_path) = {
        let guard = sftp_data.lock().await;
        prepare_remote_file_access(&guard, &request.remote_path)?
    };

    let local_path = PathBuf::from(&request.local_path);

    info!(
        "[SFTP] {} directory {} -> {}",
        mode.as_str(),
        local_path.display(),
        remote_path
    );

    transfer_directory_to_sftp(&sftp, &local_path, &remote_path, mode, &options).await
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
        tauri::async_runtime::spawn_blocking(move || {
            std::fs::create_dir_all(&target_path).map_err(|e| {
                format!(
                    "Failed to create directory {}: {}",
                    target_path.display(),
                    e
                )
            })
        })
        .await
        .map_err(|e| format!("Local directory creation failed: {}", e))??;
        return Ok(());
    }

    if access_state.is_remote_session(&request.session_id).await {
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
        let recursive = request.recursive.unwrap_or(false);

        // Recursive deletes can walk and unlink thousands of entries.
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            let metadata = std::fs::metadata(&target_path)
                .map_err(|e| format!("Failed to access {}: {}", target_path.display(), e))?;

            if metadata.is_dir() {
                if recursive {
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
                std::fs::remove_file(&target_path).map_err(|e| {
                    format!("Failed to remove file {}: {}", target_path.display(), e)
                })?;
            }

            Ok(())
        })
        .await
        .map_err(|e| format!("Local delete failed: {}", e))??;

        return Ok(());
    }

    if access_state.is_remote_session(&request.session_id).await {
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

    sftp_delete_path(sftp, &path, request.recursive.unwrap_or(false)).await
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

        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
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
            })
        })
        .await
        .map_err(|e| format!("Local rename failed: {}", e))??;

        return Ok(());
    }

    if access_state.is_remote_session(&request.session_id).await {
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

    if access_state.is_remote_session(&request.session_id).await {
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

        // Preview reads can pull up to 10 MB into memory; run them on a
        // blocking thread like commands/local_files.rs does.
        let content =
            tauri::async_runtime::spawn_blocking(move || -> Result<SftpFileContent, String> {
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

                let read_limit = if as_binary {
                    file_size
                } else {
                    max_size.min(file_size)
                };
                let mut file = std::fs::File::open(&resolved)
                    .map_err(|e| format!("Failed to open file {}: {}", resolved.display(), e))?;
                let mut bytes = Vec::with_capacity(read_limit.min(usize::MAX as u64) as usize);
                file.by_ref()
                    .take(read_limit)
                    .read_to_end(&mut bytes)
                    .map_err(|e| format!("Failed to read file {}: {}", resolved.display(), e))?;

                let (content, truncated) = if as_binary {
                    let base64_content =
                        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
                    (base64_content, false)
                } else {
                    let truncated = file_size > bytes.len() as u64;
                    let content = String::from_utf8_lossy(&bytes).to_string();
                    (content, truncated)
                };

                Ok(SftpFileContent {
                    content,
                    is_binary: as_binary,
                    size: file_size,
                    truncated,
                    mime_type,
                })
            })
            .await
            .map_err(|e| format!("Local file read failed: {}", e))??;

        return Ok(content);
    }

    if access_state.is_remote_session(&request.session_id).await {
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

    // Read only the requested preview window so a huge remote text file does
    // not have to be transferred and allocated before it can be truncated.
    let read_limit = if as_binary {
        file_size
    } else {
        max_size.min(file_size)
    };
    let file = sftp
        .open(&path)
        .await
        .map_err(|e| format!("Failed to open file {}: {}", path, e))?;
    let mut bytes = Vec::with_capacity(read_limit.min(usize::MAX as u64) as usize);
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("Failed to read file {}: {}", path, e))?;

    let (content, truncated) = if as_binary {
        let base64_content =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
        (base64_content, false)
    } else {
        let truncated = file_size > bytes.len() as u64;
        let content = String::from_utf8_lossy(&bytes).to_string();
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

        tauri::async_runtime::spawn_blocking(move || {
            std::fs::write(&resolved, &request.content)
                .map_err(|e| format!("Failed to write file {}: {}", resolved.display(), e))
        })
        .await
        .map_err(|e| format!("Local file write failed: {}", e))??;

        return Ok(());
    }

    if access_state.is_remote_session(&request.session_id).await {
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

    // Narrow lock scope: editor saves can carry megabytes of content.
    let sftp_data = get_sftp_data(sftp_state.inner(), &request.session_id).await?;
    let (sftp, path) = {
        let guard = sftp_data.lock().await;
        prepare_remote_file_access(&guard, &request.path)?
    };
    info!(
        "[SFTP] Writing file: {} ({} bytes)",
        path,
        request.content.len()
    );

    write_remote_file(&sftp, &path, request.content.as_bytes()).await?;

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

    if access_state.is_remote_session(&request.session_id).await {
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

    if access_state.is_remote_session(&request.session_id).await {
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
    fn native_path_transfer_support_matches_the_target() {
        let result = ensure_native_path_transfer_supported();
        if cfg!(any(target_os = "android", target_os = "ios")) {
            assert!(result.is_err());
        } else {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("test"), "'test'");
        assert_eq!(shell_escape("test file"), "'test file'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    // ----- prepare_remote_file_access (narrow lock-scope state prep) -----

    /// Minimal in-memory SFTP server: only the built-in INIT handshake is
    /// needed, every operation returns "unimplemented".
    struct NopSftpHandler;

    impl russh_sftp::server::Handler for NopSftpHandler {
        type Error = russh_sftp::protocol::StatusCode;

        fn unimplemented(&self) -> Self::Error {
            russh_sftp::protocol::StatusCode::OpUnsupported
        }
    }

    async fn nop_sftp_session() -> SftpSession {
        let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        russh_sftp::server::run(server_stream, NopSftpHandler).await;
        SftpSession::new(client_stream)
            .await
            .expect("initialize nop sftp session")
    }

    #[tokio::test]
    async fn prepare_fails_when_sftp_is_missing() {
        let data = SftpSessionData {
            sftp: None,
            home_dir: "/home/user".to_string(),
            current_path: "/home/user".to_string(),
            connected: true,
        };

        let error = match prepare_remote_file_access(&data, "~/file.txt") {
            Ok(_) => panic!("must fail without a session"),
            Err(error) => error,
        };
        assert!(error.contains("SFTP not initialized"));
    }

    #[tokio::test]
    async fn prepare_clones_session_and_resolves_path_without_extra_locks() {
        let data = SftpSessionData {
            sftp: Some(Arc::new(nop_sftp_session().await)),
            home_dir: "/home/user".to_string(),
            current_path: "/var/log".to_string(),
            connected: true,
        };

        // Absolute path passes through untouched.
        let (sftp, path) =
            prepare_remote_file_access(&data, "/etc/hosts").expect("absolute path resolves");
        assert_eq!(path, "/etc/hosts");

        // Home-relative and cwd-relative paths resolve like SFTP commands do.
        let (_, tilde_path) =
            prepare_remote_file_access(&data, "~/file.txt").expect("tilde path resolves");
        assert_eq!(tilde_path, "/home/user/file.txt");
        let (_, relative_path) =
            prepare_remote_file_access(&data, "app.log").expect("relative path resolves");
        assert_eq!(relative_path, "/var/log/app.log");

        // The session is shared by clone, not re-created per command, so the
        // transfer and any other command multiplex over the same SFTP session.
        let (again, _) =
            prepare_remote_file_access(&data, "/etc/hosts").expect("second prep succeeds");
        assert!(Arc::ptr_eq(&sftp, &again));
    }
}
