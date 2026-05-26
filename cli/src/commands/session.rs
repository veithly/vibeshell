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

fn parse_key_tokens(tokens: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for token in tokens {
        match token.to_ascii_lowercase().as_str() {
            "enter" | "return" | "cr" => bytes.push(b'\r'),
            "space" => bytes.push(b' '),
            "tab" => bytes.push(b'\t'),
            "esc" | "escape" => bytes.push(0x1b),
            "backspace" | "bs" => bytes.push(0x7f),
            "delete" | "del" => bytes.extend_from_slice(b"\x1b[3~"),
            "up" => bytes.extend_from_slice(b"\x1b[A"),
            "down" => bytes.extend_from_slice(b"\x1b[B"),
            "right" => bytes.extend_from_slice(b"\x1b[C"),
            "left" => bytes.extend_from_slice(b"\x1b[D"),
            "ctrl-c" => bytes.push(0x03),
            "ctrl-d" => bytes.push(0x04),
            "ctrl-z" => bytes.push(0x1a),
            _ => {
                for ch in token.chars() {
                    let mut buf = [0u8; 4];
                    let s = ch.encode_utf8(&mut buf);
                    bytes.extend_from_slice(s.as_bytes());
                }
            }
        }
    }
    bytes
}

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
///
/// When other clients (e.g. GUI) are attached, uses a dedicated SSH exec
/// channel to avoid injecting keystrokes into the shared PTY. Otherwise
/// uses the PTY approach which supports interactive prompts.
pub fn exec(session_id: &str, command: &[String]) -> Result<()> {
    let joined = command.join(" ");

    if ipc_support::session_client_count(session_id) > 0 {
        let output = ipc_support::exec_isolated(session_id, &joined)?;
        print!("{}", output);
        let _ = std::io::Write::flush(&mut std::io::stdout());
        return Ok(());
    }

    match terminal::run_command_with_handoff(session_id, &joined)? {
        CommandHandoff::Completed | CommandHandoff::SessionEnded => Ok(()),
        CommandHandoff::AwaitingInput => {
            eprintln!();
            eprintln!("Session is waiting for input. Send a response:");
            eprintln!(
                "  vshell send-key {} y enter   # send 'y' then Enter",
                session_id
            );
            eprintln!("  vshell send-key {} enter      # press Enter", session_id);
            eprintln!(
                "  vshell attach {}              # attach interactively",
                session_id
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
        return terminal::run_interactive(&session_id);
    }

    let joined = command.join(" ");

    if ipc_support::session_client_count(&session_id) > 0 {
        let output = ipc_support::exec_isolated(&session_id, &joined)?;
        print!("{}", output);
        let _ = std::io::Write::flush(&mut std::io::stdout());
        return Ok(());
    }

    match terminal::run_command_with_handoff(&session_id, &joined)? {
        CommandHandoff::Completed | CommandHandoff::SessionEnded => Ok(()),
        CommandHandoff::AwaitingInput => {
            eprintln!();
            eprintln!("Session is waiting for input. Send a response:");
            eprintln!(
                "  vshell send-key {} y enter   # send 'y' then Enter",
                alias
            );
            eprintln!("  vshell send-key {} enter      # press Enter", alias);
            eprintln!(
                "  vshell ssh-session {}          # attach interactively",
                alias
            );
            Ok(())
        }
    }
}

/// Send raw keystrokes to a session's PTY.
///
/// Tokens are parsed as named keys (`enter`, `space`, `tab`, `ctrl-c`, ...)
/// or as literal text. Multiple tokens are concatenated in order.
///
/// Examples:
///   vshell send-key 001 y enter          # sends "y\r"
///   vshell send-key 001 yes enter        # sends "yes\r"
///   vshell send-key 001 ctrl-c           # sends Ctrl+C
///   vshell send-key 001 space            # sends a space character
pub fn send_key(session_id: &str, tokens: &[String]) -> Result<()> {
    if tokens.is_empty() {
        bail!("No keys specified. Usage: vshell send-key <alias> <key> [key...]");
    }

    let data = parse_key_tokens(tokens);
    if data.is_empty() {
        bail!("Could not parse any key input from the provided tokens");
    }

    ipc_support::send(&IpcMessage::SendInput {
        session_id: session_id.to_string(),
        data,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_key_tokens;

    fn tokens(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn named_keys_produce_expected_bytes() {
        assert_eq!(parse_key_tokens(&tokens(&["enter"])), vec![b'\r']);
        assert_eq!(parse_key_tokens(&tokens(&["space"])), vec![b' ']);
        assert_eq!(parse_key_tokens(&tokens(&["tab"])), vec![b'\t']);
        assert_eq!(parse_key_tokens(&tokens(&["ctrl-c"])), vec![0x03]);
        assert_eq!(parse_key_tokens(&tokens(&["ctrl-d"])), vec![0x04]);
        assert_eq!(parse_key_tokens(&tokens(&["backspace"])), vec![0x7f]);
    }

    #[test]
    fn literal_text_passed_through() {
        assert_eq!(parse_key_tokens(&tokens(&["yes"])), b"yes".to_vec());
        assert_eq!(parse_key_tokens(&tokens(&["y"])), b"y".to_vec());
    }

    #[test]
    fn mixed_tokens_concatenated() {
        assert_eq!(
            parse_key_tokens(&tokens(&["y", "enter"])),
            vec![b'y', b'\r']
        );
        assert_eq!(
            parse_key_tokens(&tokens(&["yes", "enter"])),
            vec![b'y', b'e', b's', b'\r']
        );
    }

    #[test]
    fn case_insensitive_named_keys() {
        assert_eq!(parse_key_tokens(&tokens(&["ENTER"])), vec![b'\r']);
        assert_eq!(parse_key_tokens(&tokens(&["Ctrl-C"])), vec![0x03]);
        assert_eq!(parse_key_tokens(&tokens(&["Space"])), vec![b' ']);
    }
}
