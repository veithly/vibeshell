use anyhow::Result;
use log::{info, warn, error, debug};
use russh::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::ssh::ClientHandler;

/// Stats for dynamic (SOCKS5) forward
pub struct DynamicForwardStats {
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub active_connections: AtomicU32,
}

impl Default for DynamicForwardStats {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicForwardStats {
    pub fn new() -> Self {
        Self {
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            active_connections: AtomicU32::new(0),
        }
    }
}

/// SOCKS5 constants
const SOCKS5_VERSION: u8 = 0x05;
const SOCKS5_AUTH_NONE: u8 = 0x00;
const SOCKS5_CMD_CONNECT: u8 = 0x01;
const SOCKS5_ADDR_IPV4: u8 = 0x01;
const SOCKS5_ADDR_DOMAIN: u8 = 0x03;
const SOCKS5_ADDR_IPV6: u8 = 0x04;
const SOCKS5_REPLY_SUCCESS: u8 = 0x00;
const SOCKS5_REPLY_GENERAL_FAILURE: u8 = 0x01;
const SOCKS5_REPLY_CMD_NOT_SUPPORTED: u8 = 0x07;

/// Run a dynamic (SOCKS5) port forwarding tunnel
pub async fn run_dynamic_forward(
    ssh_handle: Arc<tokio::sync::Mutex<Option<client::Handle<ClientHandler>>>>,
    local_host: String,
    local_port: u16,
    stats: Arc<DynamicForwardStats>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let bind_addr = format!("{}:{}", local_host, local_port);
    let listener = TcpListener::bind(&bind_addr).await?;
    info!("[Tunnel:SOCKS5] Listening on {} (SOCKS5 proxy)", bind_addr);

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((tcp_stream, peer_addr)) => {
                        debug!("[Tunnel:SOCKS5] Accepted connection from {}", peer_addr);
                        stats.active_connections.fetch_add(1, Ordering::Relaxed);

                        let ssh = ssh_handle.clone();
                        let stats_clone = stats.clone();
                        let peer_str = peer_addr.to_string();

                        tokio::spawn(async move {
                            if let Err(e) = handle_socks5_connection(ssh, tcp_stream, &peer_str, &stats_clone).await {
                                debug!("[Tunnel:SOCKS5] Connection error for {}: {}", peer_str, e);
                            }
                            stats_clone.active_connections.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) => {
                        error!("[Tunnel:SOCKS5] Accept error: {}", e);
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("[Tunnel:SOCKS5] Shutdown signal received for {}", bind_addr);
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn handle_socks5_connection(
    ssh_handle: Arc<tokio::sync::Mutex<Option<client::Handle<ClientHandler>>>>,
    mut stream: tokio::net::TcpStream,
    peer_addr: &str,
    stats: &DynamicForwardStats,
) -> Result<()> {
    // === SOCKS5 Greeting Phase ===
    // Read: version, nmethods, methods...
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await?;

    if header[0] != SOCKS5_VERSION {
        return Err(anyhow::anyhow!("Not a SOCKS5 request (version: {})", header[0]));
    }

    let nmethods = header[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).await?;

    // We only support NO AUTH
    if !methods.contains(&SOCKS5_AUTH_NONE) {
        // No acceptable method
        stream.write_all(&[SOCKS5_VERSION, 0xFF]).await?;
        return Err(anyhow::anyhow!("No acceptable auth method"));
    }

    // Send auth response: no auth required
    stream.write_all(&[SOCKS5_VERSION, SOCKS5_AUTH_NONE]).await?;

    // === SOCKS5 Request Phase ===
    // Read: version, cmd, reserved, addr_type
    let mut req_header = [0u8; 4];
    stream.read_exact(&mut req_header).await?;

    if req_header[0] != SOCKS5_VERSION {
        return Err(anyhow::anyhow!("Invalid SOCKS5 request version"));
    }

    let cmd = req_header[1];
    let addr_type = req_header[3];

    if cmd != SOCKS5_CMD_CONNECT {
        // Only CONNECT is supported
        send_socks5_reply(&mut stream, SOCKS5_REPLY_CMD_NOT_SUPPORTED).await?;
        return Err(anyhow::anyhow!("Unsupported SOCKS5 command: {}", cmd));
    }

    // Parse destination address
    let dest_host = match addr_type {
        SOCKS5_ADDR_IPV4 => {
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await?;
            format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
        }
        SOCKS5_ADDR_DOMAIN => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            stream.read_exact(&mut domain).await?;
            String::from_utf8_lossy(&domain).to_string()
        }
        SOCKS5_ADDR_IPV6 => {
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).await?;
            // Format as IPv6
            let parts: Vec<String> = (0..8)
                .map(|i| format!("{:x}", u16::from_be_bytes([addr[i * 2], addr[i * 2 + 1]])))
                .collect();
            parts.join(":")
        }
        _ => {
            send_socks5_reply(&mut stream, SOCKS5_REPLY_GENERAL_FAILURE).await?;
            return Err(anyhow::anyhow!("Unknown address type: {}", addr_type));
        }
    };

    // Read destination port
    let mut port_bytes = [0u8; 2];
    stream.read_exact(&mut port_bytes).await?;
    let dest_port = u16::from_be_bytes(port_bytes);

    debug!("[Tunnel:SOCKS5] CONNECT {}:{} from {}", dest_host, dest_port, peer_addr);

    // Open direct-tcpip channel to the target
    let channel_result = {
        let handle_guard = ssh_handle.lock().await;
        let handle = handle_guard.as_ref()
            .ok_or_else(|| anyhow::anyhow!("SSH session not available"))?;
        handle
            .channel_open_direct_tcpip(&dest_host, dest_port as u32, peer_addr, 0)
            .await
    };

    match channel_result {
        Ok(channel) => {
            // Send success reply
            send_socks5_reply(&mut stream, SOCKS5_REPLY_SUCCESS).await?;

            // Bridge the connection
            bridge_socks5(stream, channel, stats).await?;
        }
        Err(e) => {
            warn!("[Tunnel:SOCKS5] Failed to open channel to {}:{}: {}", dest_host, dest_port, e);
            send_socks5_reply(&mut stream, SOCKS5_REPLY_GENERAL_FAILURE).await?;
        }
    }

    Ok(())
}

async fn send_socks5_reply(
    stream: &mut tokio::net::TcpStream,
    reply: u8,
) -> Result<()> {
    // Send reply: version, reply, reserved, addr_type=IPv4, bind_addr=0.0.0.0, bind_port=0
    let response = [
        SOCKS5_VERSION,
        reply,
        0x00, // reserved
        SOCKS5_ADDR_IPV4,
        0, 0, 0, 0, // bind address
        0, 0, // bind port
    ];
    stream.write_all(&response).await?;
    Ok(())
}

async fn bridge_socks5(
    tcp_stream: tokio::net::TcpStream,
    channel: Channel<client::Msg>,
    stats: &DynamicForwardStats,
) -> Result<()> {
    // Use into_stream() to get a bidirectional AsyncRead + AsyncWrite
    let channel_stream = channel.into_stream();
    let (mut ch_reader, mut ch_writer) = tokio::io::split(channel_stream);

    let (mut tcp_reader, mut tcp_writer) = tokio::io::split(tcp_stream);

    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(2);

    // TCP -> SSH
    let done1 = done_tx.clone();
    let bytes_in = Arc::new(AtomicU64::new(0));
    let bytes_in_c = bytes_in.clone();

    let tcp_to_ssh = tokio::spawn(async move {
        let mut buf = vec![0u8; 32768];
        loop {
            match tcp_reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    bytes_in_c.fetch_add(n as u64, Ordering::Relaxed);
                    if (ch_writer.write_all(&buf[..n]).await).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = ch_writer.shutdown().await;
        let _ = done1.send(()).await;
    });

    // SSH -> TCP
    let done2 = done_tx;
    let bytes_out = Arc::new(AtomicU64::new(0));
    let bytes_out_c = bytes_out.clone();

    let ssh_to_tcp = tokio::spawn(async move {
        let mut buf = vec![0u8; 32768];
        loop {
            match ch_reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    bytes_out_c.fetch_add(n as u64, Ordering::Relaxed);
                    if (tcp_writer.write_all(&buf[..n]).await).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tcp_writer.shutdown().await;
        let _ = done2.send(()).await;
    });

    done_rx.recv().await;

    // Update aggregate stats
    stats.bytes_in.fetch_add(bytes_in.load(Ordering::Relaxed), Ordering::Relaxed);
    stats.bytes_out.fetch_add(bytes_out.load(Ordering::Relaxed), Ordering::Relaxed);

    tcp_to_ssh.abort();
    ssh_to_tcp.abort();

    Ok(())
}
