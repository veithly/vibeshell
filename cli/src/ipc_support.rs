use std::io::BufReader;

use anyhow::{Context, Result};
use interprocess::local_socket::Stream;
use vibeshell_core::ipc::{IpcClient, IpcMessage};

use crate::daemon;

fn connect_error_context(error: anyhow::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "Failed to communicate with the VibeShell background service: {}.\nRun `vshell daemon status` to inspect service availability.",
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
