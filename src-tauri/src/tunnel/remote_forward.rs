use super::bridge::{bridge, ActiveConnection};
use crate::ssh::ClientHandler;
use anyhow::{Context, Result};
use log::debug;
use russh::{client, Channel};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;

/// Only explicitly registered forwards may receive server-initiated channels.
pub(crate) type RemoteForwardRegistry =
    Arc<Mutex<HashMap<(String, u32), mpsc::Sender<Channel<client::Msg>>>>>;

#[derive(Default)]
pub struct RemoteForwardStats {
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub active_connections: AtomicU32,
}
impl RemoteForwardStats {
    pub fn new() -> Self {
        Self::default()
    }
}

struct Registration(RemoteForwardRegistry, (String, u32));
impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut routes) = self.0.lock() {
            routes.remove(&self.1);
        }
    }
}

pub(crate) async fn run_remote_forward(
    ssh_handle: Arc<tokio::sync::Mutex<Option<client::Handle<ClientHandler>>>>,
    (local_host, local_port): (String, u16),
    (remote_host, remote_port): (String, u16),
    stats: Arc<RemoteForwardStats>,
    mut shutdown_rx: watch::Receiver<bool>,
    registry: RemoteForwardRegistry,
    ready: oneshot::Sender<Result<u16, String>>,
) -> Result<()> {
    let opened = tokio::time::timeout(Duration::from_secs(15), async {
        let mut guard = ssh_handle.lock().await;
        let handle = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("SSH session unavailable"))?;
        handle
            .tcpip_forward(&remote_host, remote_port as u32)
            .await
            .map_err(anyhow::Error::from)
    })
    .await
    .context("Remote forward request timed out")?;
    let actual_port = match opened {
        Ok(port) => {
            if remote_port == 0 {
                u16::try_from(port)?
            } else {
                remote_port
            }
        }
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return Err(error);
        }
    };
    let (sender, mut incoming) = mpsc::channel(64);
    let key = (remote_host.clone(), actual_port as u32);
    registry
        .lock()
        .map_err(|_| anyhow::anyhow!("Remote forward registry unavailable"))?
        .insert(key.clone(), sender);
    let registration = Registration(registry, key);
    let _ = ready.send(Ok(actual_port));
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() { break; }
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
            channel = incoming.recv(), if connections.len() < 256 => {
                let Some(channel) = channel else { break };
                let host = local_host.clone();
                let stats = stats.clone();
                connections.spawn(async move {
                    let _active = ActiveConnection::new(&stats.active_connections);
                    let result = async {
                        let tcp = tokio::time::timeout(Duration::from_secs(10),
                            tokio::net::TcpStream::connect((host.as_str(), local_port)))
                            .await.context("Local forward destination timed out")??;
                        bridge(channel.into_stream(), tcp, &stats.bytes_in, &stats.bytes_out).await?;
                        Ok::<(), anyhow::Error>(())
                    }.await;
                    if let Err(error) = result { debug!("[Tunnel:Remote] Connection failed: {error}"); }
                });
            }
        }
    }
    drop(registration);
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    tokio::time::timeout(Duration::from_secs(3), async {
        let guard = ssh_handle.lock().await;
        if let Some(handle) = guard.as_ref() {
            handle
                .cancel_tcpip_forward(&remote_host, actual_port as u32)
                .await?;
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
    .context("Remote forward cancellation timed out")??;
    Ok(())
}
