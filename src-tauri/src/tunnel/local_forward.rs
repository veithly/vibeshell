use anyhow::Result;
use log::{info, warn, error, debug};
use russh::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::ssh::ClientHandler;

/// Stats tracking for a local forward tunnel
pub struct LocalForwardStats {
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub active_connections: AtomicU32,
}

impl Default for LocalForwardStats {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalForwardStats {
    pub fn new() -> Self {
        Self {
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            active_connections: AtomicU32::new(0),
        }
    }
}

/// Run a local port forwarding tunnel
/// Listens on local_host:local_port and forwards connections to remote_host:remote_port via SSH
pub async fn run_local_forward(
    ssh_handle: Arc<tokio::sync::Mutex<Option<client::Handle<ClientHandler>>>>,
    local_host: String,
    local_port: u16,
    remote_host: String,
    remote_port: u16,
    stats: Arc<LocalForwardStats>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let bind_addr = format!("{}:{}", local_host, local_port);
    let listener = TcpListener::bind(&bind_addr).await?;
    info!("[Tunnel:Local] Listening on {} -> {}:{}", bind_addr, remote_host, remote_port);

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((tcp_stream, peer_addr)) => {
                        debug!("[Tunnel:Local] Accepted connection from {}", peer_addr);
                        stats.active_connections.fetch_add(1, Ordering::Relaxed);

                        let ssh = ssh_handle.clone();
                        let rh = remote_host.clone();
                        let rp = remote_port;
                        let stats_clone = stats.clone();
                        let peer_str = peer_addr.to_string();

                        tokio::spawn(async move {
                            if let Err(e) = handle_local_forward_connection(
                                ssh, tcp_stream, &rh, rp, &peer_str, &stats_clone,
                            ).await {
                                warn!("[Tunnel:Local] Connection error for {}: {}", peer_str, e);
                            }
                            stats_clone.active_connections.fetch_sub(1, Ordering::Relaxed);
                            debug!("[Tunnel:Local] Connection from {} closed", peer_str);
                        });
                    }
                    Err(e) => {
                        error!("[Tunnel:Local] Accept error: {}", e);
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("[Tunnel:Local] Shutdown signal received for {}", bind_addr);
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn handle_local_forward_connection(
    ssh_handle: Arc<tokio::sync::Mutex<Option<client::Handle<ClientHandler>>>>,
    tcp_stream: tokio::net::TcpStream,
    remote_host: &str,
    remote_port: u16,
    peer_addr: &str,
    stats: &LocalForwardStats,
) -> Result<()> {
    // Open a direct-tcpip channel to the remote host
    let channel = {
        let handle_guard = ssh_handle.lock().await;
        let handle = handle_guard.as_ref()
            .ok_or_else(|| anyhow::anyhow!("SSH session not available"))?;
        handle
            .channel_open_direct_tcpip(remote_host, remote_port as u32, peer_addr, 0)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to open direct-tcpip channel: {}", e))?
    };

    debug!("[Tunnel:Local] Opened direct-tcpip channel to {}:{}", remote_host, remote_port);

    // Use into_stream() to get a bidirectional AsyncRead + AsyncWrite stream
    let channel_stream = channel.into_stream();
    let (mut ch_reader, mut ch_writer) = tokio::io::split(channel_stream);

    let (mut tcp_reader, mut tcp_writer) = tokio::io::split(tcp_stream);

    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(2);

    // TCP -> SSH channel
    let done1 = done_tx.clone();
    let bytes_in_counter = Arc::new(AtomicU64::new(0));
    let bytes_in_c = bytes_in_counter.clone();

    let tcp_to_ssh = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut buf = vec![0u8; 32768];
        loop {
            match tcp_reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    bytes_in_c.fetch_add(n as u64, Ordering::Relaxed);
                    if let Err(e) = ch_writer.write_all(&buf[..n]).await {
                        debug!("[Tunnel:Local] SSH channel write error: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    debug!("[Tunnel:Local] TCP read error: {}", e);
                    break;
                }
            }
        }
        let _ = ch_writer.shutdown().await;
        let _ = done1.send(()).await;
    });

    // SSH channel -> TCP
    let done2 = done_tx;
    let bytes_out_counter = Arc::new(AtomicU64::new(0));
    let bytes_out_c = bytes_out_counter.clone();

    let ssh_to_tcp = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut buf = vec![0u8; 32768];
        loop {
            match ch_reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    bytes_out_c.fetch_add(n as u64, Ordering::Relaxed);
                    if let Err(e) = tcp_writer.write_all(&buf[..n]).await {
                        debug!("[Tunnel:Local] TCP write error: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    debug!("[Tunnel:Local] SSH read error: {}", e);
                    break;
                }
            }
        }
        let _ = tcp_writer.shutdown().await;
        let _ = done2.send(()).await;
    });

    // Wait for either direction to finish
    done_rx.recv().await;

    // Update aggregate stats
    stats.bytes_in.fetch_add(bytes_in_counter.load(Ordering::Relaxed), Ordering::Relaxed);
    stats.bytes_out.fetch_add(bytes_out_counter.load(Ordering::Relaxed), Ordering::Relaxed);

    // Clean up
    tcp_to_ssh.abort();
    ssh_to_tcp.abort();

    Ok(())
}
