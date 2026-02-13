//! Session management commands for the CLI.
//!
//! These commands allow listing, attaching to, and killing sessions
//! by communicating with the VibeShell GUI via IPC.

use anyhow::{bail, Context, Result};
use vibeshell_core::ipc::{IpcClient, IpcMessage};

/// List all active sessions.
///
/// Sends a ListSessions request to the GUI and displays the results.
pub fn list() -> Result<()> {
    let response = IpcClient::send(&IpcMessage::ListSessions)
        .context("Failed to communicate with VibeShell GUI")?;

    match response {
        IpcMessage::SessionList { sessions } => {
            if sessions.is_empty() {
                println!("No active sessions.");
            } else {
                println!("Active sessions:");
                for session_id in sessions {
                    println!("  {}", session_id);
                }
            }
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error listing sessions: {}", message);
        }
        _ => {
            bail!("Unexpected response from GUI");
        }
    }
}

/// Attach to an existing session.
///
/// Sends an AttachSession request to the GUI.
pub fn attach(session_id: &str) -> Result<()> {
    let response = IpcClient::send(&IpcMessage::AttachSession {
        session_id: session_id.to_string(),
    })
    .context("Failed to communicate with VibeShell GUI")?;

    match response {
        IpcMessage::Ok => {
            println!("Attached to session: {}", session_id);
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error attaching to session: {}", message);
        }
        _ => {
            bail!("Unexpected response from GUI");
        }
    }
}

/// Kill a specific session.
///
/// Sends a KillSession request to the GUI.
pub fn kill(session_id: &str) -> Result<()> {
    let response = IpcClient::send(&IpcMessage::KillSession {
        session_id: session_id.to_string(),
    })
    .context("Failed to communicate with VibeShell GUI")?;

    match response {
        IpcMessage::Ok => {
            println!("Session killed: {}", session_id);
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error killing session: {}", message);
        }
        _ => {
            bail!("Unexpected response from GUI");
        }
    }
}

/// Kill all active sessions.
///
/// Lists all sessions and kills each one.
pub fn kill_all() -> Result<()> {
    // First, get the list of sessions
    let response = IpcClient::send(&IpcMessage::ListSessions)
        .context("Failed to communicate with VibeShell GUI")?;

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

    for session_id in &sessions {
        let kill_response = IpcClient::send(&IpcMessage::KillSession {
            session_id: session_id.clone(),
        });

        match kill_response {
            Ok(IpcMessage::Ok) => {
                killed += 1;
            }
            Ok(IpcMessage::Error { message }) => {
                errors.push(format!("{}: {}", session_id, message));
            }
            Ok(_) => {
                errors.push(format!("{}: unexpected response", session_id));
            }
            Err(e) => {
                errors.push(format!("{}: {}", session_id, e));
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
