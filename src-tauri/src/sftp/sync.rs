//! Ignore-aware directory upload and sync helpers.

use directories::ProjectDirs;
use ignore::{overrides::OverrideBuilder, WalkBuilder};
use russh_sftp::client::SftpSession;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::sftp::helpers::{join_remote_child, sftp_mkdir_recursive};

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
        config.excluded_paths.extend(parse_exclude_list(&env_excludes));
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

    let mut overrides = OverrideBuilder::new(&root_path);
    for exclude in &options.excluded_paths {
        let pattern = exclude.trim();
        if pattern.is_empty() {
            continue;
        }
        overrides
            .add(&format!("!{}", pattern.replace('\\', "/")))
            .map_err(|e| format!("Invalid upload exclude pattern '{}': {}", pattern, e))?;
    }

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
        .overrides(
            overrides
                .build()
                .map_err(|e| format!("Failed to build upload exclude matcher: {}", e))?,
        );

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
    let plan = build_directory_transfer_plan(local_root, options)?;
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

        let content = fs::read(&file.local_path).map_err(|e| {
            format!(
                "Failed to read local file {}: {}",
                file.local_path.display(),
                e
            )
        })?;
        sftp.write(&remote_path, &content)
            .await
            .map_err(|e| format!("Failed to write remote file {}: {}", remote_path, e))?;
        uploaded_files += 1;
        transferred_bytes += content.len() as u64;
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

pub fn protected_by_excludes(relative_path: &str, options: &DirectoryTransferOptions) -> bool {
    let normalized = relative_path.replace('\\', "/");
    let components: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    options.excluded_paths.iter().any(|pattern| {
        let pattern = pattern
            .trim()
            .trim_start_matches('/')
            .trim_end_matches('/')
            .replace('\\', "/");
        if pattern.is_empty() || pattern.contains('*') || pattern.contains('?') {
            return false;
        }
        if pattern.contains('/') {
            normalized == pattern || normalized.starts_with(&format!("{}/", pattern))
        } else {
            components.iter().any(|component| *component == pattern)
        }
    })
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
    match sftp.metadata(remote_path).await {
        Ok(metadata) => !metadata.is_dir() && metadata.len() == file.size,
        Err(_) => false,
    }
}

fn local_file_matches(target: &Path, file: &LocalTransferEntry) -> bool {
    match fs::metadata(target) {
        Ok(metadata) => metadata.is_file() && metadata.len() == file.size,
        Err(_) => false,
    }
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
    let dir_entries = sftp
        .read_dir(root)
        .await
        .map_err(|e| format!("Failed to list remote directory {}: {}", root, e))?;

    for entry in dir_entries {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
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

    remote_entries.sort_by(|a, b| {
        b.relative_path
            .matches('/')
            .count()
            .cmp(&a.relative_path.matches('/').count())
    });

    let mut deleted = 0;
    for entry in remote_entries {
        if keep.contains(&entry.relative_path)
            || protected_by_excludes(&entry.relative_path, options)
        {
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
    entries.sort_by(|(left, _, _), (right, _, _)| {
        right.matches('/').count().cmp(&left.matches('/').count())
    });

    let mut deleted = 0;
    for (relative_path, path, is_dir) in entries {
        if keep.contains(&relative_path) || protected_by_excludes(&relative_path, options) {
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
