use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use vibeshell_core::commands::sftp::{SftpEntry, SftpFileContent};
use vibeshell_core::ipc::{IpcMessage, IpcSessionInfo};
use vibeshell_core::sftp::{DirectoryTransferMode, DirectoryTransferSummary, TransferProgress};

use crate::ipc_support;

pub fn connect(server_name: Option<&str>, session_id: Option<&str>, args: &[String]) -> Result<()> {
    let (sid, owns_session) = if let Some(existing_id) = session_id {
        // Use existing session - don't create a new one
        (existing_id.to_string(), false)
    } else {
        let name =
            server_name.ok_or_else(|| anyhow!("Either server name or --session is required"))?;
        if let Some(info) = find_reusable_session(name)? {
            (info.id, false)
        } else {
            (create_session(name)?, true)
        }
    };

    let result = (|| -> Result<()> {
        let home = init_sftp(&sid)?;
        let mut cwd = home.clone();
        let display_name = if let Some(name) = server_name {
            name.to_string()
        } else {
            format!("session {}", sid)
        };

        if args.is_empty() {
            run_repl(&display_name, &sid, &home, &mut cwd)
        } else {
            run_direct_command(&sid, &home, &mut cwd, args)
        }
    })();

    // Only kill session if we created it
    if owns_session {
        let _ = ipc_support::send(&IpcMessage::KillSession {
            session_id: sid.clone(),
        });
    }

    result
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

fn init_sftp(session_id: &str) -> Result<String> {
    match ipc_support::send(&IpcMessage::SftpInit {
        session_id: session_id.to_string(),
    })? {
        IpcMessage::Ok => pwd(session_id),
        IpcMessage::Error { message } => bail!("Failed to initialize SFTP: {}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn run_repl(server_name: &str, session_id: &str, home: &str, cwd: &mut String) -> Result<()> {
    println!("Connected to '{}' via SFTP.", server_name);
    println!("Remote directory: {}", cwd);
    println!(
        "Commands: pwd, ls [path], cd <path>, get <remote> [local], put <local> [remote], sync <local-dir> [remote-dir] [--delete], cat <path>, mkdir <path>, rm <path>, mv <old> <new>, help, quit"
    );

    let stdin = io::stdin();
    loop {
        print!("sftp:{}> ", cwd);
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            println!();
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts = match parse_repl_line(line) {
            Ok(parts) => parts,
            Err(error) => {
                eprintln!("Error: {}", error);
                continue;
            }
        };
        if parts.is_empty() {
            continue;
        }
        if matches!(parts[0].as_str(), "quit" | "exit") {
            break;
        }

        if let Err(error) = run_direct_command(session_id, home, cwd, &parts) {
            eprintln!("Error: {}", error);
        }
    }

    Ok(())
}

fn run_direct_command(
    session_id: &str,
    home: &str,
    cwd: &mut String,
    args: &[String],
) -> Result<()> {
    let command = args
        .first()
        .map(|value| value.as_str())
        .ok_or_else(|| anyhow!("Missing SFTP command"))?;

    match command {
        "help" => {
            print_help();
            Ok(())
        }
        "pwd" => {
            println!("{}", cwd);
            Ok(())
        }
        "ls" => {
            let target =
                resolve_remote_arg(args.get(1).map(|v| v.as_str()).unwrap_or("."), home, cwd)?;
            let entries = list_dir(session_id, &target, true)?;
            print_entries(&entries);
            Ok(())
        }
        "cd" => {
            let target = required_arg(args, 1, "Usage: cd <path>")?;
            let target = resolve_remote_arg(target, home, cwd)?;
            list_dir(session_id, &target, false)?;
            *cwd = target;
            println!("{}", cwd);
            Ok(())
        }
        "cat" => {
            let target = required_arg(args, 1, "Usage: cat <path>")?;
            let target = resolve_remote_arg(target, home, cwd)?;
            let content = read_file(session_id, &target)?;
            print!("{}", content.content);
            if !content.content.ends_with('\n') {
                println!();
            }
            if content.truncated {
                println!("[truncated at {} bytes]", content.size);
            }
            Ok(())
        }
        "mkdir" => {
            let target = required_arg(args, 1, "Usage: mkdir <path>")?;
            let target = resolve_remote_arg(target, home, cwd)?;
            expect_ok(ipc_support::send(&IpcMessage::SftpMkdir {
                session_id: session_id.to_string(),
                path: target.to_string(),
            })?)
        }
        "rm" => {
            let target = required_arg(args, 1, "Usage: rm <path>")?;
            let target = resolve_remote_arg(target, home, cwd)?;
            expect_ok(ipc_support::send(&IpcMessage::SftpDelete {
                session_id: session_id.to_string(),
                path: target.to_string(),
                recursive: true,
            })?)
        }
        "mv" => {
            let old_path = required_arg(args, 1, "Usage: mv <old> <new>")?;
            let new_path = required_arg(args, 2, "Usage: mv <old> <new>")?;
            let old_path = resolve_remote_arg(old_path, home, cwd)?;
            let new_path = resolve_remote_arg(new_path, home, cwd)?;
            expect_ok(ipc_support::send(&IpcMessage::SftpRename {
                session_id: session_id.to_string(),
                old_path: old_path.to_string(),
                new_path: new_path.to_string(),
            })?)
        }
        "get" => {
            let remote = required_arg(args, 1, "Usage: get <remote> [local]")?;
            let remote = resolve_remote_arg(remote, home, cwd)?;
            let local_path = prepare_download_local_path(&remote, args.get(2).map(String::as_str))?;
            let local = local_path.to_string_lossy().to_string();
            let progress = download_file(session_id, &remote, &local)?;
            println!(
                "Downloaded {} -> {} ({} bytes)",
                remote, local, progress.transferred_bytes
            );
            Ok(())
        }
        "put" => {
            let local = required_arg(args, 1, "Usage: put <local> [remote]")?;
            let local_path = prepare_upload_local_path(local)?;
            let (remote_arg, directory_options) = parse_directory_command_options(args, 2)?;
            let remote = remote_arg.unwrap_or_else(|| default_upload_target(&local_path));
            let remote = resolve_remote_arg(&remote, home, cwd)?;
            let local_path = local_path.to_string_lossy().to_string();
            if Path::new(&local_path).is_dir() {
                let summary = upload_directory(
                    session_id,
                    &local_path,
                    &remote,
                    DirectoryTransferMode::Upload,
                    directory_options,
                )?;
                print_directory_summary(&summary);
            } else {
                let progress = upload_file(session_id, &local_path, &remote)?;
                println!(
                    "Uploaded {} -> {} ({} bytes)",
                    local_path, remote, progress.transferred_bytes
                );
            }
            Ok(())
        }
        "sync" => {
            let local = required_arg(args, 1, "Usage: sync <local-dir> [remote-dir] [--delete]")?;
            let local_path = prepare_upload_local_path(local)?;
            if !local_path.is_dir() {
                bail!("Local sync path must be a directory: {}", local_path.display());
            }
            let (remote_arg, directory_options) = parse_directory_command_options(args, 2)?;
            let remote = remote_arg.unwrap_or_else(|| default_upload_target(&local_path));
            let remote = resolve_remote_arg(&remote, home, cwd)?;
            let local_path = local_path.to_string_lossy().to_string();
            let summary = upload_directory(
                session_id,
                &local_path,
                &remote,
                DirectoryTransferMode::Sync,
                directory_options,
            )?;
            print_directory_summary(&summary);
            Ok(())
        }
        other => bail!(
            "Unknown SFTP command '{}'. Supported: pwd, ls, cd, get, put, sync, cat, mkdir, rm, mv, help",
            other
        ),
    }
}

fn required_arg<'a>(args: &'a [String], index: usize, usage: &str) -> Result<&'a str> {
    args.get(index)
        .map(|value| value.as_str())
        .ok_or_else(|| anyhow!(usage.to_string()))
}

fn parse_repl_line(line: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut in_token = false;

    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            in_token = true;
            continue;
        }

        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            in_token = true;
            continue;
        }

        match quote {
            Some(active_quote) if ch == active_quote => {
                quote = None;
                in_token = true;
            }
            Some(_) => {
                current.push(ch);
                in_token = true;
            }
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                in_token = true;
            }
            None if ch.is_whitespace() => {
                if in_token {
                    args.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            None => {
                current.push(ch);
                in_token = true;
            }
        }
    }

    if escaped {
        current.push('\\');
    }
    if let Some(active_quote) = quote {
        bail!("Unclosed quote: {}", active_quote);
    }
    if in_token {
        args.push(current);
    }

    Ok(args)
}

fn resolve_remote_arg(path: &str, home: &str, cwd: &str) -> Result<String> {
    if path.is_empty() {
        bail!("Remote path cannot be empty");
    }

    let absolute = if path.starts_with('/') {
        path.to_string()
    } else if path == "~" {
        home.to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        join_remote_path(home, rest)
    } else {
        join_remote_path(cwd, path)
    };

    Ok(normalize_remote_path(&absolute))
}

fn join_remote_path(base: &str, child: &str) -> String {
    if child.is_empty() {
        return base.to_string();
    }

    let base = if base.is_empty() { "/" } else { base };
    if base == "/" {
        format!("/{}", child.trim_start_matches('/'))
    } else {
        format!(
            "{}/{}",
            base.trim_end_matches('/'),
            child.trim_start_matches('/')
        )
    }
}

fn normalize_remote_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts = Vec::new();

    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }

    if absolute {
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    } else if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn list_dir(session_id: &str, path: &str, preserve_cwd: bool) -> Result<Vec<SftpEntry>> {
    match ipc_support::send(&IpcMessage::SftpListDir {
        session_id: session_id.to_string(),
        path: path.to_string(),
        preserve_cwd,
    })? {
        IpcMessage::SftpEntries { entries } => Ok(entries),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn pwd(session_id: &str) -> Result<String> {
    match ipc_support::send(&IpcMessage::SftpPwd {
        session_id: session_id.to_string(),
    })? {
        IpcMessage::SftpPath { path } => Ok(path),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn read_file(session_id: &str, path: &str) -> Result<SftpFileContent> {
    match ipc_support::send(&IpcMessage::SftpReadFile {
        session_id: session_id.to_string(),
        path: path.to_string(),
        max_size: Some(1024 * 1024),
        as_binary: Some(false),
    })? {
        IpcMessage::SftpFileContent { content } => Ok(content),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

#[derive(Default)]
struct DirectoryCommandOptions {
    delete_extra: bool,
    respect_gitignore: Option<bool>,
    excluded_paths: Vec<String>,
}

fn parse_directory_command_options(
    args: &[String],
    start_index: usize,
) -> Result<(Option<String>, DirectoryCommandOptions)> {
    let mut remote = None;
    let mut options = DirectoryCommandOptions::default();
    let mut index = start_index;

    while index < args.len() {
        let value = &args[index];
        if value == "--delete" {
            options.delete_extra = true;
        } else if value == "--no-gitignore" {
            options.respect_gitignore = Some(false);
        } else if value == "--gitignore" {
            options.respect_gitignore = Some(true);
        } else if value == "--exclude" {
            index += 1;
            let pattern = args
                .get(index)
                .ok_or_else(|| anyhow!("Usage: --exclude <pattern>"))?;
            options.excluded_paths.push(pattern.clone());
        } else if let Some(pattern) = value.strip_prefix("--exclude=") {
            if pattern.is_empty() {
                bail!("Usage: --exclude=<pattern>");
            }
            options.excluded_paths.push(pattern.to_string());
        } else if value.starts_with("--") {
            bail!("Unknown directory transfer option: {}", value);
        } else if remote.is_none() {
            remote = Some(value.clone());
        } else {
            bail!("Unexpected argument: {}", value);
        }
        index += 1;
    }

    Ok((remote, options))
}

fn download_file(session_id: &str, remote: &str, local: &str) -> Result<TransferProgress> {
    match ipc_support::send(&IpcMessage::SftpDownloadFile {
        session_id: session_id.to_string(),
        remote_path: remote.to_string(),
        local_path: local.to_string(),
    })? {
        IpcMessage::SftpTransfer { progress } => Ok(progress),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn upload_file(session_id: &str, local: &str, remote: &str) -> Result<TransferProgress> {
    match ipc_support::send(&IpcMessage::SftpUploadFile {
        session_id: session_id.to_string(),
        local_path: local.to_string(),
        remote_path: remote.to_string(),
    })? {
        IpcMessage::SftpTransfer { progress } => Ok(progress),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn upload_directory(
    session_id: &str,
    local: &str,
    remote: &str,
    mode: DirectoryTransferMode,
    options: DirectoryCommandOptions,
) -> Result<DirectoryTransferSummary> {
    match ipc_support::send(&IpcMessage::SftpUploadDirectory {
        session_id: session_id.to_string(),
        local_path: local.to_string(),
        remote_path: remote.to_string(),
        mode,
        delete_extra: options.delete_extra,
        respect_gitignore: options.respect_gitignore,
        excluded_paths: options.excluded_paths,
    })? {
        IpcMessage::SftpDirectoryTransfer { summary } => Ok(summary),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn prepare_download_local_path(remote: &str, local: Option<&str>) -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("Failed to resolve current directory")?;
    resolve_download_local_path(remote, local, &current_dir)
}

fn resolve_download_local_path(
    remote: &str,
    local: Option<&str>,
    current_dir: &Path,
) -> Result<PathBuf> {
    let basename = default_download_target(remote);
    let target = match local {
        Some("") => bail!("Local download path cannot be empty"),
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(&basename),
    };

    let mut resolved = if target.is_absolute() {
        target
    } else {
        current_dir.join(target)
    };

    if local.map(path_text_ends_with_separator).unwrap_or(false) || resolved.is_dir() {
        resolved = resolved.join(&basename);
    }

    Ok(resolved)
}

fn path_text_ends_with_separator(path: &str) -> bool {
    path.ends_with('/') || path.ends_with('\\')
}

fn prepare_upload_local_path(local: &str) -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("Failed to resolve current directory")?;
    resolve_upload_local_path(local, &current_dir)
}

fn resolve_upload_local_path(local: &str, current_dir: &Path) -> Result<PathBuf> {
    if local.is_empty() {
        bail!("Usage: put <local> [remote]");
    }

    let path = PathBuf::from(local);
    let resolved = if path.is_absolute() {
        path
    } else {
        current_dir.join(path)
    };

    let metadata = std::fs::metadata(&resolved).with_context(|| {
        format!(
            "Failed to access local upload path '{}'",
            resolved.display()
        )
    })?;

    if !metadata.is_file() && !metadata.is_dir() {
        bail!(
            "Local upload path must be a file or directory: {}",
            resolved.display()
        );
    }

    Ok(resolved)
}

fn expect_ok(message: IpcMessage) -> Result<()> {
    match message {
        IpcMessage::Ok => Ok(()),
        IpcMessage::Error { message } => bail!("{}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn print_entries(entries: &[SftpEntry]) {
    for entry in entries {
        let kind = if entry.is_directory { "d" } else { "-" };
        println!(
            "{} {:>10} {} {}",
            kind, entry.size, entry.permissions, entry.name
        );
    }
}

fn print_directory_summary(summary: &DirectoryTransferSummary) {
    println!(
        "{} directory {} -> {}",
        if summary.mode == "sync" {
            "Synced"
        } else {
            "Uploaded"
        },
        summary.local_root,
        summary.remote_root
    );
    println!(
        "Files: {} uploaded, {} skipped, {} total; directories: {}; deleted: {}; bytes: {}",
        summary.uploaded_files,
        summary.skipped_files,
        summary.files_total,
        summary.created_directories,
        summary.deleted_entries,
        summary.transferred_bytes
    );
}

fn print_help() {
    println!("pwd");
    println!("ls [path]");
    println!("cd <path>");
    println!("get <remote> [local]");
    println!("put <local-file-or-dir> [remote] [--exclude <pattern>] [--no-gitignore]");
    println!("sync <local-dir> [remote-dir] [--delete] [--exclude <pattern>] [--no-gitignore]");
    println!("cat <path>");
    println!("mkdir <path>");
    println!("rm <path>");
    println!("mv <old> <new>");
    println!("help");
    println!("quit");
}

fn default_download_target(remote: &str) -> String {
    remote_basename(remote).unwrap_or("download").to_string()
}

fn remote_basename(remote: &str) -> Option<&str> {
    let trimmed = remote.trim_end_matches('/');
    if trimmed.is_empty() {
        None
    } else {
        trimmed.rsplit('/').find(|part| !part.is_empty())
    }
}

fn default_upload_target(local_path: &Path) -> String {
    local_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("upload")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        default_download_target, default_upload_target, parse_repl_line,
        resolve_download_local_path, resolve_remote_arg, resolve_upload_local_path,
    };
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vshell-sftp-{}-{}-{}",
            name,
            std::process::id(),
            stamp
        ))
    }

    #[test]
    fn resolve_upload_local_path_uses_cli_current_dir_for_relative_paths() {
        let dir = unique_temp_dir("relative");
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("payload.txt"), b"payload").expect("write temp file");

        let resolved = resolve_upload_local_path("payload.txt", &dir).expect("resolve upload path");

        assert_eq!(resolved, dir.join("payload.txt"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_upload_local_path_accepts_directories() {
        let dir = unique_temp_dir("directory");
        fs::create_dir_all(&dir).expect("create temp dir");

        let resolved = resolve_upload_local_path(".", &dir)
            .expect("directories should be accepted for recursive upload");

        assert_eq!(resolved, dir.join("."));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_upload_target_uses_source_filename() {
        assert_eq!(
            default_upload_target(Path::new("nested").join("payload.txt").as_path()),
            "payload.txt"
        );
    }

    #[test]
    fn parse_repl_line_supports_quoted_paths() {
        let parsed = parse_repl_line(r#"put "local file.txt" '/tmp/remote file.txt'"#)
            .expect("parse quoted SFTP command");

        assert_eq!(
            parsed,
            vec![
                "put".to_string(),
                "local file.txt".to_string(),
                "/tmp/remote file.txt".to_string()
            ]
        );
    }

    #[test]
    fn parse_directory_command_options_reads_flags() {
        let args = vec![
            "sync".to_string(),
            "dist".to_string(),
            "/var/www".to_string(),
            "--delete".to_string(),
            "--exclude=node_modules/".to_string(),
            "--no-gitignore".to_string(),
        ];
        let (remote, options) =
            super::parse_directory_command_options(&args, 2).expect("parse directory options");

        assert_eq!(remote.as_deref(), Some("/var/www"));
        assert!(options.delete_extra);
        assert_eq!(options.respect_gitignore, Some(false));
        assert_eq!(options.excluded_paths, vec!["node_modules/".to_string()]);
    }

    #[test]
    fn resolve_remote_arg_uses_cli_cwd_and_home() {
        assert_eq!(
            resolve_remote_arg("payload.txt", "/home/me", "/home/me/work").unwrap(),
            "/home/me/work/payload.txt"
        );
        assert_eq!(
            resolve_remote_arg("../logs", "/home/me", "/home/me/work").unwrap(),
            "/home/me/logs"
        );
        assert_eq!(
            resolve_remote_arg("~/payload.txt", "/home/me", "/tmp").unwrap(),
            "/home/me/payload.txt"
        );
        assert_eq!(
            resolve_remote_arg("/var//log/../tmp", "/home/me", "/tmp").unwrap(),
            "/var/tmp"
        );
    }

    #[test]
    fn default_download_target_uses_remote_filename() {
        assert_eq!(default_download_target("/tmp/payload.txt"), "payload.txt");
        assert_eq!(default_download_target("/tmp/archive/"), "archive");
    }

    #[test]
    fn resolve_download_local_path_uses_cli_current_dir_for_relative_paths() {
        let dir = unique_temp_dir("download-relative");
        fs::create_dir_all(&dir).expect("create temp dir");

        let resolved =
            resolve_download_local_path("/remote/payload.txt", Some("payload.txt"), &dir)
                .expect("resolve download path");

        assert_eq!(resolved, dir.join("payload.txt"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_download_local_path_appends_remote_filename_for_directories() {
        let dir = unique_temp_dir("download-directory");
        let download_dir = dir.join("downloads");
        fs::create_dir_all(&download_dir).expect("create download dir");

        let resolved = resolve_download_local_path(
            "/remote/payload.txt",
            Some(download_dir.to_str().unwrap()),
            &dir,
        )
        .expect("resolve download directory path");

        assert_eq!(resolved, download_dir.join("payload.txt"));
        let _ = fs::remove_dir_all(&dir);
    }
}
