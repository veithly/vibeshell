//! SSH connection commands for the CLI.
//!
//! These commands allow initiating SSH connections to configured servers.

use anyhow::{bail, Context, Result};
use vibeshell_core::ipc::{IpcClient, IpcMessage};

/// Connect to a configured SSH server.
///
/// Sends a CreateSession request to the GUI with the specified server name.
/// The server must be previously configured in the VibeShell GUI.
pub fn connect(server_name: &str) -> Result<()> {
    let response = IpcClient::send(&IpcMessage::CreateSession {
        server_name: server_name.to_string(),
    })
    .context("Failed to communicate with VibeShell GUI")?;

    match response {
        IpcMessage::SessionCreated { session_id } => {
            println!("Created session: {}", session_id);
            println!("Connected to server: {}", server_name);
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error connecting to server '{}': {}", server_name, message);
        }
        _ => {
            bail!("Unexpected response from GUI");
        }
    }
}
