//! SFTP commands for file transfer operations.
//!
//! This module provides Tauri commands for SFTP file operations including
//! directory listing, file upload/download, and file management.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tauri::State;
use tokio::sync::RwLock;

use crate::local_shell::LocalShellManager;
use crate::sftp::{FileInfo, TransferProgress, TransferStatus};
use crate::session::SessionManager;

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
#[derive(Debug, Clone, Serialize)]
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

/// SFTP session state shared across commands
pub struct SftpState {
    /// Maps session_id to SftpSession data
    pub sessions: Arc<RwLock<HashMap<String, SftpSessionData>>>,
}

impl SftpState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for SftpState {
    fn default() -> Self {
        Self::new()
    }
}

/// Data for an active SFTP session
pub struct SftpSessionData {
    /// Current working directory on the remote server
    pub current_path: String,
    /// Whether the SFTP subsystem is connected
    pub connected: bool,
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

impl From<FileInfo> for SftpEntry {
    fn from(info: FileInfo) -> Self {
        Self {
            name: info.name,
            path: info.path,
            is_directory: info.is_dir,
            size: info.size,
            modified_at: info.modified,
            permissions: info.permissions,
        }
    }
}

/// Initialize SFTP for a session.
///
/// This command establishes the SFTP subsystem on an existing SSH connection.
#[tauri::command]
pub async fn sftp_init(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    _session_manager: State<'_, Arc<SessionManager>>,
    request: SftpInitRequest,
) -> Result<bool, String> {
    let mut sessions = sftp_state.sessions.write().await;

    let initial_path = if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        local_home_dir().to_string_lossy().to_string()
    } else {
        "/".to_string()
    };

    // Initialize SFTP session data
    sessions.insert(
        request.session_id.clone(),
        SftpSessionData {
            current_path: initial_path,
            connected: true,
        },
    );

    Ok(true)
}

/// List directory contents via SFTP.
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `path` - The remote directory path to list
///
/// # Returns
///
/// A vector of SftpEntry objects representing files and directories.
#[tauri::command]
pub async fn sftp_list_dir(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpListDirRequest,
) -> Result<Vec<SftpEntry>, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_current_path(sftp_state.inner(), &request.session_id).await;
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

        set_current_path(sftp_state.inner(), &request.session_id, &resolved).await;
        return Ok(entries);
    }

    // Verify session exists
    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    // For now, we'll execute ls command over SSH and parse the output
    // In a full implementation, this would use the SFTP subsystem directly
    let path = if request.path.is_empty() { "~".to_string() } else { request.path };

    // Update current path in state
    {
        let mut sessions = sftp_state.sessions.write().await;
        if let Some(data) = sessions.get_mut(&request.session_id) {
            data.current_path = path.clone();
        }
    }

    // Execute ls command to get directory listing
    // Format: ls -la --time-style=+%s <path>
    // This gives us: permissions, links, owner, group, size, timestamp, name
    let ls_command = format!(
        "ls -la --time-style=+%s {} 2>/dev/null || ls -la {} 2>/dev/null",
        shell_escape(&path),
        shell_escape(&path)
    );

    // Send the command and collect output
    let output = execute_ssh_command(&session, &ls_command).await?;

    // Parse ls output into SftpEntry objects
    let entries = parse_ls_output(&output, &path);

    Ok(entries)
}

/// Download a file from the remote server.
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `remote_path` - The path to the remote file
/// * `local_path` - The local path to save the file
#[tauri::command]
pub async fn sftp_download_file(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpDownloadRequest,
) -> Result<TransferProgress, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_current_path(sftp_state.inner(), &request.session_id).await;
        let source_path = resolve_local_path(&request.remote_path, &current_path)?;
        let target_path = resolve_local_path(&request.local_path, &current_path)?;

        let content = std::fs::read(&source_path)
            .map_err(|e| format!("Failed to read local source file {}: {}", source_path.display(), e))?;

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

        std::fs::write(&target_path, &content)
            .map_err(|e| format!("Failed to write local target file {}: {}", target_path.display(), e))?;

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

    // Verify session exists
    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    // Use cat to read file content and write to local file
    let cat_command = format!("cat {}", shell_escape(&request.remote_path));
    let content = execute_ssh_command_binary(&session, &cat_command).await?;

    // Write to local file
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

    Ok(progress)
}

/// Upload a file to the remote server.
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `local_path` - The path to the local file
/// * `remote_path` - The remote path to save the file
#[tauri::command]
pub async fn sftp_upload_file(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpUploadRequest,
) -> Result<TransferProgress, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_current_path(sftp_state.inner(), &request.session_id).await;
        let source_path = resolve_local_path(&request.local_path, &current_path)?;
        let target_path = resolve_local_path(&request.remote_path, &current_path)?;

        let content = std::fs::read(&source_path)
            .map_err(|e| format!("Failed to read local source file {}: {}", source_path.display(), e))?;

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

        std::fs::write(&target_path, &content)
            .map_err(|e| format!("Failed to write local target file {}: {}", target_path.display(), e))?;

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

    // Verify session exists
    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    // Read local file
    let content = std::fs::read(&request.local_path)
        .map_err(|e| format!("Failed to read local file: {}", e))?;

    let filename = std::path::Path::new(&request.local_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Upload using base64 encoding to handle binary files
    let base64_content = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &content);
    let upload_command = format!(
        "echo '{}' | base64 -d > {}",
        base64_content,
        shell_escape(&request.remote_path)
    );

    execute_ssh_command(&session, &upload_command).await?;

    let mut progress = TransferProgress::new(filename, content.len() as u64);
    progress.transferred_bytes = content.len() as u64;
    progress.status = TransferStatus::Completed;

    Ok(progress)
}

/// Create a directory on the remote server.
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `path` - The path of the directory to create
#[tauri::command]
pub async fn sftp_mkdir(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpMkdirRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_current_path(sftp_state.inner(), &request.session_id).await;
        let target_path = resolve_local_path(&request.path, &current_path)?;
        std::fs::create_dir_all(&target_path)
            .map_err(|e| format!("Failed to create directory {}: {}", target_path.display(), e))?;
        return Ok(());
    }

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    let mkdir_command = format!("mkdir -p {}", shell_escape(&request.path));
    execute_ssh_command(&session, &mkdir_command).await?;

    Ok(())
}

/// Delete a file or directory on the remote server.
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `path` - The path to delete
/// * `recursive` - Whether to delete directories recursively
#[tauri::command]
pub async fn sftp_delete(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpDeleteRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_current_path(sftp_state.inner(), &request.session_id).await;
        let target_path = resolve_local_path(&request.path, &current_path)?;

        let metadata = std::fs::metadata(&target_path)
            .map_err(|e| format!("Failed to access {}: {}", target_path.display(), e))?;

        if metadata.is_dir() {
            if request.recursive.unwrap_or(false) {
                std::fs::remove_dir_all(&target_path).map_err(|e| {
                    format!("Failed to remove directory {}: {}", target_path.display(), e)
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

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    let rm_command = if request.recursive.unwrap_or(false) {
        format!("rm -rf {}", shell_escape(&request.path))
    } else {
        format!("rm -f {}", shell_escape(&request.path))
    };

    execute_ssh_command(&session, &rm_command).await?;

    Ok(())
}

/// Rename or move a file/directory on the remote server.
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `old_path` - The current path
/// * `new_path` - The new path
#[tauri::command]
pub async fn sftp_rename(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpRenameRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_current_path(sftp_state.inner(), &request.session_id).await;
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

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    let mv_command = format!("mv {} {}", shell_escape(&request.old_path), shell_escape(&request.new_path));
    execute_ssh_command(&session, &mv_command).await?;

    Ok(())
}

/// Get the current working directory for a session.
#[tauri::command]
pub async fn sftp_pwd(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpPwdRequest,
) -> Result<String, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let path = get_current_path(sftp_state.inner(), &request.session_id).await;
        set_current_path(
            sftp_state.inner(),
            &request.session_id,
            Path::new(&path),
        ).await;
        return Ok(path);
    }

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    let pwd_command = "pwd";
    let output = execute_ssh_command(&session, pwd_command).await?;

    let path = output.trim().to_string();

    // Update state
    {
        let mut sessions = sftp_state.sessions.write().await;
        if let Some(data) = sessions.get_mut(&request.session_id) {
            data.current_path = path.clone();
        }
    }

    Ok(path)
}

/// Get file/directory information.
#[tauri::command]
pub async fn sftp_stat(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpStatRequest,
) -> Result<SftpEntry, String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_current_path(sftp_state.inner(), &request.session_id).await;
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

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    let stat_command = format!(
        "ls -ld --time-style=+%s {} 2>/dev/null || ls -ld {}",
        shell_escape(&request.path),
        shell_escape(&request.path)
    );
    let output = execute_ssh_command(&session, &stat_command).await?;

    let entries = parse_ls_output(&output, &request.path);
    entries
        .into_iter()
        .next()
        .ok_or_else(|| format!("File not found: {}", request.path))
}

/// Read file content for preview.
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `path` - The path to the remote file
/// * `max_size` - Maximum bytes to read (default: 1MB for text, 10MB for binary)
/// * `as_binary` - Whether to return base64 encoded content
///
/// # Returns
///
/// SftpFileContent with the file content and metadata.
#[tauri::command]
pub async fn sftp_read_file(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpReadFileRequest,
) -> Result<SftpFileContent, String> {
    let as_binary = request.as_binary.unwrap_or(false);
    let default_max = if as_binary { 10 * 1024 * 1024 } else { 1024 * 1024 }; // 10MB for binary, 1MB for text
    let max_size = request.max_size.unwrap_or(default_max);

    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_current_path(sftp_state.inner(), &request.session_id).await;
        let resolved = resolve_local_path(&request.path, &current_path)?;

        let metadata = std::fs::metadata(&resolved)
            .map_err(|e| format!("Failed to access file {}: {}", resolved.display(), e))?;

        if metadata.is_dir() {
            return Err(format!("Cannot read directory as file: {}", resolved.display()));
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
            let base64_content = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
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

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    // Get file size first
    let stat_command = format!("stat -c %s {} 2>/dev/null || wc -c < {}",
        shell_escape(&request.path),
        shell_escape(&request.path)
    );
    let size_output = execute_ssh_command(&session, &stat_command).await?;
    let file_size: u64 = size_output.trim().parse().unwrap_or(0);

    // Determine MIME type from extension
    let mime_type = get_mime_type(&request.path);

    // Check if file is too large
    if file_size > max_size
        && as_binary {
            // For binary files that are too large, return error
            return Err(format!(
                "File too large for preview: {} bytes (max: {} bytes)",
                file_size, max_size
            ));
        }
        // For text files, we'll read partial content

    let (content, truncated) = if as_binary {
        // Read binary content with base64 encoding
        let cat_command = format!("cat {} | base64", shell_escape(&request.path));
        let output = execute_ssh_command(&session, &cat_command).await?;
        // Remove newlines from base64 output
        let base64_content = output.replace(['\n', '\r'], "");
        (base64_content, false)
    } else {
        // Read text content, optionally truncated
        let read_size = std::cmp::min(file_size, max_size);
        let cat_command = if read_size < file_size {
            format!("head -c {} {}", read_size, shell_escape(&request.path))
        } else {
            format!("cat {}", shell_escape(&request.path))
        };
        let output = execute_ssh_command(&session, &cat_command).await?;
        (output, read_size < file_size)
    };

    Ok(SftpFileContent {
        content,
        is_binary: as_binary,
        size: file_size,
        truncated,
        mime_type,
    })
}

/// Write content to a remote file (for text editing).
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `path` - The path to the remote file
/// * `content` - The text content to write
///
/// # Returns
///
/// Ok(()) on success, or error message on failure.
#[tauri::command]
pub async fn sftp_write_file(
    sftp_state: State<'_, Arc<SftpState>>,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpWriteFileRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        let current_path = get_current_path(sftp_state.inner(), &request.session_id).await;
        let resolved = resolve_local_path(&request.path, &current_path)?;

        std::fs::write(&resolved, &request.content)
            .map_err(|e| format!("Failed to write file {}: {}", resolved.display(), e))?;

        return Ok(());
    }

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    // Upload text content using base64 encoding to handle special characters safely
    let base64_content = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        request.content.as_bytes(),
    );
    let write_command = format!(
        "echo '{}' | base64 -d > {}",
        base64_content,
        shell_escape(&request.path)
    );

    execute_ssh_command(&session, &write_command).await?;

    Ok(())
}

/// Compress files/directories into an archive.
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `paths` - List of file/directory paths to compress
/// * `archive_path` - Output archive path
/// * `format` - Compression format: "tar.gz" or "zip"
///
/// # Returns
///
/// Ok(()) on success, or error message on failure.
#[tauri::command]
pub async fn sftp_compress(
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpCompressRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        return Err("Local session does not support sftp_compress yet".to_string());
    }

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    if request.paths.is_empty() {
        return Err("No files to compress".to_string());
    }

    // Build the compression command based on format
    let command = match request.format.as_str() {
        "tar.gz" | "tgz" => {
            // Use tar with gzip compression
            // We need to handle paths that might be in different directories
            // by using the -C option to change to parent directory first
            let escaped_paths: Vec<String> = request.paths.iter()
                .map(|p| {
                    // Get just the filename/dirname for tar
                    let name = p.rsplit('/').next().unwrap_or(p);
                    shell_escape(name)
                })
                .collect();

            // Get the parent directory of the first file for -C option
            let first_path = &request.paths[0];
            let parent_dir = if let Some(pos) = first_path.rfind('/') {
                &first_path[..pos]
            } else {
                "."
            };

            format!(
                "cd {} && tar -czf {} {}",
                shell_escape(parent_dir),
                shell_escape(&request.archive_path),
                escaped_paths.join(" ")
            )
        }
        "zip" => {
            // Use zip for compression
            let escaped_paths: Vec<String> = request.paths.iter()
                .map(|p| shell_escape(p))
                .collect();

            // Check if zip is available, use it; otherwise try creating a tar.gz
            format!(
                "which zip > /dev/null 2>&1 && zip -r {} {} || (echo 'zip not found, falling back to tar.gz' && tar -czf {} {})",
                shell_escape(&request.archive_path),
                escaped_paths.join(" "),
                shell_escape(&request.archive_path.replace(".zip", ".tar.gz")),
                escaped_paths.join(" ")
            )
        }
        _ => {
            return Err(format!("Unsupported compression format: {}", request.format));
        }
    };

    execute_ssh_command(&session, &command).await?;

    Ok(())
}

/// Extract an archive to a destination directory.
///
/// # Arguments
///
/// * `session_id` - The SSH session ID to use
/// * `archive_path` - Path to the archive file
/// * `destination_path` - Destination directory for extraction
///
/// # Returns
///
/// Ok(()) on success, or error message on failure.
#[tauri::command]
pub async fn sftp_extract(
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
    session_manager: State<'_, Arc<SessionManager>>,
    request: SftpExtractRequest,
) -> Result<(), String> {
    if is_local_session(local_shell_manager.inner(), &request.session_id).await {
        return Err("Local session does not support sftp_extract yet".to_string());
    }

    let session = session_manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", request.session_id))?;

    let archive = &request.archive_path;
    let dest = &request.destination_path;

    // Determine the archive type and build the extraction command
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
        // Single file gzip
        format!(
            "cd {} && gunzip -c {} > {}",
            shell_escape(dest),
            shell_escape(archive),
            shell_escape(archive.replace(".gz", "").rsplit('/').next().unwrap_or("output"))
        )
    } else if archive.ends_with(".bz2") && !archive.ends_with(".tar.bz2") {
        // Single file bzip2
        format!(
            "cd {} && bunzip2 -c {} > {}",
            shell_escape(dest),
            shell_escape(archive),
            shell_escape(archive.replace(".bz2", "").rsplit('/').next().unwrap_or("output"))
        )
    } else if archive.ends_with(".xz") && !archive.ends_with(".tar.xz") {
        // Single file xz
        format!(
            "cd {} && xz -dc {} > {}",
            shell_escape(dest),
            shell_escape(archive),
            shell_escape(archive.replace(".xz", "").rsplit('/').next().unwrap_or("output"))
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

    execute_ssh_command(&session, &command).await?;

    Ok(())
}

// Helper functions

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
    } else if let Some(rest) = input.strip_prefix("~/").or_else(|| input.strip_prefix("~\\")) {
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

async fn get_current_path(sftp_state: &Arc<SftpState>, session_id: &str) -> String {
    let sessions = sftp_state.sessions.read().await;
    sessions
        .get(session_id)
        .map(|s| s.current_path.clone())
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| local_home_dir().to_string_lossy().to_string())
}

async fn set_current_path(sftp_state: &Arc<SftpState>, session_id: &str, path: &Path) {
    let mut sessions = sftp_state.sessions.write().await;
    if let Some(data) = sessions.get_mut(session_id) {
        data.current_path = path.to_string_lossy().to_string();
    }
}

/// Get MIME type from file extension
fn get_mime_type(path: &str) -> String {
    let ext = path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();

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
    }.to_string()
}

/// Escape shell special characters in a path
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Execute an SSH command via exec channel and return the output as a string.
/// This uses a dedicated exec channel, so output won't appear in the terminal.
async fn execute_ssh_command(
    session: &std::sync::Arc<crate::session::Session>,
    command: &str,
) -> Result<String, String> {
    session
        .exec_command(command)
        .await
        .map_err(|e| format!("Failed to execute command: {}", e))
}

/// Execute an SSH command and return the output as bytes
async fn execute_ssh_command_binary(
    session: &std::sync::Arc<crate::session::Session>,
    command: &str,
) -> Result<Vec<u8>, String> {
    let output = execute_ssh_command(session, command).await?;
    Ok(output.into_bytes())
}

/// Parse ls -la output into SftpEntry objects
fn parse_ls_output(output: &str, base_path: &str) -> Vec<SftpEntry> {
    let mut entries = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("total ") {
            continue;
        }

        // Parse ls -la output format:
        // drwxr-xr-x 2 user group 4096 1609459200 filename
        // or standard format without timestamp:
        // drwxr-xr-x 2 user group 4096 Jan 1 12:00 filename
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 {
            continue;
        }

        let permissions = parts[0];
        let is_directory = permissions.starts_with('d');
        let is_link = permissions.starts_with('l');

        // Try to parse size
        let size: u64 = parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);

        // Try to parse timestamp (Unix timestamp format)
        let modified_at: i64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);

        // Get filename (everything after the timestamp)
        let name_start_index = if modified_at > 0 { 6 } else { 8 };
        let name = if parts.len() > name_start_index {
            parts[name_start_index..].join(" ")
        } else if parts.len() > 5 {
            parts[parts.len() - 1].to_string()
        } else {
            continue;
        };

        // Handle symlinks (name -> target)
        let name = if is_link {
            name.split(" -> ").next().unwrap_or(&name).to_string()
        } else {
            name
        };

        // Skip . and .. entries
        if name == "." || name == ".." {
            continue;
        }

        let path = if base_path.ends_with('/') {
            format!("{}{}", base_path, name)
        } else {
            format!("{}/{}", base_path, name)
        };

        entries.push(SftpEntry {
            name,
            path,
            is_directory: is_directory || is_link,
            size,
            modified_at,
            permissions: permissions.to_string(),
        });
    }

    entries
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

    #[test]
    fn test_parse_ls_output_empty() {
        let entries = parse_ls_output("", "/");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_ls_output_total_line() {
        let entries = parse_ls_output("total 24", "/");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_ls_output_directory() {
        let output = "drwxr-xr-x 2 user group 4096 1609459200 testdir";
        let entries = parse_ls_output(output, "/home/user");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "testdir");
        assert!(entries[0].is_directory);
        assert_eq!(entries[0].path, "/home/user/testdir");
    }

    #[test]
    fn test_parse_ls_output_file() {
        let output = "-rw-r--r-- 1 user group 1234 1609459200 test.txt";
        let entries = parse_ls_output(output, "/home/user");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "test.txt");
        assert!(!entries[0].is_directory);
        assert_eq!(entries[0].size, 1234);
    }

    #[test]
    fn test_parse_ls_output_skip_dots() {
        let output = "drwxr-xr-x 2 user group 4096 1609459200 .\ndrwxr-xr-x 2 user group 4096 1609459200 ..";
        let entries = parse_ls_output(output, "/home/user");
        assert!(entries.is_empty());
    }
}
