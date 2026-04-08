use std::io::{self, Write};
use std::path::Path;

use anyhow::{anyhow, bail, Result};
use vibeshell_core::commands::sftp::{SftpEntry, SftpFileContent};
use vibeshell_core::ipc::IpcMessage;
use vibeshell_core::sftp::TransferProgress;

use crate::ipc_support;

pub fn connect(server_name: Option<&str>, session_id: Option<&str>, args: &[String]) -> Result<()> {
    let (sid, owns_session) = if let Some(existing_id) = session_id {
        // Use existing session - don't create a new one
        (existing_id.to_string(), false)
    } else {
        let name =
            server_name.ok_or_else(|| anyhow!("Either server name or --session is required"))?;
        (create_session(name)?, true)
    };

    let mut cwd = init_sftp(&sid)?;
    let display_name = if let Some(name) = server_name {
        name.to_string()
    } else {
        format!("session {}", sid)
    };

    let result = if args.is_empty() {
        run_repl(&display_name, &sid, &mut cwd)
    } else {
        run_direct_command(&sid, &mut cwd, args)
    };

    // Only kill session if we created it
    if owns_session {
        let _ = ipc_support::send(&IpcMessage::KillSession {
            session_id: sid.clone(),
        });
    }

    result
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

fn run_repl(server_name: &str, session_id: &str, cwd: &mut String) -> Result<()> {
    println!("Connected to '{}' via SFTP.", server_name);
    println!("Remote directory: {}", cwd);
    println!(
        "Commands: pwd, ls [path], cd <path>, get <remote> [local], put <local> [remote], cat <path>, mkdir <path>, rm <path>, mv <old> <new>, help, quit"
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

        let parts: Vec<String> = line
            .split_whitespace()
            .map(|part| part.to_string())
            .collect();
        if matches!(parts[0].as_str(), "quit" | "exit") {
            break;
        }

        run_direct_command(session_id, cwd, &parts)?;
    }

    Ok(())
}

fn run_direct_command(session_id: &str, cwd: &mut String, args: &[String]) -> Result<()> {
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
            *cwd = pwd(session_id)?;
            println!("{}", cwd);
            Ok(())
        }
        "ls" => {
            let target = args.get(1).map(|v| v.as_str()).unwrap_or(".");
            let entries = list_dir(session_id, target)?;
            if target != "." {
                *cwd = pwd(session_id)?;
            }
            print_entries(&entries);
            Ok(())
        }
        "cd" => {
            let target = required_arg(args, 1, "Usage: cd <path>")?;
            list_dir(session_id, target)?;
            *cwd = pwd(session_id)?;
            println!("{}", cwd);
            Ok(())
        }
        "cat" => {
            let target = required_arg(args, 1, "Usage: cat <path>")?;
            let content = read_file(session_id, target)?;
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
            expect_ok(ipc_support::send(&IpcMessage::SftpMkdir {
                session_id: session_id.to_string(),
                path: target.to_string(),
            })?)
        }
        "rm" => {
            let target = required_arg(args, 1, "Usage: rm <path>")?;
            expect_ok(ipc_support::send(&IpcMessage::SftpDelete {
                session_id: session_id.to_string(),
                path: target.to_string(),
                recursive: true,
            })?)
        }
        "mv" => {
            let old_path = required_arg(args, 1, "Usage: mv <old> <new>")?;
            let new_path = required_arg(args, 2, "Usage: mv <old> <new>")?;
            expect_ok(ipc_support::send(&IpcMessage::SftpRename {
                session_id: session_id.to_string(),
                old_path: old_path.to_string(),
                new_path: new_path.to_string(),
            })?)
        }
        "get" => {
            let remote = required_arg(args, 1, "Usage: get <remote> [local]")?;
            let local = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| default_download_target(remote));
            let progress = download_file(session_id, remote, &local)?;
            println!(
                "Downloaded {} -> {} ({} bytes)",
                remote, local, progress.transferred_bytes
            );
            Ok(())
        }
        "put" => {
            let local = required_arg(args, 1, "Usage: put <local> [remote]")?;
            let remote = args.get(2).cloned().unwrap_or_else(|| {
                Path::new(local)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(local)
                    .to_string()
            });
            let progress = upload_file(session_id, local, &remote)?;
            println!(
                "Uploaded {} -> {} ({} bytes)",
                local, remote, progress.transferred_bytes
            );
            Ok(())
        }
        other => bail!(
            "Unknown SFTP command '{}'. Supported: pwd, ls, cd, get, put, cat, mkdir, rm, mv, help",
            other
        ),
    }
}

fn required_arg<'a>(args: &'a [String], index: usize, usage: &str) -> Result<&'a str> {
    args.get(index)
        .map(|value| value.as_str())
        .ok_or_else(|| anyhow!(usage.to_string()))
}

fn list_dir(session_id: &str, path: &str) -> Result<Vec<SftpEntry>> {
    match ipc_support::send(&IpcMessage::SftpListDir {
        session_id: session_id.to_string(),
        path: path.to_string(),
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

fn print_help() {
    println!("pwd");
    println!("ls [path]");
    println!("cd <path>");
    println!("get <remote> [local]");
    println!("put <local> [remote]");
    println!("cat <path>");
    println!("mkdir <path>");
    println!("rm <path>");
    println!("mv <old> <new>");
    println!("help");
    println!("quit");
}

fn default_download_target(remote: &str) -> String {
    Path::new(remote)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(remote)
        .to_string()
}
