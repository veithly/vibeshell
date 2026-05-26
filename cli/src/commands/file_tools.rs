//! Agent-friendly remote file tools for the CLI.

use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use vibeshell_core::commands::sftp::{SftpEntry, SftpFileContent};
use vibeshell_core::ipc::{IpcMessage, IpcSessionInfo};
use vibeshell_core::remote_tools::{build_remote_rg_command, RemoteSearchOptions};

use crate::ipc_support;
use crate::session_alias;

#[derive(Args, Clone, Debug, Default)]
pub struct ContentInputArgs {
    /// Text content to write
    #[arg(long)]
    pub content: Option<String>,

    /// Read text content from a local file
    #[arg(long = "content-file", value_name = "PATH")]
    pub content_file: Option<PathBuf>,

    /// Read text content from stdin
    #[arg(long = "content-stdin")]
    pub content_stdin: bool,
}

impl ContentInputArgs {
    pub fn has_source(&self) -> bool {
        self.source_count() > 0
    }

    pub fn resolve(&self) -> Result<String> {
        match self.source_count() {
            0 => bail!("Use one of --content, --content-file, or --content-stdin"),
            1 => {}
            _ => {
                bail!("Use only one content source: --content, --content-file, or --content-stdin")
            }
        }

        if let Some(content) = &self.content {
            return Ok(content.clone());
        }

        if let Some(path) = &self.content_file {
            return std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read content file {}", path.display()));
        }

        let mut content = String::new();
        std::io::stdin()
            .read_to_string(&mut content)
            .context("Failed to read content from stdin")?;
        Ok(content)
    }

    fn source_count(&self) -> usize {
        usize::from(self.content.is_some())
            + usize::from(self.content_file.is_some())
            + usize::from(self.content_stdin)
    }
}

pub fn get_content(
    target: &str,
    target_is_session: bool,
    path: &str,
    max_bytes: u64,
) -> Result<()> {
    with_sftp_session(target, target_is_session, |session_id| {
        let content = read_file(session_id, path, max_bytes)?;
        print!("{}", content.content);
        if !content.content.ends_with('\n') {
            println!();
        }
        if content.truncated {
            eprintln!("[truncated at {} bytes]", content.size);
        }
        std::io::stdout().flush()?;
        Ok(())
    })
}

pub fn add_file(
    target: &str,
    target_is_session: bool,
    path: &str,
    content_input: &ContentInputArgs,
    overwrite: bool,
    parents: bool,
) -> Result<()> {
    let content = content_input.resolve()?;
    with_sftp_session(target, target_is_session, |session_id| {
        if !overwrite && remote_path_exists(session_id, path)? {
            bail!(
                "Remote path already exists: {}. Use --overwrite to replace it.",
                path
            );
        }

        if parents {
            if let Some(parent) = remote_parent_path(path) {
                mkdir(session_id, &parent)?;
            }
        }

        write_file(session_id, path, &content)?;
        println!("Added {} ({} bytes)", path, content.len());
        Ok(())
    })
}

pub fn edit_file(
    target: &str,
    target_is_session: bool,
    path: &str,
    content_input: &ContentInputArgs,
    old_text: Option<&str>,
    new_text: Option<&str>,
    replace_all: bool,
) -> Result<()> {
    let replace_mode = old_text.is_some() || new_text.is_some();
    if replace_mode && content_input.has_source() {
        bail!("Use either a content source or --replace/--with, not both");
    }

    with_sftp_session(target, target_is_session, |session_id| {
        ensure_existing_file(session_id, path)?;

        if replace_mode {
            let old_text = old_text.ok_or_else(|| anyhow!("Missing --replace <old-text>"))?;
            let new_text = new_text.ok_or_else(|| anyhow!("Missing --with <new-text>"))?;
            let current = read_file(session_id, path, 10 * 1024 * 1024)?;
            if current.truncated {
                bail!(
                    "Refusing to edit truncated content from {} ({} bytes)",
                    path,
                    current.size
                );
            }

            let (updated, replacements) =
                replace_text(&current.content, old_text, new_text, replace_all)?;
            write_file(session_id, path, &updated)?;
            println!("Edited {} ({} replacement(s))", path, replacements);
            return Ok(());
        }

        let content = content_input.resolve()?;
        write_file(session_id, path, &content)?;
        println!("Edited {} ({} bytes)", path, content.len());
        Ok(())
    })
}

#[allow(clippy::too_many_arguments)]
pub fn rg(
    target: &str,
    target_is_session: bool,
    pattern: &str,
    path: Option<&str>,
    ignore_case: bool,
    fixed_strings: bool,
    hidden: bool,
    globs: Vec<String>,
    max_results: usize,
) -> Result<()> {
    with_session(target, target_is_session, |session_id| {
        let mut options = RemoteSearchOptions::new(pattern, path.unwrap_or("."));
        options.ignore_case = ignore_case;
        options.fixed_strings = fixed_strings;
        options.hidden = hidden;
        options.globs = globs;
        options.max_results = max_results;

        let command = build_remote_rg_command(&options);
        let output = ipc_support::exec_isolated(session_id, &command)?;
        print!("{}", output);
        if !output.is_empty() && !output.ends_with('\n') {
            println!();
        }
        std::io::stdout().flush()?;
        Ok(())
    })
}

struct SessionLease {
    session_id: String,
    owns_session: bool,
}

fn with_sftp_session<T>(
    target: &str,
    target_is_session: bool,
    action: impl FnOnce(&str) -> Result<T>,
) -> Result<T> {
    with_session(target, target_is_session, |session_id| {
        init_sftp(session_id)?;
        action(session_id)
    })
}

fn with_session<T>(
    target: &str,
    target_is_session: bool,
    action: impl FnOnce(&str) -> Result<T>,
) -> Result<T> {
    let lease = acquire_session(target, target_is_session)?;
    let result = action(&lease.session_id);

    if lease.owns_session {
        let _ = ipc_support::send(&IpcMessage::KillSession {
            session_id: lease.session_id.clone(),
        });
        let _ = session_alias::remove_by_session_id(&lease.session_id);
    }

    result
}

fn acquire_session(target: &str, target_is_session: bool) -> Result<SessionLease> {
    if target_is_session {
        let session_id = session_alias::resolve(target).unwrap_or_else(|| target.to_string());
        return Ok(SessionLease {
            session_id,
            owns_session: false,
        });
    }

    if let Some(info) = find_reusable_session(target)? {
        return Ok(SessionLease {
            session_id: info.id,
            owns_session: false,
        });
    }

    let session_id = create_session(target)?;
    Ok(SessionLease {
        session_id,
        owns_session: true,
    })
}

fn find_reusable_session(server_name: &str) -> Result<Option<IpcSessionInfo>> {
    match ipc_support::send(&IpcMessage::ListSessions)? {
        IpcMessage::SessionList { sessions } => Ok(sessions
            .into_iter()
            .filter(|info| info.server_name == server_name)
            .filter(|info| matches!(info.state.as_str(), "connected" | "connecting"))
            .min_by_key(|info| info.created_at)),
        IpcMessage::Error { message } => bail!("Error listing sessions: {}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn create_session(server_name: &str) -> Result<String> {
    match ipc_support::send(&IpcMessage::CreateSession {
        server_name: server_name.to_string(),
    })? {
        IpcMessage::SessionCreated { session_id } => Ok(session_id),
        IpcMessage::Error { message } => {
            bail!("Error connecting to server '{}': {}", server_name, message);
        }
        _ => bail!("Unexpected response from background service"),
    }
}

fn init_sftp(session_id: &str) -> Result<()> {
    match ipc_support::send(&IpcMessage::SftpInit {
        session_id: session_id.to_string(),
    })? {
        IpcMessage::Ok => Ok(()),
        IpcMessage::Error { message } => bail!("Failed to initialize SFTP: {}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn read_file(session_id: &str, path: &str, max_bytes: u64) -> Result<SftpFileContent> {
    match ipc_support::send(&IpcMessage::SftpReadFile {
        session_id: session_id.to_string(),
        path: path.to_string(),
        max_size: Some(max_bytes),
        as_binary: Some(false),
    })? {
        IpcMessage::SftpFileContent { content } => Ok(content),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn write_file(session_id: &str, path: &str, content: &str) -> Result<()> {
    match ipc_support::send(&IpcMessage::SftpWriteFile {
        session_id: session_id.to_string(),
        path: path.to_string(),
        content: content.to_string(),
    })? {
        IpcMessage::Ok => Ok(()),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn mkdir(session_id: &str, path: &str) -> Result<()> {
    match ipc_support::send(&IpcMessage::SftpMkdir {
        session_id: session_id.to_string(),
        path: path.to_string(),
    })? {
        IpcMessage::Ok => Ok(()),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn remote_path_exists(session_id: &str, path: &str) -> Result<bool> {
    match ipc_support::send(&IpcMessage::SftpStat {
        session_id: session_id.to_string(),
        path: path.to_string(),
    })? {
        IpcMessage::SftpStatResult { .. } => Ok(true),
        IpcMessage::Error { .. } => Ok(false),
        _ => bail!("Unexpected response from background service"),
    }
}

fn ensure_existing_file(session_id: &str, path: &str) -> Result<SftpEntry> {
    match ipc_support::send(&IpcMessage::SftpStat {
        session_id: session_id.to_string(),
        path: path.to_string(),
    })? {
        IpcMessage::SftpStatResult { entry } if entry.is_directory => {
            bail!("Remote path is a directory, not a file: {}", path)
        }
        IpcMessage::SftpStatResult { entry } => Ok(entry),
        IpcMessage::Error { message } => {
            bail!("Remote file does not exist: {} ({})", path, message)
        }
        _ => bail!("Unexpected response from background service"),
    }
}

fn replace_text(
    content: &str,
    old_text: &str,
    new_text: &str,
    replace_all: bool,
) -> Result<(String, usize)> {
    if old_text.is_empty() {
        bail!("--replace text cannot be empty");
    }

    if replace_all {
        let replacements = content.matches(old_text).count();
        if replacements == 0 {
            bail!("Text to replace was not found");
        }
        return Ok((content.replace(old_text, new_text), replacements));
    }

    let Some(index) = content.find(old_text) else {
        bail!("Text to replace was not found");
    };

    let mut updated = String::with_capacity(content.len() - old_text.len() + new_text.len());
    updated.push_str(&content[..index]);
    updated.push_str(new_text);
    updated.push_str(&content[index + old_text.len()..]);
    Ok((updated, 1))
}

fn remote_parent_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }

    let index = trimmed.rfind('/')?;
    if index == 0 {
        Some("/".to_string())
    } else {
        Some(trimmed[..index].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{remote_parent_path, replace_text, ContentInputArgs};

    #[test]
    fn replace_text_replaces_once_by_default() {
        let (updated, count) =
            replace_text("hello hello", "hello", "hi", false).expect("replace text");

        assert_eq!(updated, "hi hello");
        assert_eq!(count, 1);
    }

    #[test]
    fn replace_text_can_replace_all_occurrences() {
        let (updated, count) =
            replace_text("hello hello", "hello", "hi", true).expect("replace text");

        assert_eq!(updated, "hi hi");
        assert_eq!(count, 2);
    }

    #[test]
    fn remote_parent_path_handles_absolute_and_relative_paths() {
        assert_eq!(
            remote_parent_path("/etc/nginx/nginx.conf").as_deref(),
            Some("/etc/nginx")
        );
        assert_eq!(remote_parent_path("/payload.txt").as_deref(), Some("/"));
        assert_eq!(
            remote_parent_path("nested/payload.txt").as_deref(),
            Some("nested")
        );
        assert_eq!(remote_parent_path("payload.txt"), None);
    }

    #[test]
    fn content_input_rejects_missing_source() {
        let args = ContentInputArgs::default();
        let error = args.resolve().expect_err("missing source should fail");

        assert!(error.to_string().contains("--content"));
    }
}
