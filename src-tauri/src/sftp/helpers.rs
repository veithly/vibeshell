//! Shared SFTP helper functions used by both Tauri commands and MCP tools.

use russh_sftp::{client::SftpSession, protocol::OpenFlags};
use tokio::io::AsyncWriteExt;

/// Maximum recursion depth for directory operations (prevents symlink loops)
pub const MAX_RECURSIVE_DEPTH: u32 = 100;

#[derive(Clone, Copy, Debug)]
pub struct WriteRemoteFileOptions {
    pub create_parent_dirs: bool,
    pub overwrite: bool,
}

impl Default for WriteRemoteFileOptions {
    fn default() -> Self {
        Self {
            create_parent_dirs: true,
            overwrite: true,
        }
    }
}

/// Resolve a path that may contain `~` against the SFTP home directory.
/// Relative paths are resolved against `current_path`.
pub fn resolve_remote_path(path: &str, home_dir: &str, current_path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "~" {
        home_dir.to_string()
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        join_remote_child(home_dir, rest)
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        // Relative path - resolve against current_path (or home_dir if empty)
        let base = if current_path.is_empty() {
            home_dir
        } else {
            current_path
        };
        join_remote_child(base, trimmed)
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
        Ok(_) => resolved_remote_path.to_string(),
        Err(_) => match sftp.read_dir(resolved_remote_path).await {
            Ok(_) => join_remote_child(resolved_remote_path, local_filename),
            Err(_) => resolved_remote_path.to_string(),
        },
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

/// Write a file through SFTP, creating the parent directory first when needed.
pub async fn write_remote_file(
    sftp: &SftpSession,
    remote_path: &str,
    content: &[u8],
) -> Result<(), String> {
    write_remote_file_with_options(
        sftp,
        remote_path,
        content,
        WriteRemoteFileOptions::default(),
    )
    .await
}

pub async fn write_remote_file_with_options(
    sftp: &SftpSession,
    remote_path: &str,
    content: &[u8],
    options: WriteRemoteFileOptions,
) -> Result<(), String> {
    if remote_path.trim().is_empty() || remote_path.trim_end_matches('/') != remote_path {
        return Err(format!("Remote upload path is not a file: {}", remote_path));
    }

    let mut open_flags = OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE;
    if !options.overwrite {
        // CREATE + EXCLUDE checks existence atomically without relying on STAT.
        open_flags |= OpenFlags::EXCLUDE;
    }

    // Try the requested file operation first. Existing writable parent
    // directories do not need a metadata/MKDIR preflight, and some otherwise
    // functional SFTP servers reject those requests.
    let mut file = match sftp.open_with_flags(remote_path, open_flags).await {
        Ok(file) => file,
        Err(first_error) if options.create_parent_dirs => {
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
                })?
        }
        Err(error) => {
            return Err(format!(
                "Failed to create remote file {}: {}",
                remote_path, error
            ))
        }
    };

    file.write_all(content)
        .await
        .map_err(|e| format!("Failed to write remote file {}: {}", remote_path, e))?;
    file.shutdown()
        .await
        .map_err(|e| format!("Failed to close remote file {}: {}", remote_path, e))
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
        let child_path = join_remote_child(path, &name);
        if let Err(file_error) = sftp.remove_file(&child_path).await {
            // Directory entry attributes are optional in SFTP v3. Trying
            // REMOVE first avoids relying on absent/incorrect type bits and
            // safely removes symlinks without following them.
            Box::pin(sftp_remove_recursive(sftp, &child_path, depth + 1))
                .await
                .map_err(|directory_error| {
                    format!(
                        "Failed to delete {} as file ({}) or directory ({})",
                        child_path, file_error, directory_error
                    )
                })?;
        }
    }

    // Now remove the empty directory itself
    sftp.remove_dir(path)
        .await
        .map_err(|e| format!("Failed to remove directory {}: {}", path, e))?;

    Ok(())
}

/// Delete a remote path without requiring a separate STAT request.
/// Some SFTP servers support REMOVE/RMDIR but reject metadata requests.
pub async fn sftp_delete_path(
    sftp: &SftpSession,
    path: &str,
    recursive: bool,
) -> Result<(), String> {
    match sftp.remove_file(path).await {
        Ok(()) => Ok(()),
        Err(file_error) if recursive => {
            sftp_remove_recursive(sftp, path, 0)
                .await
                .map_err(|directory_error| {
                    format!(
                        "Failed to delete {} as file ({}) or directory ({})",
                        path, file_error, directory_error
                    )
                })
        }
        Err(file_error) => sftp.remove_dir(path).await.map_err(|directory_error| {
            format!(
                "Failed to delete {} as file ({}) or empty directory ({})",
                path, file_error, directory_error
            )
        }),
    }
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
        // Try to create this directory; if it fails, verify that an existing
        // directory is usable. READDIR is a compatibility fallback for SFTP
        // servers that implement directory operations but reject STAT.
        if let Err(create_error) = sftp.create_dir(&current).await {
            let is_directory = match sftp.metadata(&current).await {
                Ok(metadata) => metadata.is_dir(),
                Err(_) => sftp.read_dir(&current).await.is_ok(),
            };
            if !is_directory {
                return Err(format!(
                    "Failed to create directory component {}: {}",
                    current, create_error
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh_sftp::protocol::{File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum TestNode {
        Directory,
        File(Vec<u8>),
    }

    #[derive(Clone, Debug)]
    enum TestHandle {
        Directory { path: String, read: bool },
        File(String),
    }

    #[derive(Debug)]
    struct TestSftpState {
        nodes: HashMap<String, TestNode>,
        handles: HashMap<String, TestHandle>,
    }

    impl TestSftpState {
        fn with_home() -> Self {
            Self {
                nodes: HashMap::from([
                    ("/".to_string(), TestNode::Directory),
                    ("/home".to_string(), TestNode::Directory),
                    ("/home/test".to_string(), TestNode::Directory),
                ]),
                handles: HashMap::new(),
            }
        }

        fn parent_is_directory(&self, path: &str) -> bool {
            remote_parent_dir(path)
                .and_then(|parent| self.nodes.get(&parent))
                .is_some_and(|node| matches!(node, TestNode::Directory))
        }

        fn is_empty_directory(&self, path: &str) -> bool {
            let prefix = format!("{}/", path.trim_end_matches('/'));
            !self
                .nodes
                .keys()
                .any(|candidate| candidate.starts_with(&prefix))
        }
    }

    struct StatlessSftpHandler {
        state: Arc<Mutex<TestSftpState>>,
    }

    fn ok_status(id: u32) -> Status {
        Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".to_string(),
            language_tag: "en-US".to_string(),
        }
    }

    fn test_file_attributes(node: &TestNode) -> FileAttributes {
        FileAttributes {
            size: match node {
                TestNode::Directory => None,
                TestNode::File(content) => Some(content.len() as u64),
            },
            // SFTP v3 directory entries may omit the permissions/type bits.
            permissions: None,
            ..FileAttributes::default()
        }
    }

    impl russh_sftp::server::Handler for StatlessSftpHandler {
        type Error = StatusCode;

        fn unimplemented(&self) -> Self::Error {
            StatusCode::OpUnsupported
        }

        async fn open(
            &mut self,
            id: u32,
            filename: String,
            pflags: OpenFlags,
            _attrs: FileAttributes,
        ) -> Result<Handle, Self::Error> {
            let mut state = self.state.lock().expect("test SFTP state lock");
            if !state.parent_is_directory(&filename) {
                return Err(StatusCode::NoSuchFile);
            }
            if matches!(state.nodes.get(&filename), Some(TestNode::Directory)) {
                return Err(StatusCode::Failure);
            }
            if pflags.contains(OpenFlags::EXCLUDE) && state.nodes.contains_key(&filename) {
                return Err(StatusCode::Failure);
            }
            if pflags.contains(OpenFlags::CREATE) {
                state
                    .nodes
                    .entry(filename.clone())
                    .or_insert_with(|| TestNode::File(Vec::new()));
            }
            if pflags.contains(OpenFlags::TRUNCATE) {
                state
                    .nodes
                    .insert(filename.clone(), TestNode::File(Vec::new()));
            }
            if !matches!(state.nodes.get(&filename), Some(TestNode::File(_))) {
                return Err(StatusCode::NoSuchFile);
            }

            let handle = format!("file-{id}");
            state
                .handles
                .insert(handle.clone(), TestHandle::File(filename));
            Ok(Handle { id, handle })
        }

        async fn write(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            data: Vec<u8>,
        ) -> Result<Status, Self::Error> {
            let mut state = self.state.lock().expect("test SFTP state lock");
            let path = match state.handles.get(&handle) {
                Some(TestHandle::File(path)) => path.clone(),
                _ => return Err(StatusCode::Failure),
            };
            let Some(TestNode::File(content)) = state.nodes.get_mut(&path) else {
                return Err(StatusCode::NoSuchFile);
            };
            let offset = offset as usize;
            if content.len() < offset + data.len() {
                content.resize(offset + data.len(), 0);
            }
            content[offset..offset + data.len()].copy_from_slice(&data);
            Ok(ok_status(id))
        }

        async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
            self.state
                .lock()
                .expect("test SFTP state lock")
                .handles
                .remove(&handle);
            Ok(ok_status(id))
        }

        async fn stat(
            &mut self,
            _id: u32,
            _path: String,
        ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
            Err(StatusCode::OpUnsupported)
        }

        async fn mkdir(
            &mut self,
            id: u32,
            path: String,
            _attrs: FileAttributes,
        ) -> Result<Status, Self::Error> {
            let mut state = self.state.lock().expect("test SFTP state lock");
            if state.nodes.contains_key(&path) {
                return Err(StatusCode::Failure);
            }
            if !state.parent_is_directory(&path) {
                return Err(StatusCode::NoSuchFile);
            }
            state.nodes.insert(path, TestNode::Directory);
            Ok(ok_status(id))
        }

        async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
            let mut state = self.state.lock().expect("test SFTP state lock");
            if !matches!(state.nodes.get(&path), Some(TestNode::Directory)) {
                return Err(StatusCode::NoSuchFile);
            }
            let handle = format!("dir-{id}");
            state
                .handles
                .insert(handle.clone(), TestHandle::Directory { path, read: false });
            Ok(Handle { id, handle })
        }

        async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
            let mut state = self.state.lock().expect("test SFTP state lock");
            let directory = match state.handles.get_mut(&handle) {
                Some(TestHandle::Directory { read, .. }) if *read => return Err(StatusCode::Eof),
                Some(TestHandle::Directory { path, read }) => {
                    *read = true;
                    path.clone()
                }
                _ => return Err(StatusCode::Failure),
            };
            let prefix = format!("{}/", directory.trim_end_matches('/'));
            let files = state
                .nodes
                .iter()
                .filter_map(|(path, node)| {
                    let name = path.strip_prefix(&prefix)?;
                    if name.is_empty() || name.contains('/') {
                        return None;
                    }
                    Some(File::new(name, test_file_attributes(node)))
                })
                .collect();
            Ok(Name { id, files })
        }

        async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
            let mut state = self.state.lock().expect("test SFTP state lock");
            if matches!(state.nodes.get(&filename), Some(TestNode::File(_))) {
                state.nodes.remove(&filename);
                Ok(ok_status(id))
            } else if state.nodes.contains_key(&filename) {
                Err(StatusCode::Failure)
            } else {
                Err(StatusCode::NoSuchFile)
            }
        }

        async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
            let mut state = self.state.lock().expect("test SFTP state lock");
            if matches!(state.nodes.get(&path), Some(TestNode::Directory))
                && state.is_empty_directory(&path)
            {
                state.nodes.remove(&path);
                Ok(ok_status(id))
            } else if state.nodes.contains_key(&path) {
                Err(StatusCode::Failure)
            } else {
                Err(StatusCode::NoSuchFile)
            }
        }
    }

    async fn statless_sftp(state: TestSftpState) -> (SftpSession, Arc<Mutex<TestSftpState>>) {
        let state = Arc::new(Mutex::new(state));
        let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        russh_sftp::server::run(
            server_stream,
            StatlessSftpHandler {
                state: state.clone(),
            },
        )
        .await;
        let client = SftpSession::new(client_stream)
            .await
            .expect("initialize test SFTP client");
        (client, state)
    }

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
        assert_eq!(resolve_remote_path("file.txt", "/", "/"), "/file.txt");
        assert_eq!(resolve_remote_path("~/docs", "/", "/"), "/docs");
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

    #[test]
    fn test_remote_parent_dir() {
        assert_eq!(
            remote_parent_dir("/home/user/file.txt"),
            Some("/home/user".to_string())
        );
        assert_eq!(remote_parent_dir("/file.txt"), Some("/".to_string()));
        assert_eq!(remote_parent_dir("./file.txt"), Some(".".to_string()));
        assert_eq!(remote_parent_dir("file.txt"), None);
        assert_eq!(remote_parent_dir("/"), None);
        assert_eq!(remote_parent_dir(""), None);
    }

    #[tokio::test]
    async fn writes_a_new_file_when_stat_is_unsupported_and_parent_exists() {
        let (sftp, state) = statless_sftp(TestSftpState::with_home()).await;

        write_remote_file(&sftp, "/home/test/new.txt", b"created")
            .await
            .expect("create file without STAT support");

        assert_eq!(
            state
                .lock()
                .expect("test SFTP state lock")
                .nodes
                .get("/home/test/new.txt"),
            Some(&TestNode::File(b"created".to_vec()))
        );
    }

    #[tokio::test]
    async fn resolves_an_existing_upload_directory_when_stat_is_unsupported() {
        let (sftp, _) = statless_sftp(TestSftpState::with_home()).await;

        assert_eq!(
            resolve_remote_upload_path(&sftp, "/home/test", "local.txt").await,
            "/home/test/local.txt"
        );
        assert_eq!(
            resolve_remote_upload_path(&sftp, "/home/test/new.txt", "local.txt").await,
            "/home/test/new.txt"
        );
    }

    #[tokio::test]
    async fn creates_exclusively_without_stat_and_preserves_existing_files() {
        let mut initial = TestSftpState::with_home();
        initial.nodes.insert(
            "/home/test/existing.txt".to_string(),
            TestNode::File(b"original".to_vec()),
        );
        let (sftp, state) = statless_sftp(initial).await;
        let create_options = WriteRemoteFileOptions {
            create_parent_dirs: false,
            overwrite: false,
        };

        write_remote_file_with_options(
            &sftp,
            "/home/test/new-exclusive.txt",
            b"new",
            create_options,
        )
        .await
        .expect("create a new file atomically without STAT support");
        assert!(write_remote_file_with_options(
            &sftp,
            "/home/test/existing.txt",
            b"replacement",
            create_options,
        )
        .await
        .is_err());

        let state = state.lock().expect("test SFTP state lock");
        assert_eq!(
            state.nodes.get("/home/test/new-exclusive.txt"),
            Some(&TestNode::File(b"new".to_vec()))
        );
        assert_eq!(
            state.nodes.get("/home/test/existing.txt"),
            Some(&TestNode::File(b"original".to_vec()))
        );
    }

    #[tokio::test]
    async fn creates_nested_directories_when_stat_is_unsupported() {
        let (sftp, state) = statless_sftp(TestSftpState::with_home()).await;

        sftp_mkdir_recursive(&sftp, "/home/test/nested/leaf")
            .await
            .expect("create nested directories without STAT support");

        let state = state.lock().expect("test SFTP state lock");
        assert_eq!(
            state.nodes.get("/home/test/nested"),
            Some(&TestNode::Directory)
        );
        assert_eq!(
            state.nodes.get("/home/test/nested/leaf"),
            Some(&TestNode::Directory)
        );
    }

    #[tokio::test]
    async fn deletes_files_and_directories_when_stat_is_unsupported() {
        let mut initial = TestSftpState::with_home();
        initial.nodes.extend([
            (
                "/home/test/delete-me.txt".to_string(),
                TestNode::File(Vec::new()),
            ),
            ("/home/test/tree".to_string(), TestNode::Directory),
            ("/home/test/tree/nested".to_string(), TestNode::Directory),
            (
                "/home/test/tree/nested/child.txt".to_string(),
                TestNode::File(Vec::new()),
            ),
        ]);
        let (sftp, state) = statless_sftp(initial).await;

        sftp_delete_path(&sftp, "/home/test/delete-me.txt", false)
            .await
            .expect("delete file without STAT support");
        sftp_delete_path(&sftp, "/home/test/tree", true)
            .await
            .expect("delete directory without STAT support");

        let state = state.lock().expect("test SFTP state lock");
        assert!(!state.nodes.contains_key("/home/test/delete-me.txt"));
        assert!(!state.nodes.contains_key("/home/test/tree/nested/child.txt"));
        assert!(!state.nodes.contains_key("/home/test/tree/nested"));
        assert!(!state.nodes.contains_key("/home/test/tree"));
    }
}
