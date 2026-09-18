use super::bridge::{bridge, ActiveConnection};
use crate::ssh::ClientHandler;
use anyhow::{bail, ensure, Context, Result};
use log::debug;
use russh::{client, Channel};
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, watch};
use tokio::task::JoinSet;

#[derive(Default)]
pub struct DynamicForwardStats {
    pub bytes_in: AtomicU64,
    pub bytes_out: AtomicU64,
    pub active_connections: AtomicU32,
}
impl DynamicForwardStats {
    pub fn new() -> Self {
        Self::default()
    }
}

pub async fn run_dynamic_forward(
    ssh_handle: Arc<tokio::sync::Mutex<Option<client::Handle<ClientHandler>>>>,
    local_host: String,
    local_port: u16,
    stats: Arc<DynamicForwardStats>,
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
    let _ = ready.send(Ok(listener.local_addr()?.port()));
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() { break; }
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
            result = listener.accept(), if connections.len() < 256 => {
                let (mut stream, peer) = result?;
                let ssh = ssh_handle.clone();
                let stats = stats.clone();
                connections.spawn(async move {
                    let _active = ActiveConnection::new(&stats.active_connections);
                    let result = async {
                        // Bound negotiation, not the lifetime of an established connection.
                        let channel = tokio::time::timeout(Duration::from_secs(15), async {
                            let (host, port) = read_target(&mut stream).await?;
                            let guard = ssh.lock().await;
                            let handle = guard.as_ref().ok_or_else(|| anyhow::anyhow!("SSH session unavailable"))?;
                            let channel = handle.channel_open_direct_tcpip(&host, port as u32,
                                peer.ip().to_string(), peer.port() as u32).await?;
                            Ok::<Channel<client::Msg>, anyhow::Error>(channel)
                        }).await.context("SOCKS5 negotiation timed out")?;
                        let channel = match channel {
                            Ok(channel) => channel,
                            Err(error) => { let _ = reply(&mut stream, 1).await; return Err(error); }
                        };
                        reply(&mut stream, 0).await?;
                        bridge(stream, channel.into_stream(), &stats.bytes_in, &stats.bytes_out).await?;
                        Ok::<(), anyhow::Error>(())
                    }.await;
                    if let Err(error) = result { debug!("[Tunnel:SOCKS5] Connection failed: {error}"); }
                });
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(())
}

async fn read_target(stream: &mut TcpStream) -> Result<(String, u16)> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await?;
    ensure!(header[0] == 5, "Not a SOCKS5 request");
    let mut methods = vec![0; header[1] as usize];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&0) {
        stream.write_all(&[5, 255]).await?;
        bail!("No acceptable SOCKS5 authentication method");
    }
    stream.write_all(&[5, 0]).await?;
    let mut request = [0u8; 4];
    stream.read_exact(&mut request).await?;
    ensure!(
        request[0] == 5 && request[2] == 0,
        "Invalid SOCKS5 request header"
    );
    if request[1] != 1 {
        reply(stream, 7).await?;
        bail!("Only SOCKS5 CONNECT is supported");
    }
    let host = match request[3] {
        1 => {
            let mut bytes = [0; 4];
            stream.read_exact(&mut bytes).await?;
            std::net::Ipv4Addr::from(bytes).to_string()
        }
        3 => {
            let length = stream.read_u8().await? as usize;
            ensure!(length > 0, "Empty SOCKS5 hostname");
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).await?;
            let host = String::from_utf8(bytes).context("Invalid SOCKS5 hostname encoding")?;
            ensure!(!host.contains('\0'), "Invalid SOCKS5 hostname");
            host
        }
        4 => {
            let mut bytes = [0; 16];
            stream.read_exact(&mut bytes).await?;
            std::net::Ipv6Addr::from(bytes).to_string()
        }
        _ => {
            reply(stream, 8).await?;
            bail!("Unsupported SOCKS5 address type");
        }
    };
    let port = stream.read_u16().await?;
    ensure!(port != 0, "SOCKS5 destination port must be nonzero");
    Ok((host, port))
}

async fn reply(stream: &mut TcpStream, status: u8) -> Result<()> {
    stream
        .write_all(&[5, status, 0, 1, 0, 0, 0, 0, 0, 0])
        .await?;
    Ok(())
}
