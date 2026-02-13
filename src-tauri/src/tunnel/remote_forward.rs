use anyhow::Result;
use log::{info, warn, debug};
use russh::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicU32};
use tokio::sync::watch;

use crate::ssh::ClientHandler;

/// Stats tracking for a remote forward tunnel
pub struct RemoteForwardStats {
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub active_connections: AtomicU32,
}

impl Default for RemoteForwardStats {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteForwardStats {
    pub fn new() -> Self {
        Self {
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            active_connections: AtomicU32::new(0),
        }
    }
}

/// Run a remote port forwarding tunnel
/// Requests the SSH server to listen on remote_host:remote_port and forwards to local_host:local_port
pub async fn run_remote_forward(
    ssh_handle: Arc<tokio::sync::Mutex<Option<client::Handle<ClientHandler>>>>,
    local_host: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
    _stats: Arc<RemoteForwardStats>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    info!(
        "[Tunnel:Remote] Requesting remote forward {}:{} -> {}:{}",
        remote_host, remote_port, local_host, local_port
    );

    // Request the server to start listening on the remote side
    // tcpip_forward returns the actual port (useful when remote_port is 0)
    let actual_port = {
        let mut handle_guard = ssh_handle.lock().await;
        let handle = handle_guard.as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH session not available"))?;
        handle
            .tcpip_forward(&remote_host, remote_port as u32)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to request remote forward: {}", e))?
    };

    info!(
        "[Tunnel:Remote] Server listening on {}:{} (requested: {})",
        remote_host, actual_port, remote_port
    );

    // Wait for shutdown signal
    // Note: The actual forwarded connection handling happens through the SSH client handler's
    // `server_channel_open_forwarded_tcpip` callback. For now we keep this tunnel alive
    // and the connection bridging would need to be handled via the ClientHandler.
    //
    // TODO: Full remote forward requires implementing forwarded-tcpip channel handling
    // in the ClientHandler. For now, this sets up the server-side listener.

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("[Tunnel:Remote] Shutdown signal received");
                    break;
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                // Periodic check - tunnel is alive as long as SSH session is alive
                debug!("[Tunnel:Remote] Still active: {}:{}", remote_host, actual_port);
            }
        }
    }

    // Cancel the remote forward
    {
        let handle_guard = ssh_handle.lock().await;
        if let Some(handle) = handle_guard.as_ref() {
            if let Err(e) = handle.cancel_tcpip_forward(&remote_host, actual_port).await {
                warn!("[Tunnel:Remote] Failed to cancel remote forward: {}", e);
            }
        }
    }

    info!("[Tunnel:Remote] Remote forward stopped");
    Ok(())
}
