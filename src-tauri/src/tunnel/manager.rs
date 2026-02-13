use anyhow::Result;
use log::{info, error};
use russh::client;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::{RwLock, watch};
use uuid::Uuid;

use crate::ssh::ClientHandler;
use crate::storage::models::{TunnelConfig, TunnelType, TunnelInfo, TunnelStatus};
use crate::tunnel::local_forward::{self, LocalForwardStats};
use crate::tunnel::remote_forward::{self, RemoteForwardStats};
use crate::tunnel::dynamic_forward::{self, DynamicForwardStats};

/// Generic stats wrapper
enum TunnelStatsInner {
    Local(Arc<LocalForwardStats>),
    Remote(Arc<RemoteForwardStats>),
    Dynamic(Arc<DynamicForwardStats>),
}

impl TunnelStatsInner {
    fn bytes_in(&self) -> u64 {
        match self {
            Self::Local(s) => s.bytes_in.load(Ordering::Relaxed),
            Self::Remote(s) => s.bytes_in.load(Ordering::Relaxed),
            Self::Dynamic(s) => s.bytes_in.load(Ordering::Relaxed),
        }
    }

    fn bytes_out(&self) -> u64 {
        match self {
            Self::Local(s) => s.bytes_out.load(Ordering::Relaxed),
            Self::Remote(s) => s.bytes_out.load(Ordering::Relaxed),
            Self::Dynamic(s) => s.bytes_out.load(Ordering::Relaxed),
        }
    }

    fn active_connections(&self) -> u32 {
        match self {
            Self::Local(s) => s.active_connections.load(Ordering::Relaxed),
            Self::Remote(s) => s.active_connections.load(Ordering::Relaxed),
            Self::Dynamic(s) => s.active_connections.load(Ordering::Relaxed),
        }
    }
}

struct TunnelHandle {
    id: String,
    config: TunnelConfig,
    session_id: String,
    status: Arc<RwLock<TunnelStatus>>,
    stats: TunnelStatsInner,
    shutdown_tx: watch::Sender<bool>,
    task_handle: tokio::task::JoinHandle<()>,
}

pub struct TunnelManager {
    tunnels: Arc<RwLock<HashMap<String, TunnelHandle>>>,
}

impl Default for TunnelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TunnelManager {
    pub fn new() -> Self {
        Self {
            tunnels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create and start a tunnel
    pub async fn create_tunnel(
        &self,
        session_id: &str,
        ssh_handle: Arc<tokio::sync::Mutex<Option<client::Handle<ClientHandler>>>>,
        config: TunnelConfig,
    ) -> Result<TunnelInfo> {
        let tunnel_id = if config.id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            config.id.clone()
        };

        let status = Arc::new(RwLock::new(TunnelStatus::Starting));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let status_clone = status.clone();
        let tid = tunnel_id.clone();

        let (task_handle, stats) = match config.tunnel_type {
            TunnelType::Local => {
                let s = Arc::new(LocalForwardStats::new());
                let s2 = s.clone();
                let lh = config.local_host.clone();
                let lp = config.local_port;
                let rh = config.remote_host.clone().unwrap_or_else(|| "localhost".to_string());
                let rp = config.remote_port.unwrap_or(0);
                let ssh = ssh_handle.clone();

                let handle = tokio::spawn(async move {
                    {
                        let mut st = status_clone.write().await;
                        *st = TunnelStatus::Active;
                    }
                    if let Err(e) = local_forward::run_local_forward(
                        ssh, lh, lp, rh, rp, s2, shutdown_rx,
                    ).await {
                        error!("[TunnelManager] Local forward {} error: {}", tid, e);
                        let mut st = status_clone.write().await;
                        *st = TunnelStatus::Error;
                    } else {
                        let mut st = status_clone.write().await;
                        *st = TunnelStatus::Stopped;
                    }
                });

                (handle, TunnelStatsInner::Local(s))
            }
            TunnelType::Remote => {
                let s = Arc::new(RemoteForwardStats::new());
                let s2 = s.clone();
                let lh = config.local_host.clone();
                let lp = config.local_port;
                let rh = config.remote_host.clone().unwrap_or_else(|| "0.0.0.0".to_string());
                let rp = config.remote_port.unwrap_or(0);
                let ssh = ssh_handle.clone();

                let handle = tokio::spawn(async move {
                    {
                        let mut st = status_clone.write().await;
                        *st = TunnelStatus::Active;
                    }
                    if let Err(e) = remote_forward::run_remote_forward(
                        ssh, lh, lp, rh, rp, s2, shutdown_rx,
                    ).await {
                        error!("[TunnelManager] Remote forward {} error: {}", tid, e);
                        let mut st = status_clone.write().await;
                        *st = TunnelStatus::Error;
                    } else {
                        let mut st = status_clone.write().await;
                        *st = TunnelStatus::Stopped;
                    }
                });

                (handle, TunnelStatsInner::Remote(s))
            }
            TunnelType::Dynamic => {
                let s = Arc::new(DynamicForwardStats::new());
                let s2 = s.clone();
                let lh = config.local_host.clone();
                let lp = config.local_port;
                let ssh = ssh_handle.clone();

                let handle = tokio::spawn(async move {
                    {
                        let mut st = status_clone.write().await;
                        *st = TunnelStatus::Active;
                    }
                    if let Err(e) = dynamic_forward::run_dynamic_forward(
                        ssh, lh, lp, s2, shutdown_rx,
                    ).await {
                        error!("[TunnelManager] Dynamic forward {} error: {}", tid, e);
                        let mut st = status_clone.write().await;
                        *st = TunnelStatus::Error;
                    } else {
                        let mut st = status_clone.write().await;
                        *st = TunnelStatus::Stopped;
                    }
                });

                (handle, TunnelStatsInner::Dynamic(s))
            }
        };

        let info = TunnelInfo {
            id: tunnel_id.clone(),
            config: config.clone(),
            session_id: session_id.to_string(),
            status: TunnelStatus::Active,
            bytes_in: 0,
            bytes_out: 0,
            active_connections: 0,
            error_message: None,
        };

        let handle = TunnelHandle {
            id: tunnel_id.clone(),
            config,
            session_id: session_id.to_string(),
            status,
            stats,
            shutdown_tx,
            task_handle,
        };

        let mut tunnels = self.tunnels.write().await;
        tunnels.insert(tunnel_id, handle);

        Ok(info)
    }

    /// Stop a specific tunnel
    pub async fn stop_tunnel(&self, tunnel_id: &str) -> Result<()> {
        let mut tunnels = self.tunnels.write().await;
        if let Some(handle) = tunnels.remove(tunnel_id) {
            let _ = handle.shutdown_tx.send(true);
            handle.task_handle.abort();
            info!("[TunnelManager] Stopped tunnel {}", tunnel_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Tunnel {} not found", tunnel_id))
        }
    }

    /// List all active tunnels, optionally filtered by session_id
    pub async fn list_tunnels(&self, session_id: Option<&str>) -> Vec<TunnelInfo> {
        let tunnels = self.tunnels.read().await;
        let mut result = Vec::new();

        for handle in tunnels.values() {
            if let Some(sid) = session_id {
                if handle.session_id != sid {
                    continue;
                }
            }

            let status = handle.status.read().await.clone();
            let error_msg = if status == TunnelStatus::Error {
                Some("Tunnel encountered an error".to_string())
            } else {
                None
            };

            result.push(TunnelInfo {
                id: handle.id.clone(),
                config: handle.config.clone(),
                session_id: handle.session_id.clone(),
                status,
                bytes_in: handle.stats.bytes_in(),
                bytes_out: handle.stats.bytes_out(),
                active_connections: handle.stats.active_connections(),
                error_message: error_msg,
            });
        }

        result
    }

    /// Stop all tunnels for a specific session
    pub async fn stop_all_for_session(&self, session_id: &str) {
        let mut tunnels = self.tunnels.write().await;
        let to_remove: Vec<String> = tunnels.iter()
            .filter(|(_, h)| h.session_id == session_id)
            .map(|(k, _)| k.clone())
            .collect();

        for id in to_remove {
            if let Some(handle) = tunnels.remove(&id) {
                let _ = handle.shutdown_tx.send(true);
                handle.task_handle.abort();
                info!("[TunnelManager] Stopped tunnel {} (session cleanup)", id);
            }
        }
    }

    /// Stop all tunnels
    pub async fn stop_all(&self) {
        let mut tunnels = self.tunnels.write().await;
        for (id, handle) in tunnels.drain() {
            let _ = handle.shutdown_tx.send(true);
            handle.task_handle.abort();
            info!("[TunnelManager] Stopped tunnel {} (global cleanup)", id);
        }
    }
}
