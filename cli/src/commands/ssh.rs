//! SSH connection commands for the CLI.
//!
//! These commands allow initiating SSH connections to configured servers,
//! entering an interactive terminal session, or running a single remote command.
//! Interactive sessions are automatically assigned a short 3-digit alias so
//! users can reference them via `vshell ssh-session <alias>` later.

use std::io::Write;
use std::time::Duration;

use anyhow::{bail, Result};
use vibeshell_core::ipc::{IpcMessage, IpcSessionInfo};

use crate::ipc_support;
use crate::session_alias;
use crate::terminal;

/// Connect to a configured SSH server and enter interactive mode.
///
/// Creates a session via IPC, then attaches to it with a streaming
/// terminal connection. Press Ctrl+] to detach (session keeps running).
///
/// A persistent 3-digit alias is assigned automatically so subsequent
/// interactions can use `vshell ssh-session <alias>`.
pub fn connect(server_name: &str, command: &[String], wait: bool, force_new: bool) -> Result<()> {
    let reused = if force_new {
        None
    } else {
        find_reusable_session(server_name)?
    };

    let (session_id, reused_existing) = match reused {
        Some(info) => (info.id, true),
        None => (create_session_with_retry(server_name, wait)?, false),
    };

    let alias = session_alias::find_by_session_id(&session_id)
        .or_else(|| session_alias::register(&session_id, server_name).ok())
        .unwrap_or_else(|| session_id[..8].to_string());

    // Single-command mode: use a dedicated SSH exec channel so the
    // command output is clean (no MOTD banner), and the command never
    // gets typed into the shared PTY (avoids garbled input when the
    // GUI terminal is also attached).
    if !command.is_empty() {
        let joined = command.join(" ");
        if !reused_existing {
            return run_isolated_command(&session_id, &alias, &joined, true);
        }

        match run_isolated_command(&session_id, &alias, &joined, false) {
            Ok(()) => return Ok(()),
            Err(error) => {
                eprintln!(
                    "Reused session {} (alias: {}) could not execute the command: {}",
                    session_id, alias, error
                );
                eprintln!("Creating a fresh session and retrying...");

                if ipc_support::session_client_count(&session_id) == 0 {
                    let _ = ipc_support::send(&IpcMessage::KillSession {
                        session_id: session_id.clone(),
                    });
                    let _ = session_alias::remove_by_session_id(&session_id);
                }

                let fresh_session_id = create_session_with_retry(server_name, wait)?;
                let fresh_alias = session_alias::register(&fresh_session_id, server_name)
                    .unwrap_or_else(|_| fresh_session_id[..8].to_string());

                return run_isolated_command(&fresh_session_id, &fresh_alias, &joined, true);
            }
        }
    }

    if reused_existing {
        eprintln!(
            "Reusing server '{}' session {} (alias: {})",
            server_name, session_id, alias
        );
    } else {
        eprintln!(
            "Connected to server '{}' (session: {}, alias: {})",
            server_name, session_id, alias
        );
    }
    eprintln!("Press Ctrl+] to detach. Session continues in the background.\r");

    let result = terminal::run_interactive(&session_id);

    eprintln!();
    eprintln!(
        "Session {} (alias: {}) is still running.",
        session_id, alias
    );
    eprintln!("Next use: vshell ssh-session {} -- <command>", alias);
    eprintln!();
    eprintln!("  Next time, interact with this session using:");
    eprintln!(
        "    vshell ssh-session {}              # Reattach interactively",
        alias
    );
    eprintln!(
        "    vshell ssh-session {} -- <command>  # Execute a single command",
        alias
    );
    eprintln!(
        "    vshell ssh-session {} --command-file ./remote-command.sh",
        alias
    );
    eprintln!(
        "    vshell sftp --session {}            # Open SFTP file browser",
        session_id
    );
    eprintln!(
        "    vshell kill {}                      # Terminate the session",
        alias
    );

    result
}

/// Create a session with optional retry logic for flaky connections (e.g., Tailscale/VPN).
fn create_session_with_retry(server_name: &str, wait: bool) -> Result<String> {
    let max_attempts = if wait { 30 } else { 1 };
    let mut last_error = String::new();

    for attempt in 1..=max_attempts {
        if attempt > 1 {
            let delay = std::cmp::min(2u64.pow((attempt - 1).min(4) as u32), 16);
            eprintln!(
                "Connection failed, retrying in {}s ({}/{})...",
                delay, attempt, max_attempts
            );
            std::thread::sleep(Duration::from_secs(delay));
        }

        match ipc_support::send(&IpcMessage::CreateSession {
            server_name: server_name.to_string(),
        }) {
            Ok(IpcMessage::SessionCreated { session_id }) => return Ok(session_id),
            Ok(IpcMessage::Error { message }) => {
                last_error = message;
                if !wait {
                    bail!(
                        "Error connecting to server '{}': {}",
                        server_name,
                        last_error
                    );
                }
                eprintln!("  Error: {}", last_error);
            }
            Ok(_) => {
                bail!("Unexpected response from background service");
            }
            Err(e) => {
                last_error = e.to_string();
                if !wait {
                    return Err(e);
                }
                eprintln!("  Error: {}", last_error);
            }
        }
    }

    bail!(
        "Failed to connect to '{}' after {} attempts: {}",
        server_name,
        max_attempts,
        last_error
    )
}

fn find_reusable_session(server_name: &str) -> Result<Option<IpcSessionInfo>> {
    let response = ipc_support::send(&IpcMessage::ListSessions)?;
    match response {
        IpcMessage::SessionList { sessions } => Ok(pick_reusable_session(&sessions, server_name)),
        IpcMessage::Error { message } => bail!("Error listing sessions: {}", message),
        _ => bail!("Unexpected response from background service"),
    }
}

fn pick_reusable_session(sessions: &[IpcSessionInfo], server_name: &str) -> Option<IpcSessionInfo> {
    sessions
        .iter()
        .filter(|info| info.server_name == server_name)
        .filter(|info| info.state == "connected")
        .min_by_key(|info| info.created_at)
        .cloned()
}

/// Execute a command via a dedicated SSH exec channel.
///
/// The exec channel bypasses the interactive PTY entirely, which:
/// 1. Suppresses the remote server's MOTD / welcome banner.
/// 2. Prevents garbled input when the GUI terminal shares the same session.
fn run_isolated_command(
    session_id: &str,
    alias: &str,
    command: &str,
    owns_session: bool,
) -> Result<()> {
    let output = ipc_support::exec_isolated(session_id, command)?;
    print!("{}", output);
    let _ = std::io::stdout().flush();

    if output_looks_interactive(&output) {
        eprintln!();
        eprintln!("Interactive input may be required. Send a response:");
        eprintln!(
            "  vshell send-key {} y enter   # send 'y' then Enter",
            alias
        );
        eprintln!("  vshell send-key {} enter      # press Enter", alias);
        eprintln!(
            "  vshell ssh-session {}          # attach interactively",
            alias
        );
        return Ok(());
    }

    if owns_session {
        let _ = ipc_support::send(&IpcMessage::KillSession {
            session_id: session_id.to_string(),
        });
        let _ = session_alias::remove_by_session_id(session_id);
    } else {
        eprintln!("Next use: vshell ssh-session {} -- <command>", alias);
        eprintln!(
            "If your local shell mangles quotes, use: vshell ssh-session {} --command-file ./remote-command.sh",
            alias
        );
    }

    Ok(())
}

fn output_looks_interactive(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    [
        "[y/n]",
        "[yes/no]",
        "(y/n)",
        "(yes/no)",
        "press enter",
        "continue?",
        "are you sure",
        "password:",
        "passphrase:",
        "confirmation",
        "confirm ",
        "do you want",
        "[y] yes",
        "[n] no",
    ]
    .iter()
    .any(|marker| lower.contains(&marker.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::pick_reusable_session;
    use vibeshell_core::ipc::IpcSessionInfo;

    fn session(id: &str, server_name: &str, state: &str, created_at: i64) -> IpcSessionInfo {
        IpcSessionInfo {
            id: id.to_string(),
            server_id: format!("server-{server_name}"),
            server_name: server_name.to_string(),
            state: state.to_string(),
            created_at,
            clients: 0,
        }
    }

    #[test]
    fn pick_reusable_session_prefers_oldest_active_session_for_same_server() {
        let sessions = vec![
            session("newer", "prod", "connected", 200),
            session("other", "staging", "connected", 100),
            session("oldest", "prod", "connected", 50),
        ];

        let picked = pick_reusable_session(&sessions, "prod").expect("should reuse prod session");
        assert_eq!(picked.id, "oldest");
    }

    #[test]
    fn pick_reusable_session_ignores_disconnected_sessions() {
        let sessions = vec![
            session("pending", "prod", "connecting", 0),
            session("dead", "prod", "disconnected", 1),
            session("errored", "prod", "error", 2),
        ];

        assert!(
            pick_reusable_session(&sessions, "prod").is_none(),
            "inactive sessions must not be reused"
        );
    }

    #[test]
    fn output_looks_interactive_detects_common_prompts() {
        use super::output_looks_interactive;
        assert!(output_looks_interactive("Do you want to continue? [Y/n] "));
        assert!(output_looks_interactive(
            "Are you sure you want to proceed?"
        ));
        assert!(output_looks_interactive("Enter password: "));
        assert!(output_looks_interactive("Press ENTER to continue"));
        assert!(!output_looks_interactive(
            "total 42\ndrwxr-xr-x 2 root root"
        ));
        assert!(!output_looks_interactive("Linux hostname 5.15.0"));
    }
}
