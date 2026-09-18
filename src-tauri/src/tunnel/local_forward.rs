use super::bridge::{bridge, ActiveConnection};
use crate::ssh::ClientHandler;
use anyhow::{Context, Result};
use log::{info, warn};
use russh::client;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, watch};
use tokio::task::JoinSet;

#[derive(Default)]
pub struct LocalForwardStats {
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub active_connections: AtomicU32,
}
impl LocalForwardStats {
    pub fn new() -> Self {
        Self::default()
    }
}

pub async fn run_local_forward(
    ssh_handle: Arc<tokio::sync::Mutex<Option<client::Handle<ClientHandler>>>>,
    (local_host, local_port): (String, u16),
    (remote_host, remote_port): (String, u16),
    stats: Arc<LocalForwardStats>,
    mut shutdown_rx: watch::Receiver<bool>,
    ready: oneshot::Sender<Result<u16, String>>,
) -> Result<()> {
    let listener = match TcpListener::bind((local_host.as_str(), local_port)).await {
        Ok(listener) => listener,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return Err(error.into());
        }
    };
    let address = listener.local_addr()?;
    info!(
        "[Tunnel:Local] Listening on {} -> {}:{}",
        address, remote_host, remote_port
    );
    let _ = ready.send(Ok(address.port()));
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() { break; }
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
            result = listener.accept(), if connections.len() < 256 => {
                let (tcp, peer) = result?;
                let ssh = ssh_handle.clone();
                let target = remote_host.clone();
                let stats = stats.clone();
                connections.spawn(async move {
                    let _active = ActiveConnection::new(&stats.active_connections);
                    let result = async {
                        let channel = tokio::time::timeout(Duration::from_secs(15), async {
                            let guard = ssh.lock().await;
                            guard.as_ref().ok_or_else(|| anyhow::anyhow!("SSH session unavailable"))?
                                .channel_open_direct_tcpip(&target, remote_port as u32, peer.ip().to_string(), peer.port() as u32)
                                .await.map_err(anyhow::Error::from)
                        }).await.context("SSH forwarding channel timed out")??;
                        bridge(tcp, channel.into_stream(), &stats.bytes_in, &stats.bytes_out).await?;
                        Ok::<(), anyhow::Error>(())
                    }.await;
                    if let Err(error) = result { warn!("[Tunnel:Local] Connection failed: {error}"); }
                });
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(())
}
