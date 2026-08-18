use std::io::BufReader;

use anyhow::{bail, Context, Result};
use interprocess::local_socket::Stream;
use vibeshell_core::ipc::{IpcClient, IpcMessage};

use crate::daemon;

fn connect_error_context(error: anyhow::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "Failed to communicate with the VibeShell background service: {}.\nRun `vibeshell daemon status` to inspect service availability.",
        error
    )
}

pub fn ensure_ipc_ready() -> Result<()> {
    daemon::ensure_running()
}

pub fn send(message: &IpcMessage) -> Result<IpcMessage> {
    ensure_ipc_ready()?;
    match IpcClient::send(message) {
        Ok(response) => Ok(response),
        Err(_) => {
            // If the endpoint restarted between readiness check and connect, give the
            // daemon one more chance to come up before surfacing the failure.
            ensure_ipc_ready()?;
            IpcClient::send(message).map_err(connect_error_context)
        }
    }
}

pub fn connect_streaming(message: &IpcMessage) -> Result<BufReader<Stream>> {
    ensure_ipc_ready()?;
    match IpcClient::connect_streaming(message) {
        Ok(stream) => Ok(stream),
        Err(_) => {
            ensure_ipc_ready()?;
            IpcClient::connect_streaming(message).map_err(connect_error_context)
        }
    }
}

#[allow(dead_code)]
pub fn send_without_autostart(message: &IpcMessage) -> Result<IpcMessage> {
    IpcClient::send(message).context("IPC server is unavailable")
}

/// Return the number of clients currently attached to a session.
/// Returns 0 on any IPC error so callers can fall back to the default path.
pub fn session_client_count(session_id: &str) -> usize {
    match send(&IpcMessage::ListSessions) {
        Ok(IpcMessage::SessionList { sessions }) => sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| s.clients)
            .unwrap_or(0),
        _ => 0,
    }
}

/// Execute a command via SSH exec channel (separate from the shared PTY).
///
/// This opens a dedicated SSH channel for the command, so the command text
/// never appears in the interactive terminal that may be displayed by the GUI.
pub fn exec_isolated(session_id: &str, command: &str) -> Result<String> {
    match send(&IpcMessage::ExecCommand {
        session_id: session_id.to_string(),
        command: command.to_string(),
        stdin: None,
    })? {
        IpcMessage::CommandOutput { output } => Ok(output),
        IpcMessage::Error { message } => bail!("Command failed: {}", message),
        _ => bail!("Unexpected response from background service"),
    }
}
