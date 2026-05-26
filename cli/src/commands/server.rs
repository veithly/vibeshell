//! Server management commands for the CLI.
//!
//! These commands allow listing configured servers by communicating
//! with the VibeShell GUI over IPC.

use anyhow::{bail, Result};
use vibeshell_core::ipc::IpcMessage;

use crate::ipc_support;

/// List all configured servers known to VibeShell.
pub fn list() -> Result<()> {
    let response = ipc_support::send(&IpcMessage::ListServers)?;

    match response {
        IpcMessage::ServerList { servers } => {
            if servers.is_empty() {
                println!("No configured servers.");
                return Ok(());
            }

            println!("Configured servers:");
            for server in servers {
                println!(
                    "  {}  {}@{}:{}  auth={}",
                    server.name, server.username, server.host, server.port, server.auth_type
                );
            }
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error listing servers: {}", message);
        }
        _ => {
            bail!("Unexpected response from background service");
        }
    }
}
