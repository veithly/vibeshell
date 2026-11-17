//! Ignore-aware directory upload and sync helpers.

use directories::ProjectDirs;
use ignore::{
    gitignore::GitignoreBuilder,
    overrides::{Override, OverrideBuilder},
    WalkBuilder,
};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::sftp::helpers::{
    join_remote_child, sftp_mkdir_recursive, validate_remote_entry_name, MAX_RECURSIVE_DEPTH,
};

/// Buffer size for streaming SFTP transfers (256 KiB).
///
/// russh-sftp additionally caps every protocol READ/WRITE request at 255 KiB
/// (or a lower server-advertised limit via `limits@openssh.com`), so a chunk
/// may still be split into multiple protocol requests. The point of this
/// constant is that peak memory per transfer stays at one chunk, never the
/// whole file.
pub const SFTP_TRANSFER_CHUNK_SIZE: usize = 256 * 1024;

/// Bytes to request for the next chunk of a streaming transfer.
///
/// Pass `Some(remaining)` when the total transfer size is known so the final
/// chunk is right-sized; pass `None` when the size is unknown. Pure helper so
/// the boundary behaviour is unit-testable.
pub fn next_transfer_chunk_size(remaining: Option<u64>) -> usize {
    const MAX: u64 = SFTP_TRANSFER_CHUNK_SIZE as u64;
    match remaining {
        Some(remaining) if remaining < MAX => remaining as usize,
        _ => SFTP_TRANSFER_CHUNK_SIZE,
    }
}

fn remote_parent_dir(path: &str) -> Option<String> {
    let path = path.trim().trim_end_matches('/');
    if path.is_empty() || path == "/" {
        return None;
    }

    match path.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(index) => Some(path[..index].to_string()),
        None => None,
    }
}

/// Open a remote file for streaming writes.
///
/// Mirrors the open strategy of `helpers::write_remote_file_with_options`:
/// try the CREATE|TRUNCATE|WRITE open first, and only on failure prepare the
/// parent directory (some servers reject metadata/MKDIR preflights) and retry.
async fn open_remote_file_for_write(
    sftp: &SftpSession,
    remote_path: &str,
) -> Result<russh_sftp::client::fs::File, String> {
    let open_flags = OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE;

    match sftp.open_with_flags(remote_path, open_flags).await {
        Ok(file) => Ok(file),
        Err(first_error) => {
            let Some(parent) =
                remote_parent_dir(remote_path).filter(|parent| parent != "/" && parent != ".")
            else {
                return Err(format!(
                    "Failed to create remote file {}: {}",
                    remote_path, first_error
                ));
            };

            sftp_mkdir_recursive(sftp, &parent).await.map_err(|parent_error| {
                format!(
                    "Failed to create remote file {} ({}); failed to prepare parent directory {}: {}",
                    remote_path, first_error, parent, parent_error
                )
            })?;

            sftp.open_with_flags(remote_path, open_flags)
                .await
                .map_err(|retry_error| {
                    format!(
                        "Failed to create remote file {} after preparing parent directory {}: {}",
                        remote_path, parent, retry_error
                    )
                })
        }
    }
}

/// Stream a remote file to a local path in fixed-size chunks.
///
/// Peak memory is one [`SFTP_TRANSFER_CHUNK_SIZE`] buffer regardless of file
/// size. Returns the number of bytes transferred.
pub async fn download_remote_file_streaming(
    sftp: &SftpSession,
    remote_path: &str,
    local_path: &Path,
) -> Result<u64, String> {
    let mut remote = sftp
        .open(remote_path)
        .await
        .map_err(|e| format!("Failed to open remote file {}: {}", remote_path, e))?;

    // fstat may be unsupported by exotic servers; treat unknown size as None.
    let total_size = remote.metadata().await.ok().and_then(|meta| meta.size);

    if let Some(parent) = local_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                format!(
                    "Failed to create parent directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }
    }

    let mut local = tokio::fs::File::create(local_path).await.map_err(|e| {
        format!(
            "Failed to create local file {}: {}",
            local_path.display(),
            e
        )
    })?;

    let mut buffer = vec![0u8; SFTP_TRANSFER_CHUNK_SIZE];
    let mut transferred: u64 = 0;
    loop {
        let chunk = next_transfer_chunk_size(total_size.map(|t| t.saturating_sub(transferred)));
        if chunk == 0 {
            break;
        }
        let read = remote
            .read(&mut buffer[..chunk])
            .await
            .map_err(|e| format!("Failed to read remote file {}: {}", remote_path, e))?;
        if read == 0 {
            break;
        }
        local
            .write_all(&buffer[..read])
            .await
            .map_err(|e| format!("Failed to write local file {}: {}", local_path.display(), e))?;
        transferred += read as u64;
        if total_size == Some(transferred) {
            break;
        }
    }

    // Tokio file writes may still be pending after write_all returns.
    // Do not report completion (or hide a delayed write error) before flushing.
    local
        .flush()
        .await
        .map_err(|e| format!("Failed to flush local file {}: {}", local_path.display(), e))?;

    remote
        .shutdown()
        .await
        .map_err(|e| format!("Failed to close remote file {}: {}", remote_path, e))?;

    Ok(transferred)
}

/// Stream a local file to a remote path in fixed-size chunks.
///
/// Peak memory is one [`SFTP_TRANSFER_CHUNK_SIZE`] buffer regardless of file
/// size. Returns the number of bytes transferred.
pub async fn upload_local_file_streaming(
    sftp: &SftpSession,
    local_path: &Path,
    remote_path: &str,
) -> Result<u64, String> {
    let mut local = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| format!("Failed to open local file {}: {}", local_path.display(), e))?;

    let mut remote = open_remote_file_for_write(sftp, remote_path).await?;

    let mut buffer = vec![0u8; SFTP_TRANSFER_CHUNK_SIZE];
    let mut transferred: u64 = 0;
    loop {
        let read = local
            .read(&mut buffer)
            .await
            .map_err(|e| format!("Failed to read local file {}: {}", local_path.display(), e))?;
        if read == 0 {
            break;
        }
        remote
            .write_all(&buffer[..read])
            .await
            .map_err(|e| format!("Failed to write remote file {}: {}", remote_path, e))?;
        transferred += read as u64;
    }

    remote
        .shutdown()
        .await
        .map_err(|e| format!("Failed to close remote file {}: {}", remote_path, e))?;

    Ok(transferred)
}

const DEFAULT_UPLOAD_EXCLUDES: &[&str] = &[
    "node_modules/",
    ".git/",
    ".svn/",
    ".hg/",
    "target/",
    ".next/",
    ".nuxt/",
    ".turbo/",
    ".cache/",
    "coverage/",
    "__pycache__/",
    ".pytest_cache/",
    ".mypy_cache/",
    ".ruff_cache/",
    ".venv/",
    "venv/",
    "env/",
];

/// Progress information for a file transfer operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    /// Unique identifier for this transfer.
    pub id: String,
    /// Name of the file being transferred.
    pub filename: String,
    /// Total size of the file in bytes.
    pub total_bytes: u64,
    /// Number of bytes transferred so far.
    pub transferred_bytes: u64,
    /// Current status of the transfer.
    pub status: TransferStatus,
}

/// Status of a file transfer operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    /// Transfer is queued but not yet started.
    Pending,
    /// Transfer is currently in progress.
    InProgress,
    /// Transfer completed successfully.
    Completed,
    /// Transfer failed due to an error.
    Failed,
    /// Transfer was cancelled by the user.
    Cancelled,
}

impl TransferProgress {
    /// Creates a new transfer progress tracker.
    ///
    /// # Arguments
    ///
    /// * `filename` - The name of the file being transferred
    /// * `total_bytes` - The total size of the file in bytes
    ///
    /// # Returns
    ///
    /// A new `TransferProgress` instance in `Pending` status.
    pub fn new(filename: String, total_bytes: u64) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            filename,
            total_bytes,
            transferred_bytes: 0,
            status: TransferStatus::Pending,
        }
    }

    /// Returns the transfer progress as a percentage (0.0 to 100.0).
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            return 100.0;
        }
        (self.transferred_bytes as f64 / self.total_bytes as f64) * 100.0
    }

    /// Checks if the transfer is complete.
    pub fn is_complete(&self) -> bool {
        self.status == TransferStatus::Completed
    }

    /// Checks if the transfer has failed.
    pub fn is_failed(&self) -> bool {
        self.status == TransferStatus::Failed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryTransferMode {
    Upload,
    Sync,
}

impl DirectoryTransferMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Upload => "upload",
            Self::Sync => "sync",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadIgnoreConfig {
    pub excluded_paths: Vec<String>,
    pub respect_gitignore: bool,
}

impl Default for UploadIgnoreConfig {
    fn default() -> Self {
        Self {
            excluded_paths: DEFAULT_UPLOAD_EXCLUDES
                .iter()
                .map(|value| value.to_string())
                .collect(),
            respect_gitignore: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectoryTransferOptions {
    pub excluded_paths: Vec<String>,
    pub respect_gitignore: bool,
    pub delete_extra: bool,
}

#[derive(Debug, Clone)]
pub struct LocalTransferEntry {
    pub local_path: PathBuf,
    pub relative_path: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct DirectoryTransferPlan {
    pub root_path: PathBuf,
    pub root_name: String,
    pub directories: Vec<String>,
    pub files: Vec<LocalTransferEntry>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryTransferSummary {
    pub mode: String,
    pub local_root: String,
    pub remote_root: String,
    pub directories_total: usize,
    pub files_total: usize,
    pub created_directories: usize,
    pub uploaded_files: usize,
    pub skipped_files: usize,
    pub deleted_entries: usize,
    pub transferred_bytes: u64,
}

pub fn default_upload_ignore_config() -> UploadIgnoreConfig {
    UploadIgnoreConfig::default()
}

pub fn load_upload_ignore_config() -> Result<UploadIgnoreConfig, String> {
    let path = upload_ignore_config_path()?;
    if !path.exists() {
        return Ok(default_upload_ignore_config());
    }

    let raw = fs::read_to_string(&path).map_err(|e| {
        format!(
            "Failed to read upload ignore config {}: {}",
            path.display(),
            e
        )
    })?;
    let mut config: UploadIgnoreConfig = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "Failed to parse upload ignore config {}: {}",
            path.display(),
            e
        )
    })?;
    config.excluded_paths = normalize_excludes(config.excluded_paths);
    Ok(config)
}

pub fn save_upload_ignore_config(config: UploadIgnoreConfig) -> Result<UploadIgnoreConfig, String> {
    let path = upload_ignore_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create upload ignore config directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }

    let config = UploadIgnoreConfig {
        excluded_paths: normalize_excludes(config.excluded_paths),
        respect_gitignore: config.respect_gitignore,
    };
    let raw = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize upload ignore config: {}", e))?;
    fs::write(&path, raw).map_err(|e| {
        format!(
            "Failed to write upload ignore config {}: {}",
            path.display(),
            e
        )
    })?;
    Ok(config)
}

pub fn effective_directory_transfer_options(
    request_excludes: Option<Vec<String>>,
    respect_gitignore: Option<bool>,
    delete_extra: bool,
) -> DirectoryTransferOptions {
    let mut config = load_upload_ignore_config().unwrap_or_else(|_| default_upload_ignore_config());

    if let Ok(env_excludes) = std::env::var("VIBESHELL_SFTP_EXCLUDES") {
        config
            .excluded_paths
            .extend(parse_exclude_list(&env_excludes));
    }
    if let Some(extra) = request_excludes {
        config.excluded_paths.extend(extra);
    }

    DirectoryTransferOptions {
        excluded_paths: normalize_excludes(config.excluded_paths),
        respect_gitignore: respect_gitignore.unwrap_or(config.respect_gitignore),
        delete_extra,
    }
}

pub fn build_directory_transfer_plan(
    local_root: &Path,
    options: &DirectoryTransferOptions,
) -> Result<DirectoryTransferPlan, String> {
    let root_path = fs::canonicalize(local_root).map_err(|e| {
        format!(
            "Failed to access local directory {}: {}",
            local_root.display(),
            e
        )
    })?;
    let root_meta = fs::metadata(&root_path)
        .map_err(|e| format!("Failed to read metadata for {}: {}", root_path.display(), e))?;
    if !root_meta.is_dir() {
        return Err(format!(
            "Local upload path is not a directory: {}",
            root_path.display()
        ));
    }

    let overrides = upload_exclude_matcher(&root_path, options)?;

    let mut builder = WalkBuilder::new(&root_path);
    builder
        .hidden(false)
        .ignore(false)
        .git_ignore(options.respect_gitignore)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .parents(false)
        .follow_links(false)
        .same_file_system(false)
        .overrides(overrides);

    let mut directories = Vec::new();
    let mut files = Vec::new();
    let mut total_bytes = 0;

    for result in builder.build() {
        let entry = result.map_err(|e| format!("Failed to walk local upload directory: {}", e))?;
        let path = entry.path();
        let relative_path = relative_path_string(&root_path, path)?;
        if relative_path.is_empty() {
            continue;
        }

        let file_type = entry
            .file_type()
            .ok_or_else(|| format!("Failed to determine file type for {}", path.display()))?;
        if file_type.is_dir() {
            directories.push(relative_path);
        } else if file_type.is_file() {
            let metadata = entry
                .metadata()
                .map_err(|e| format!("Failed to read metadata for {}: {}", path.display(), e))?;
            let size = metadata.len();
            total_bytes += size;
            files.push(LocalTransferEntry {
                local_path: path.to_path_buf(),
                relative_path,
                size,
            });
        }
    }

    directories.sort_by_key(|path| path.matches('/').count());
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let root_name = root_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("upload")
        .to_string();

    Ok(DirectoryTransferPlan {
        root_path,
        root_name,
        directories,
        files,
        total_bytes,
    })
}

pub async fn transfer_directory_to_sftp(
    sftp: &SftpSession,
    local_root: &Path,
    remote_root: &str,
    mode: DirectoryTransferMode,
    options: &DirectoryTransferOptions,
) -> Result<DirectoryTransferSummary, String> {
    // The ignore-aware walk can traverse huge trees; keep it off async workers.
    let walk_root = local_root.to_path_buf();
    let walk_options = options.clone();
    let plan = tokio::task::spawn_blocking(move || {
        build_directory_transfer_plan(&walk_root, &walk_options)
    })
    .await
    .map_err(|e| format!("Directory planning task failed: {}", e))??;

    let remote_root = normalize_remote_dir(remote_root);
    let mut created_directories = 0;
    let mut uploaded_files = 0;
    let mut skipped_files = 0;
    let mut transferred_bytes = 0;

    sftp_mkdir_recursive(sftp, &remote_root).await?;
    created_directories += 1;

    for directory in &plan.directories {
        let remote_path = join_remote_relative(&remote_root, directory);
        sftp_mkdir_recursive(sftp, &remote_path).await?;
        created_directories += 1;
    }

    for file in &plan.files {
        let remote_path = join_remote_relative(&remote_root, &file.relative_path);
        if mode == DirectoryTransferMode::Sync
            && remote_file_matches(sftp, &remote_path, file).await
        {
            skipped_files += 1;
            continue;
        }

        // Stream in fixed-size chunks so a large file never materialises in memory.
        let transferred = upload_local_file_streaming(sftp, &file.local_path, &remote_path).await?;
        uploaded_files += 1;
        transferred_bytes += transferred;
    }

    let deleted_entries = if mode == DirectoryTransferMode::Sync && options.delete_extra {
        delete_extra_sftp_entries(sftp, &remote_root, &plan, options).await?
    } else {
        0
    };

    Ok(DirectoryTransferSummary {
        mode: mode.as_str().to_string(),
        local_root: plan.root_path.to_string_lossy().to_string(),
        remote_root,
        directories_total: plan.directories.len() + 1,
        files_total: plan.files.len(),
        created_directories,
        uploaded_files,
        skipped_files,
        deleted_entries,
        transferred_bytes,
    })
}

pub fn transfer_directory_to_local(
    local_root: &Path,
    target_root: &Path,
    mode: DirectoryTransferMode,
    options: &DirectoryTransferOptions,
) -> Result<DirectoryTransferSummary, String> {
    let plan = build_directory_transfer_plan(local_root, options)?;
    if target_root.exists() && target_root.is_file() {
        return Err(format!("Target path is a file: {}", target_root.display()));
    }

    let mut created_directories = 0;
    let mut uploaded_files = 0;
    let mut skipped_files = 0;
    let mut transferred_bytes = 0;

    fs::create_dir_all(target_root).map_err(|e| {
        format!(
            "Failed to create target directory {}: {}",
            target_root.display(),
            e
        )
    })?;
    let canonical_target = fs::canonicalize(target_root)
        .map_err(|e| format!("Failed to resolve target directory: {}", e))?;
    if canonical_target.starts_with(&plan.root_path)
        || plan.root_path.starts_with(&canonical_target)
    {
        return Err("Source and target directories must not overlap".into());
    }
    created_directories += 1;

    for directory in &plan.directories {
        let target = join_local_relative(target_root, directory);
        fs::create_dir_all(&target)
            .map_err(|e| format!("Failed to create directory {}: {}", target.display(), e))?;
        created_directories += 1;
    }

    for file in &plan.files {
        let target = join_local_relative(target_root, &file.relative_path);
        if mode == DirectoryTransferMode::Sync && local_file_matches(&target, file) {
            skipped_files += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }
        fs::copy(&file.local_path, &target).map_err(|e| {
            format!(
                "Failed to copy {} -> {}: {}",
                file.local_path.display(),
                target.display(),
                e
            )
        })?;
        uploaded_files += 1;
        transferred_bytes += file.size;
    }

    let deleted_entries = if mode == DirectoryTransferMode::Sync && options.delete_extra {
        delete_extra_local_entries(target_root, &plan, options)?
    } else {
        0
    };

    Ok(DirectoryTransferSummary {
        mode: mode.as_str().to_string(),
        local_root: plan.root_path.to_string_lossy().to_string(),
        remote_root: target_root.to_string_lossy().to_string(),
        directories_total: plan.directories.len() + 1,
        files_total: plan.files.len(),
        created_directories,
        uploaded_files,
        skipped_files,
        deleted_entries,
        transferred_bytes,
    })
}

pub fn join_remote_relative(base: &str, relative: &str) -> String {
    relative
        .split('/')
        .filter(|component| !component.is_empty())
        .fold(normalize_remote_dir(base), |path, component| {
            join_remote_child(&path, component)
        })
}

fn upload_exclude_matcher(
    root: &Path,
    options: &DirectoryTransferOptions,
) -> Result<Override, String> {
    let mut builder = OverrideBuilder::new(root);
    for pattern in &options.excluded_paths {
        let pattern = pattern.trim().replace('\\', "/");
        if !pattern.is_empty() {
            builder
                .add(&format!("!{}", pattern))
                .map_err(|e| format!("Invalid upload exclude pattern '{}': {}", pattern, e))?;
        }
    }
    builder
        .build()
        .map_err(|e| format!("Failed to build upload exclude matcher: {}", e))
}

fn excluded_path(matcher: &Override, root: &Path, path: &Path, is_dir: bool) -> bool {
    path.ancestors()
        .take_while(|parent| *parent != root)
        .enumerate()
        .any(|(index, parent)| matcher.matched(parent, is_dir || index > 0).is_ignore())
}

pub fn protected_by_excludes(relative_path: &str, options: &DirectoryTransferOptions) -> bool {
    let root = Path::new(".");
    let path = root.join(relative_path.replace('\\', "/"));
    upload_exclude_matcher(root, options)
        .map(|matcher| excluded_path(&matcher, root, &path, true))
        .unwrap_or(true)
}

/// Deletion uses the same upload exclusions and source-tree gitignore rules.
/// Also retain ancestors of protected entries so nonempty protected directories
/// never cause a partial sync followed by a remove_dir failure.
fn sync_protected_paths<'a>(
    plan: &DirectoryTransferPlan,
    options: &DirectoryTransferOptions,
    entries: impl IntoIterator<Item = (&'a str, bool)>,
) -> Result<HashSet<String>, String> {
    let overrides = upload_exclude_matcher(&plan.root_path, options)?;
    let mut ignores = Vec::new();
    if options.respect_gitignore {
        for relative in std::iter::once("").chain(plan.directories.iter().map(String::as_str)) {
            let root = plan.root_path.join(relative);
            let path = root.join(".gitignore");
            let content = match fs::read_to_string(&path) {
                Ok(content) => content,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    return Err(format!(
                        "Cannot read sync ignore rules {}: {}",
                        path.display(),
                        e
                    ))
                }
            };
            let mut builder = GitignoreBuilder::new(&root);
            for line in content.lines() {
                builder
                    .add_line(Some(path.clone()), line)
                    .map_err(|e| e.to_string())?;
            }
            ignores.push((root, builder.build().map_err(|e| e.to_string())?));
        }
    }
    let mut protected = HashSet::new();
    for (relative, is_dir) in entries {
        let path = plan.root_path.join(relative);
        let git_ignored = ignores
            .iter()
            .rev()
            .find_map(|(root, matcher)| {
                if !path.starts_with(root) {
                    return None;
                }
                let matched = matcher.matched_path_or_any_parents(&path, is_dir);
                if matched.is_none() {
                    None
                } else {
                    Some(matched.is_ignore())
                }
            })
            .unwrap_or(false);
        if excluded_path(&overrides, &plan.root_path, &path, is_dir) || git_ignored {
            let mut current = Some(relative);
            while let Some(path) = current {
                protected.insert(path.to_string());
                current = path.rsplit_once('/').map(|(parent, _)| parent);
            }
        }
    }
    Ok(protected)
}

fn upload_ignore_config_path() -> Result<PathBuf, String> {
    let dirs = ProjectDirs::from("com", "vibeshell", "VibeShell")
        .ok_or_else(|| "Could not determine VibeShell config directory".to_string())?;
    Ok(dirs.config_dir().join("sftp-upload-ignore.json"))
}

fn normalize_excludes(excludes: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    excludes
        .into_iter()
        .flat_map(|value| parse_exclude_list(&value))
        .filter_map(|value| {
            let normalized = value.trim().replace('\\', "/");
            if normalized.is_empty() || normalized.starts_with('#') {
                return None;
            }
            if seen.insert(normalized.clone()) {
                Some(normalized)
            } else {
                None
            }
        })
        .collect()
}

fn parse_exclude_list(value: &str) -> Vec<String> {
    value
        .split(['\n', ',', ';'])
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

fn relative_path_string(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|e| {
        format!(
            "Failed to compute relative path for {} under {}: {}",
            path.display(),
            root.display(),
            e
        )
    })?;

    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn normalize_remote_dir(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        ".".to_string()
    } else if trimmed != "/" {
        trimmed.trim_end_matches('/').to_string()
    } else {
        trimmed.to_string()
    }
}

async fn remote_file_matches(
    sftp: &SftpSession,
    remote_path: &str,
    file: &LocalTransferEntry,
) -> bool {
    if !matches!(sftp.metadata(remote_path).await, Ok(meta) if !meta.is_dir() && meta.len() == file.size)
    {
        return false;
    }
    let Ok(mut remote) = sftp.open(remote_path).await else {
        return false;
    };
    let matched: std::io::Result<bool> = async {
        let mut local = tokio::fs::File::open(&file.local_path).await?;
        let mut source = vec![0; SFTP_TRANSFER_CHUNK_SIZE];
        let mut target = vec![0; SFTP_TRANSFER_CHUNK_SIZE];
        loop {
            let count = local.read(&mut source).await?;
            if count == 0 {
                return Ok(remote.read(&mut target[..1]).await? == 0);
            }
            remote.read_exact(&mut target[..count]).await?;
            if source[..count] != target[..count] {
                return Ok(false);
            }
        }
    }
    .await;
    let closed = remote.shutdown().await.is_ok();
    closed && matched.unwrap_or(false)
}

fn local_file_matches(target: &Path, file: &LocalTransferEntry) -> bool {
    use std::io::Read;
    if !matches!(fs::metadata(target), Ok(meta) if meta.is_file() && meta.len() == file.size) {
        return false;
    }
    let compare = || -> std::io::Result<bool> {
        let mut local = fs::File::open(&file.local_path)?;
        let mut remote = fs::File::open(target)?;
        let mut source = vec![0; SFTP_TRANSFER_CHUNK_SIZE];
        let mut target = vec![0; SFTP_TRANSFER_CHUNK_SIZE];
        loop {
            let count = local.read(&mut source)?;
            if count == 0 {
                return Ok(remote.read(&mut target[..1])? == 0);
            }
            remote.read_exact(&mut target[..count])?;
            if source[..count] != target[..count] {
                return Ok(false);
            }
        }
    };
    compare().unwrap_or(false)
}

fn join_local_relative(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .filter(|component| !component.is_empty())
        .fold(root.to_path_buf(), |path, component| path.join(component))
}

#[derive(Debug, Clone)]
struct RemoteEntry {
    relative_path: String,
    path: String,
    is_dir: bool,
}

async fn collect_remote_entries(
    sftp: &SftpSession,
    root: &str,
    relative_root: &str,
    entries: &mut Vec<RemoteEntry>,
) -> Result<(), String> {
    if relative_root
        .split('/')
        .filter(|part| !part.is_empty())
        .count()
        > MAX_RECURSIVE_DEPTH as usize
    {
        return Err("Remote directory traversal exceeded the recursion limit".into());
    }
    let dir_entries = sftp
        .read_dir(root)
        .await
        .map_err(|e| format!("Failed to list remote directory {}: {}", root, e))?;

    for entry in dir_entries {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        validate_remote_entry_name(&name)?;
        let relative_path = if relative_root.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", relative_root, name)
        };
        let path = join_remote_child(root, &name);
        let is_dir = entry.file_type().is_dir();
        if is_dir {
            Box::pin(collect_remote_entries(sftp, &path, &relative_path, entries)).await?;
        }
        entries.push(RemoteEntry {
            relative_path,
            path,
            is_dir,
        });
    }

    Ok(())
}

async fn delete_extra_sftp_entries(
    sftp: &SftpSession,
    remote_root: &str,
    plan: &DirectoryTransferPlan,
    options: &DirectoryTransferOptions,
) -> Result<usize, String> {
    let mut keep = planned_relative_paths(plan);
    keep.insert(String::new());

    let mut remote_entries = Vec::new();
    collect_remote_entries(sftp, remote_root, "", &mut remote_entries).await?;
    let protected = sync_protected_paths(
        plan,
        options,
        remote_entries
            .iter()
            .map(|entry| (entry.relative_path.as_str(), entry.is_dir)),
    )?;

    remote_entries.sort_by(|a, b| {
        b.relative_path
            .matches('/')
            .count()
            .cmp(&a.relative_path.matches('/').count())
    });

    let mut deleted = 0;
    for entry in remote_entries {
        if keep.contains(&entry.relative_path) || protected.contains(&entry.relative_path) {
            continue;
        }
        if entry.is_dir {
            sftp.remove_dir(&entry.path)
                .await
                .map_err(|e| format!("Failed to remove remote directory {}: {}", entry.path, e))?;
        } else {
            sftp.remove_file(&entry.path)
                .await
                .map_err(|e| format!("Failed to remove remote file {}: {}", entry.path, e))?;
        }
        deleted += 1;
    }

    Ok(deleted)
}

fn delete_extra_local_entries(
    target_root: &Path,
    plan: &DirectoryTransferPlan,
    options: &DirectoryTransferOptions,
) -> Result<usize, String> {
    if !target_root.exists() {
        return Ok(0);
    }

    let keep = planned_relative_paths(plan);
    let mut entries = Vec::new();
    collect_local_entries(target_root, target_root, &mut entries)?;
    let protected = sync_protected_paths(
        plan,
        options,
        entries
            .iter()
            .map(|(relative, _, is_dir)| (relative.as_str(), *is_dir)),
    )?;
    entries.sort_by(|(left, _, _), (right, _, _)| {
        right.matches('/').count().cmp(&left.matches('/').count())
    });

    let mut deleted = 0;
    for (relative_path, path, is_dir) in entries {
        if keep.contains(&relative_path) || protected.contains(&relative_path) {
            continue;
        }
        if is_dir {
            fs::remove_dir(&path).map_err(|e| {
                format!("Failed to remove local directory {}: {}", path.display(), e)
            })?;
        } else {
            fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove local file {}: {}", path.display(), e))?;
        }
        deleted += 1;
    }

    Ok(deleted)
}

fn collect_local_entries(
    root: &Path,
    current: &Path,
    entries: &mut Vec<(String, PathBuf, bool)>,
) -> Result<(), String> {
    for entry in fs::read_dir(current)
        .map_err(|e| format!("Failed to read directory {}: {}", current.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|e| format!("Failed to read metadata for {}: {}", path.display(), e))?;
        let relative_path = relative_path_string(root, &path)?;
        if metadata.is_dir() {
            collect_local_entries(root, &path, entries)?;
        }
        entries.push((relative_path, path, metadata.is_dir()));
    }

    Ok(())
}

fn planned_relative_paths(plan: &DirectoryTransferPlan) -> HashSet<String> {
    let mut keep: HashSet<String> = plan.directories.iter().cloned().collect();
    keep.extend(plan.files.iter().map(|file| file.relative_path.clone()));
    keep
}

#[allow(dead_code)]
fn modified_at_seconds(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write as _;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_transfer_progress_new() {
        let progress = TransferProgress::new("file.txt".to_string(), 1000);

        assert_eq!(progress.filename, "file.txt");
        assert_eq!(progress.total_bytes, 1000);
        assert_eq!(progress.transferred_bytes, 0);
        assert_eq!(progress.status, TransferStatus::Pending);
        assert!(!progress.id.is_empty());
    }

    #[test]
    fn test_transfer_progress_percentage() {
        let mut progress = TransferProgress::new("file.txt".to_string(), 1000);

        assert_eq!(progress.percentage(), 0.0);

        progress.transferred_bytes = 500;
        assert_eq!(progress.percentage(), 50.0);

        progress.transferred_bytes = 1000;
        assert_eq!(progress.percentage(), 100.0);
    }

    #[test]
    fn test_transfer_progress_percentage_zero_total() {
        let progress = TransferProgress::new("empty.txt".to_string(), 0);
        assert_eq!(progress.percentage(), 100.0);
    }

    #[test]
    fn test_transfer_status_serialization() {
        let status = TransferStatus::InProgress;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"in_progress\"");

        let deserialized: TransferStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_transfer_progress_is_complete() {
        let mut progress = TransferProgress::new("file.txt".to_string(), 1000);
        assert!(!progress.is_complete());

        progress.status = TransferStatus::Completed;
        assert!(progress.is_complete());
    }

    #[test]
    fn test_transfer_progress_is_failed() {
        let mut progress = TransferProgress::new("file.txt".to_string(), 1000);
        assert!(!progress.is_failed());

        progress.status = TransferStatus::Failed;
        assert!(progress.is_failed());
    }

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vibeshell-sync-{}-{}-{}",
            name,
            std::process::id(),
            stamp
        ))
    }

    #[test]
    fn chunk_size_is_capped_for_bounded_and_unbounded_transfers() {
        const CHUNK: u64 = SFTP_TRANSFER_CHUNK_SIZE as u64;

        // Unknown size always gets the full chunk.
        assert_eq!(next_transfer_chunk_size(None), SFTP_TRANSFER_CHUNK_SIZE);
        // Larger than a chunk is clamped to the chunk.
        assert_eq!(
            next_transfer_chunk_size(Some(CHUNK * 10)),
            SFTP_TRANSFER_CHUNK_SIZE
        );
        // Exact chunk boundary stays at full size.
        assert_eq!(
            next_transfer_chunk_size(Some(CHUNK)),
            SFTP_TRANSFER_CHUNK_SIZE
        );
        // The final partial chunk is right-sized.
        assert_eq!(next_transfer_chunk_size(Some(1234)), 1234);
        // A completed transfer requests nothing (keeps downloads from
        // over-reading past the advertised size).
        assert_eq!(next_transfer_chunk_size(Some(0)), 0);
    }

    #[test]
    fn remote_parent_dir_splits_like_helpers() {
        assert_eq!(
            remote_parent_dir("/home/user/file.txt"),
            Some("/home/user".to_string())
        );
        assert_eq!(remote_parent_dir("/file.txt"), Some("/".to_string()));
        assert_eq!(remote_parent_dir("/"), None);
    }

    // ----- In-memory SFTP server used to exercise the streaming loops -----

    struct MemSftpHandler {
        files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        handles: Arc<Mutex<HashMap<String, String>>>,
    }

    impl russh_sftp::server::Handler for MemSftpHandler {
        type Error = russh_sftp::protocol::StatusCode;

        fn unimplemented(&self) -> Self::Error {
            russh_sftp::protocol::StatusCode::OpUnsupported
        }

        async fn open(
            &mut self,
            id: u32,
            filename: String,
            pflags: OpenFlags,
            _attrs: russh_sftp::protocol::FileAttributes,
        ) -> Result<russh_sftp::protocol::Handle, Self::Error> {
            use russh_sftp::protocol::{Handle, StatusCode};

            let mut files = self.files.lock().expect("mem sftp files lock");
            let write_mode = pflags.contains(OpenFlags::WRITE);
            if write_mode {
                if pflags.contains(OpenFlags::TRUNCATE) || !files.contains_key(&filename) {
                    files.insert(filename.clone(), Vec::new());
                }
            } else if !files.contains_key(&filename) {
                return Err(StatusCode::NoSuchFile);
            }

            let handle = format!("h-{id}");
            self.handles
                .lock()
                .expect("mem sftp handles lock")
                .insert(handle.clone(), filename);
            Ok(Handle { id, handle })
        }

        async fn read(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            len: u32,
        ) -> Result<russh_sftp::protocol::Data, Self::Error> {
            use russh_sftp::protocol::{Data, StatusCode};

            let path = self
                .handles
                .lock()
                .expect("mem sftp handles lock")
                .get(&handle)
                .cloned()
                .ok_or(StatusCode::Failure)?;
            let files = self.files.lock().expect("mem sftp files lock");
            let content = files.get(&path).ok_or(StatusCode::Failure)?;
            let offset = offset as usize;
            if offset >= content.len() {
                return Err(StatusCode::Eof);
            }
            let end = (offset + len as usize).min(content.len());
            Ok(Data {
                id,
                data: content[offset..end].to_vec(),
            })
        }

        async fn write(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            data: Vec<u8>,
        ) -> Result<russh_sftp::protocol::Status, Self::Error> {
            use russh_sftp::protocol::StatusCode;

            let path = self
                .handles
                .lock()
                .expect("mem sftp handles lock")
                .get(&handle)
                .cloned()
                .ok_or(StatusCode::Failure)?;
            let mut files = self.files.lock().expect("mem sftp files lock");
            let content = files.entry(path).or_default();
            let offset = offset as usize;
            if content.len() < offset + data.len() {
                content.resize(offset + data.len(), 0);
            }
            content[offset..offset + data.len()].copy_from_slice(&data);
            Ok(russh_sftp::protocol::Status {
                id,
                status_code: StatusCode::Ok,
                error_message: "Ok".to_string(),
                language_tag: "en-US".to_string(),
            })
        }

        async fn fstat(
            &mut self,
            id: u32,
            handle: String,
        ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
            use russh_sftp::protocol::{Attrs, FileAttributes, StatusCode};

            let path = self
                .handles
                .lock()
                .expect("mem sftp handles lock")
                .get(&handle)
                .cloned()
                .ok_or(StatusCode::Failure)?;
            let files = self.files.lock().expect("mem sftp files lock");
            let size = files.get(&path).ok_or(StatusCode::Failure)?.len() as u64;
            Ok(Attrs {
                id,
                attrs: FileAttributes {
                    size: Some(size),
                    ..FileAttributes::default()
                },
            })
        }

        async fn close(
            &mut self,
            id: u32,
            handle: String,
        ) -> Result<russh_sftp::protocol::Status, Self::Error> {
            use russh_sftp::protocol::StatusCode;

            self.handles
                .lock()
                .expect("mem sftp handles lock")
                .remove(&handle);
            Ok(russh_sftp::protocol::Status {
                id,
                status_code: StatusCode::Ok,
                error_message: "Ok".to_string(),
                language_tag: "en-US".to_string(),
            })
        }
    }

    async fn mem_sftp() -> (SftpSession, Arc<Mutex<HashMap<String, Vec<u8>>>>) {
        let files = Arc::new(Mutex::new(HashMap::new()));
        let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        russh_sftp::server::run(
            server_stream,
            MemSftpHandler {
                files: files.clone(),
                handles: Arc::new(Mutex::new(HashMap::new())),
            },
        )
        .await;
        let client = SftpSession::new(client_stream)
            .await
            .expect("initialize mem sftp client");
        (client, files)
    }

    fn deterministic_bytes(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[tokio::test]
    async fn upload_and_download_stream_in_chunks_without_losing_bytes() {
        let (sftp, files) = mem_sftp().await;
        let root = temp_dir("stream");
        std::fs::create_dir_all(root.join("in")).unwrap();

        // Larger than two chunks so the loop must issue multiple requests.
        let payload = deterministic_bytes(SFTP_TRANSFER_CHUNK_SIZE * 2 + 7777);
        let local_src = root.join("in/big.bin");
        std::fs::File::create(&local_src)
            .unwrap()
            .write_all(&payload)
            .unwrap();

        let transferred = upload_local_file_streaming(&sftp, &local_src, "/mem/big.bin")
            .await
            .expect("streaming upload");
        assert_eq!(transferred, payload.len() as u64);
        assert_eq!(files.lock().unwrap().get("/mem/big.bin").unwrap(), &payload);

        // Download into a not-yet-existing nested directory to cover parent
        // directory creation in the streaming path.
        let local_dst = root.join("out/nested/big.bin");
        let downloaded = download_remote_file_streaming(&sftp, "/mem/big.bin", &local_dst)
            .await
            .expect("streaming download");
        assert_eq!(downloaded, payload.len() as u64);
        let actual = std::fs::read(&local_dst).unwrap();
        assert_eq!(
            actual.len(),
            payload.len(),
            "download returned before all bytes were written"
        );
        assert!(
            actual == payload,
            "downloaded bytes differ from the uploaded bytes"
        );

        fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn streaming_transfer_handles_empty_files() {
        let (sftp, files) = mem_sftp().await;
        let root = temp_dir("stream-empty");
        std::fs::create_dir_all(&root).unwrap();

        let local_src = root.join("empty.bin");
        std::fs::File::create(&local_src).unwrap();

        let transferred = upload_local_file_streaming(&sftp, &local_src, "/mem/empty.bin")
            .await
            .expect("streaming empty upload");
        assert_eq!(transferred, 0);
        assert_eq!(
            files.lock().unwrap().get("/mem/empty.bin").unwrap(),
            &Vec::<u8>::new()
        );

        let local_dst = root.join("empty-out.bin");
        let downloaded = download_remote_file_streaming(&sftp, "/mem/empty.bin", &local_dst)
            .await
            .expect("streaming empty download");
        assert_eq!(downloaded, 0);
        assert!(std::fs::metadata(&local_dst).unwrap().len() == 0);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn plan_ignores_default_dependency_directories() {
        let root = temp_dir("defaults");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join("src/app.ts"), b"ok").unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), b"skip").unwrap();

        let options = DirectoryTransferOptions {
            excluded_paths: default_upload_ignore_config().excluded_paths,
            respect_gitignore: true,
            delete_extra: false,
        };
        let plan = build_directory_transfer_plan(&root, &options).unwrap();

        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].relative_path, "src/app.ts");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn plan_respects_root_gitignore() {
        let root = temp_dir("gitignore");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".gitignore"), "ignored.log\ncache/\n").unwrap();
        fs::write(root.join("src/app.ts"), b"ok").unwrap();
        fs::write(root.join("ignored.log"), b"skip").unwrap();
        fs::create_dir_all(root.join("cache")).unwrap();
        fs::write(root.join("cache/blob"), b"skip").unwrap();

        let options = DirectoryTransferOptions {
            excluded_paths: Vec::new(),
            respect_gitignore: true,
            delete_extra: false,
        };
        let plan = build_directory_transfer_plan(&root, &options).unwrap();
        let files: Vec<_> = plan
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();

        assert!(files.contains(&".gitignore"));
        assert!(files.contains(&"src/app.ts"));
        assert!(!files.contains(&"ignored.log"));
        assert!(!files.contains(&"cache/blob"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn sync_copies_same_size_edits_and_skips_identical_content() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        fs::write(source.path().join("file.txt"), b"new!").unwrap();
        fs::write(target.path().join("file.txt"), b"old!").unwrap();
        let options = DirectoryTransferOptions {
            excluded_paths: vec![],
            respect_gitignore: false,
            delete_extra: false,
        };
        let first = transfer_directory_to_local(
            source.path(),
            target.path(),
            DirectoryTransferMode::Sync,
            &options,
        )
        .unwrap();
        assert_eq!(first.uploaded_files, 1);
        assert_eq!(fs::read(target.path().join("file.txt")).unwrap(), b"new!");
        let second = transfer_directory_to_local(
            source.path(),
            target.path(),
            DirectoryTransferMode::Sync,
            &options,
        )
        .unwrap();
        assert_eq!(second.skipped_files, 1);
    }

    #[test]
    fn sync_delete_retains_globs_nested_gitignore_and_protected_parents() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        fs::create_dir(source.path().join("src")).unwrap();
        fs::write(source.path().join(".gitignore"), "cache/\n*.secret\n").unwrap();
        fs::write(source.path().join("src/.gitignore"), "nested.tmp\n").unwrap();
        for name in [
            "old/data.log",
            "cache/keep.bin",
            "private.secret",
            "src/nested.tmp",
            "remove.txt",
        ] {
            let path = target.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"keep unless unignored").unwrap();
        }
        let options = DirectoryTransferOptions {
            excluded_paths: vec!["*.log".into()],
            respect_gitignore: true,
            delete_extra: true,
        };
        let result = transfer_directory_to_local(
            source.path(),
            target.path(),
            DirectoryTransferMode::Sync,
            &options,
        )
        .unwrap();
        assert_eq!(result.deleted_entries, 1);
        for name in [
            "old/data.log",
            "cache/keep.bin",
            "private.secret",
            "src/nested.tmp",
        ] {
            assert!(
                target.path().join(name).exists(),
                "deleted protected entry {name}"
            );
        }
        assert!(!target.path().join("remove.txt").exists());
    }

    #[test]
    fn local_directory_transfer_rejects_overlapping_roots() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("keep.txt"), b"keep").unwrap();
        let options = DirectoryTransferOptions {
            excluded_paths: vec![],
            respect_gitignore: false,
            delete_extra: true,
        };
        for target in [source.path().to_path_buf(), source.path().join("nested")] {
            assert!(transfer_directory_to_local(
                source.path(),
                &target,
                DirectoryTransferMode::Sync,
                &options
            )
            .unwrap_err()
            .contains("overlap"));
        }
        assert_eq!(fs::read(source.path().join("keep.txt")).unwrap(), b"keep");
    }

    #[test]
    fn protected_excludes_match_nested_directory_names() {
        let options = DirectoryTransferOptions {
            excluded_paths: vec!["node_modules/".to_string()],
            respect_gitignore: true,
            delete_extra: true,
        };

        assert!(protected_by_excludes(
            "app/node_modules/pkg/index.js",
            &options
        ));
        assert!(!protected_by_excludes("app/src/index.js", &options));
    }
}
