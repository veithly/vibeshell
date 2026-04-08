//! Session management commands for the CLI.
//!
//! These commands allow listing, attaching to, executing commands on, and
//! killing sessions by communicating with the VibeShell background service
//! via IPC. Sessions can be referenced by their full UUID or by a short
//! 3-digit alias assigned during `vshell ssh`.

use anyhow::{bail, Result};
use vibeshell_core::ipc::IpcMessage;

use crate::ipc_support;
use crate::session_alias;
use crate::terminal::{self, CommandHandoff};

/// Send an IPC message with a human-friendly error wrapper.
fn ipc_send(message: &IpcMessage) -> Result<IpcMessage> {
    ipc_support::send(message)
}

/// List all active sessions, annotated with their short aliases.
pub fn list() -> Result<()> {
    let response = ipc_send(&IpcMessage::ListSessions)?;

    match response {
        IpcMessage::SessionList { sessions } => {
            if sessions.is_empty() {
                println!("No active sessions.");
            } else {
                println!("Active sessions:");
                for info in &sessions {
                    let alias_tag = session_alias::find_by_session_id(&info.id)
                        .map(|a| format!("[{}] ", a))
                        .unwrap_or_else(|| "      ".to_string());
                    println!(
                        "  {}{}  {}  [{}]",
                        alias_tag, info.id, info.server_name, info.state
                    );
                }
            }
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error listing sessions: {}", message);
        }
        _ => {
            bail!("Unexpected response from background service");
        }
    }
}

/// Attach to an existing session and enter interactive terminal mode.
///
/// Streams session output to stdout and forwards stdin to the remote session.
/// Press Ctrl+] to detach.
pub fn attach(session_id: &str) -> Result<()> {
    eprintln!("Attaching to session {}...", session_id);
    eprintln!("Press Ctrl+] to detach.\r");

    terminal::run_interactive(session_id)
}

/// Kill a specific session and clean up its alias.
pub fn kill(session_id: &str) -> Result<()> {
    let response = ipc_send(&IpcMessage::KillSession {
        session_id: session_id.to_string(),
    })?;

    match response {
        IpcMessage::Ok => {
            let _ = session_alias::remove_by_session_id(session_id);
            println!("Session killed: {}", session_id);
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error killing session: {}", message);
        }
        _ => {
            bail!("Unexpected response from background service");
        }
    }
}

/// Kill all active sessions.
///
/// Lists all sessions and kills each one.
pub fn kill_all() -> Result<()> {
    // First, get the list of sessions
    let response = ipc_send(&IpcMessage::ListSessions)?;

    let sessions = match response {
        IpcMessage::SessionList { sessions } => sessions,
        IpcMessage::Error { message } => {
            bail!("Error listing sessions: {}", message);
        }
        _ => {
            bail!("Unexpected response from GUI");
        }
    };

    if sessions.is_empty() {
        println!("No active sessions to kill.");
        return Ok(());
    }

    let mut errors = Vec::new();
    let mut killed = 0;

    for info in &sessions {
        let kill_response = ipc_support::send(&IpcMessage::KillSession {
            session_id: info.id.clone(),
        });

        match kill_response {
            Ok(IpcMessage::Ok) => {
                killed += 1;
            }
            Ok(IpcMessage::Error { message }) => {
                errors.push(format!("{}: {}", info.id, message));
            }
            Ok(_) => {
                errors.push(format!("{}: unexpected response", info.id));
            }
            Err(e) => {
                errors.push(format!("{}: {}", info.id, e));
            }
        }
    }

    println!("Killed {} of {} sessions.", killed, sessions.len());

    if !errors.is_empty() {
        println!("\nErrors:");
        for err in errors {
            println!("  {}", err);
        }
    }

    Ok(())
}

/// Execute a command on an existing session.
pub fn exec(session_id: &str, command: &[String]) -> Result<()> {
    let joined = command.join(" ");
    match terminal::run_command_with_handoff(session_id, &joined)? {
        CommandHandoff::Completed | CommandHandoff::SessionEnded => Ok(()),
        CommandHandoff::AwaitingInput => {
            eprintln!();
            eprintln!("Next use: vshell exec {} -- <command>", session_id);
            eprintln!(
                "Session '{}' is waiting for more input. Reattach with 'vshell attach {}'.",
                session_id, session_id
            );
            Ok(())
        }
    }
}

/// Interact with a session via its short alias ID.
///
/// If no command is provided, attaches interactively (like `vshell attach`).
/// If a command is provided, executes it and prints the output (like `vshell exec`).
pub fn ssh_session(alias: &str, command: &[String]) -> Result<()> {
    let session_id = session_alias::resolve(alias).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown session alias '{}'. Run 'vshell sessions' to see active sessions.",
            alias
        )
    })?;

    let entry = session_alias::get_entry(alias);
    let display_name = entry
        .as_ref()
        .map(|e| e.server_name.as_str())
        .unwrap_or("unknown");

    if command.is_empty() {
        eprintln!(
            "Reattaching to session {} ({}, alias: {})...",
            session_id, display_name, alias
        );
        eprintln!("Press Ctrl+] to detach.\r");
        terminal::run_interactive(&session_id)
    } else {
        let joined = command.join(" ");
        match terminal::run_command_with_handoff(&session_id, &joined)? {
            CommandHandoff::Completed | CommandHandoff::SessionEnded => Ok(()),
            CommandHandoff::AwaitingInput => {
                eprintln!();
                eprintln!("Next use: vshell ssh-session {} -- <command>", alias);
                eprintln!(
                    "Session '{}' (alias {}) is waiting for more input. Reattach with 'vshell ssh-session {}'.",
                    display_name, alias, alias
                );
                Ok(())
            }
        }
    }
}
