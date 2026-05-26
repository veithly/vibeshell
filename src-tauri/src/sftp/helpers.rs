//! Shared SFTP helper functions used by both Tauri commands and MCP tools.

use russh_sftp::client::SftpSession;

/// Maximum recursion depth for directory operations (prevents symlink loops)
pub const MAX_RECURSIVE_DEPTH: u32 = 100;

/// Resolve a path that may contain `~` against the SFTP home directory.
/// Relative paths are resolved against `current_path`.
pub fn resolve_remote_path(path: &str, home_dir: &str, current_path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "~" {
        home_dir.to_string()
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        format!("{}/{}", home_dir, rest)
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        // Relative path - resolve against current_path (or home_dir if empty)
        let base = if current_path.is_empty() {
            home_dir
        } else {
            current_path
        };
        format!("{}/{}", base, trimmed)
    }
}

pub fn join_remote_child(parent: &str, child_name: &str) -> String {
    let parent = parent.trim_end_matches('/');
    let child_name = child_name.trim_start_matches('/');

    if parent.is_empty() {
        format!("/{}", child_name)
    } else {
        format!("{}/{}", parent, child_name)
    }
}

/// Resolve the final remote file path for an upload.
///
/// Upload callers often pass a directory as the destination, especially from
/// CLI-style `put local /remote/dir` usage. In that case SFTP writes must target
/// `dir/<local filename>` rather than attempting to write bytes over the
/// directory path itself.
pub async fn resolve_remote_upload_path(
    sftp: &SftpSession,
    resolved_remote_path: &str,
    local_filename: &str,
) -> String {
    if local_filename.is_empty() {
        return resolved_remote_path.to_string();
    }

    if resolved_remote_path.ends_with('/') {
        return join_remote_child(resolved_remote_path, local_filename);
    }

    match sftp.metadata(resolved_remote_path).await {
        Ok(metadata) if metadata.is_dir() => {
            join_remote_child(resolved_remote_path, local_filename)
        }
        _ => resolved_remote_path.to_string(),
    }
}

/// Recursively delete a directory via SFTP with depth limit to prevent symlink loops
pub async fn sftp_remove_recursive(
    sftp: &SftpSession,
    path: &str,
    depth: u32,
) -> Result<(), String> {
    if depth > MAX_RECURSIVE_DEPTH {
        return Err(format!(
            "Maximum recursion depth ({}) exceeded while deleting {}. Possible symlink loop.",
            MAX_RECURSIVE_DEPTH, path
        ));
    }

    // List directory contents
    let entries = sftp
        .read_dir(path)
        .await
        .map_err(|e| format!("Failed to list directory for deletion {}: {}", path, e))?;

    for entry in entries {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let child_path = format!("{}/{}", path, name);
        let file_type = entry.file_type();
        if file_type.is_dir() {
            // Recurse into subdirectory with depth tracking
            Box::pin(sftp_remove_recursive(sftp, &child_path, depth + 1)).await?;
        } else {
            sftp.remove_file(&child_path)
                .await
                .map_err(|e| format!("Failed to delete file {}: {}", child_path, e))?;
        }
    }

    // Now remove the empty directory itself
    sftp.remove_dir(path)
        .await
        .map_err(|e| format!("Failed to remove directory {}: {}", path, e))?;

    Ok(())
}

/// Recursively create directories via SFTP (equivalent to mkdir -p)
pub async fn sftp_mkdir_recursive(sftp: &SftpSession, path: &str) -> Result<(), String> {
    // Try creating the directory directly first (fast path for single-level creation)
    if sftp.create_dir(path).await.is_ok() {
        return Ok(());
    }

    // Walk path components and create each missing directory
    let mut current = String::new();
    for component in path.split('/') {
        if component.is_empty() {
            current.push('/');
            continue;
        }
        if current.is_empty() || current == "/" {
            current = format!("{}{}", current, component);
        } else {
            current = format!("{}/{}", current, component);
        }
        // Try to create this directory; if it fails, check if it already exists
        if sftp.create_dir(&current).await.is_err() {
            match sftp.metadata(&current).await {
                Ok(meta) if meta.is_dir() => {} // Already exists as directory, continue
                _ => return Err(format!("Failed to create directory component: {}", current)),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_remote_path() {
        assert_eq!(
            resolve_remote_path("~", "/home/user", "/home/user"),
            "/home/user"
        );
        assert_eq!(
            resolve_remote_path("", "/home/user", "/home/user"),
            "/home/user"
        );
        assert_eq!(
            resolve_remote_path("~/docs", "/home/user", "/home/user"),
            "/home/user/docs"
        );
        assert_eq!(
            resolve_remote_path("/absolute/path", "/home/user", "/home/user"),
            "/absolute/path"
        );
        // Relative paths now resolve against current_path
        assert_eq!(
            resolve_remote_path("relative", "/home/user", "/var/log"),
            "/var/log/relative"
        );
        assert_eq!(
            resolve_remote_path("docs/file.txt", "/home/user", "/home/user/projects"),
            "/home/user/projects/docs/file.txt"
        );
        // Relative paths with empty current_path fall back to home_dir
        assert_eq!(
            resolve_remote_path("relative", "/home/user", ""),
            "/home/user/relative"
        );
    }

    #[test]
    fn test_join_remote_child() {
        assert_eq!(
            join_remote_child("/home/user/uploads", "file.txt"),
            "/home/user/uploads/file.txt"
        );
        assert_eq!(
            join_remote_child("/home/user/uploads/", "file.txt"),
            "/home/user/uploads/file.txt"
        );
        assert_eq!(join_remote_child("/", "file.txt"), "/file.txt");
        assert_eq!(
            join_remote_child("/home/user/uploads", "/file.txt"),
            "/home/user/uploads/file.txt"
        );
    }
}
