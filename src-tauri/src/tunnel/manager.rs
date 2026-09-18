use crate::ssh::SshClient;
use crate::storage::models::{TunnelConfig, TunnelInfo, TunnelStatus, TunnelType};
use crate::tunnel::dynamic_forward::{self, DynamicForwardStats};
use crate::tunnel::local_forward::{self, LocalForwardStats};
use crate::tunnel::remote_forward::{self, RemoteForwardStats};
use anyhow::{anyhow, ensure, Result};
use log::{error, info};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, watch, Mutex, RwLock};
use uuid::Uuid;

#[derive(Clone)]
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
    creation_lock: Mutex<()>,
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
            creation_lock: Mutex::new(()),
        }
    }

    /// Return Active only after the listener or remote forward actually exists.
    pub async fn create_tunnel(
        &self,
        session_id: &str,
        ssh_client: SshClient,
        mut config: TunnelConfig,
    ) -> Result<TunnelInfo> {
        let _creation = self.creation_lock.lock().await;
        ensure!(ssh_client.is_connected().await, "SSH session not connected");
        ensure!(
            !config.local_host.is_empty(),
            "Local host must not be empty"
        );
        match config.tunnel_type {
            TunnelType::Local => ensure!(
                config.remote_port.is_some_and(|port| port != 0),
                "Remote destination port must be nonzero"
            ),
            TunnelType::Remote => ensure!(
                config.local_port != 0,
                "Local destination port must be nonzero"
            ),
            TunnelType::Dynamic => (),
        }
        let tunnel_id = if config.id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            config.id.clone()
        };
        ensure!(
            !self.tunnels.read().await.contains_key(&tunnel_id),
            "Tunnel is already running: {tunnel_id}"
        );
        let ssh_handle = ssh_client.session_arc();
        let registry = ssh_client.remote_forward_registry();
        let status = Arc::new(RwLock::new(TunnelStatus::Starting));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (ready_tx, ready_rx) = oneshot::channel();
        let stats = match config.tunnel_type {
            TunnelType::Local => TunnelStatsInner::Local(Arc::new(LocalForwardStats::new())),
            TunnelType::Remote => TunnelStatsInner::Remote(Arc::new(RemoteForwardStats::new())),
            TunnelType::Dynamic => TunnelStatsInner::Dynamic(Arc::new(DynamicForwardStats::new())),
        };
        let worker_stats = stats.clone();
        let worker_config = config.clone();
        let status_clone = status.clone();
        let tid = tunnel_id.clone();
        let mut task_handle = tokio::spawn(async move {
            let c = worker_config;
            let result = match worker_stats {
                TunnelStatsInner::Local(s) => {
                    local_forward::run_local_forward(
                        ssh_handle,
                        (c.local_host, c.local_port),
                        (
                            c.remote_host.unwrap_or_else(|| "localhost".into()),
                            c.remote_port.unwrap_or(0),
                        ),
                        s,
                        shutdown_rx,
                        ready_tx,
                    )
                    .await
                }
                TunnelStatsInner::Remote(s) => {
                    remote_forward::run_remote_forward(
                        ssh_handle,
                        (c.local_host, c.local_port),
                        (
                            c.remote_host.unwrap_or_else(|| "127.0.0.1".into()),
                            c.remote_port.unwrap_or(0),
                        ),
                        s,
                        shutdown_rx,
                        registry,
                        ready_tx,
                    )
                    .await
                }
                TunnelStatsInner::Dynamic(s) => {
                    dynamic_forward::run_dynamic_forward(
                        ssh_handle,
                        c.local_host,
                        c.local_port,
                        s,
                        shutdown_rx,
                        ready_tx,
                    )
                    .await
                }
            };
            *status_clone.write().await = match result {
                Ok(()) => TunnelStatus::Stopped,
                Err(error) => {
                    error!("[TunnelManager] Tunnel {tid} failed: {error}");
                    TunnelStatus::Error
                }
            };
        });
        let started = tokio::time::timeout(Duration::from_secs(20), ready_rx).await;
        let actual_port = match started {
            Ok(Ok(Ok(port))) => port,
            other => {
                let _ = shutdown_tx.send(true);
                if tokio::time::timeout(Duration::from_secs(4), &mut task_handle)
                    .await
                    .is_err()
                {
                    task_handle.abort();
                    let _ = task_handle.await;
                }
                return Err(anyhow!("Tunnel did not start: {other:?}"));
            }
        };
        match config.tunnel_type {
            TunnelType::Remote => config.remote_port = Some(actual_port),
            _ => config.local_port = actual_port,
        }
        {
            let mut current = status.write().await;
            if *current == TunnelStatus::Starting {
                *current = TunnelStatus::Active;
            }
        }
        let info = TunnelInfo {
            id: tunnel_id.clone(),
            config: config.clone(),
            session_id: session_id.into(),
            status: status.read().await.clone(),
            bytes_in: 0,
            bytes_out: 0,
            active_connections: 0,
            error_message: None,
        };
        self.tunnels.write().await.insert(
            tunnel_id.clone(),
            TunnelHandle {
                id: tunnel_id,
                config,
                session_id: session_id.into(),
                status,
                stats,
                shutdown_tx,
                task_handle,
            },
        );
        Ok(info)
    }

    async fn finish(mut handle: TunnelHandle) {
        let _ = handle.shutdown_tx.send(true);
        // Let reverse forwarding send cancel-tcpip-forward before aborting.
        if tokio::time::timeout(Duration::from_secs(4), &mut handle.task_handle)
            .await
            .is_err()
        {
            handle.task_handle.abort();
            let _ = handle.task_handle.await;
        }
        info!("[TunnelManager] Stopped tunnel {}", handle.id);
    }

    pub async fn stop_tunnel(&self, tunnel_id: &str) -> Result<()> {
        let handle = self
            .tunnels
            .write()
            .await
            .remove(tunnel_id)
            .ok_or_else(|| anyhow!("Tunnel {tunnel_id} not found"))?;
        Self::finish(handle).await;
        Ok(())
    }

    pub async fn list_tunnels(&self, session_id: Option<&str>) -> Vec<TunnelInfo> {
        let tunnels = self.tunnels.read().await;
        let mut result = Vec::new();
        for handle in tunnels.values() {
            if session_id.is_some_and(|id| handle.session_id != id) {
                continue;
            }
            let status = handle.status.read().await.clone();
            let error_message =
                (status == TunnelStatus::Error).then(|| "Tunnel encountered an error".into());
            result.push(TunnelInfo {
                id: handle.id.clone(),
                config: handle.config.clone(),
                session_id: handle.session_id.clone(),
                status,
                bytes_in: handle.stats.bytes_in(),
                bytes_out: handle.stats.bytes_out(),
                active_connections: handle.stats.active_connections(),
                error_message,
            });
        }
        result
    }

    pub async fn stop_all_for_session(&self, session_id: &str) {
        let handles = {
            let mut tunnels = self.tunnels.write().await;
            let ids: Vec<_> = tunnels
                .iter()
                .filter(|(_, h)| h.session_id == session_id)
                .map(|(id, _)| id.clone())
                .collect();
            ids.into_iter()
                .filter_map(|id| tunnels.remove(&id))
                .collect::<Vec<_>>()
        };
        for handle in handles {
            Self::finish(handle).await;
        }
    }
    pub async fn stop_all(&self) {
        let handles = std::mem::take(&mut *self.tunnels.write().await);
        for handle in handles.into_values() {
            Self::finish(handle).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(tunnel_type: TunnelType) -> TunnelConfig {
        TunnelConfig {
            id: "config-1".to_string(),
            server_id: "srv-1".to_string(),
            tunnel_type,
            local_host: "127.0.0.1".to_string(),
            local_port: 8080,
            remote_host: Some("db.internal".to_string()),
            remote_port: Some(5432),
            auto_start: false,
            enabled: true,
        }
    }

    /// Build stats for each forward variant pre-seeded with the given counters,
    /// the way a live forwarding loop would have incremented them.
    fn seeded_stats(
        tunnel_type: TunnelType,
        bytes_in: u64,
        bytes_out: u64,
        connections: u32,
    ) -> TunnelStatsInner {
        fn fill(stats: &TunnelStatsInner, bytes_in: u64, bytes_out: u64, connections: u32) {
            match stats {
                TunnelStatsInner::Local(s) => {
                    s.bytes_in.store(bytes_in, Ordering::Relaxed);
                    s.bytes_out.store(bytes_out, Ordering::Relaxed);
                    s.active_connections.store(connections, Ordering::Relaxed);
                }
                TunnelStatsInner::Remote(s) => {
                    s.bytes_in.store(bytes_in, Ordering::Relaxed);
                    s.bytes_out.store(bytes_out, Ordering::Relaxed);
                    s.active_connections.store(connections, Ordering::Relaxed);
                }
                TunnelStatsInner::Dynamic(s) => {
                    s.bytes_in.store(bytes_in, Ordering::Relaxed);
                    s.bytes_out.store(bytes_out, Ordering::Relaxed);
                    s.active_connections.store(connections, Ordering::Relaxed);
                }
            }
        }

        let stats = match tunnel_type {
            TunnelType::Local => TunnelStatsInner::Local(Arc::new(LocalForwardStats::new())),
            TunnelType::Remote => TunnelStatsInner::Remote(Arc::new(RemoteForwardStats::new())),
            TunnelType::Dynamic => TunnelStatsInner::Dynamic(Arc::new(DynamicForwardStats::new())),
        };
        fill(&stats, bytes_in, bytes_out, connections);
        stats
    }

    /// Register a handle directly in the manager map. `create_tunnel` itself
    /// needs a live SSH handle, so tests seed the same bookkeeping state the
    /// spawn path would produce and exercise the accounting from there.
    async fn insert_handle(
        manager: &TunnelManager,
        id: &str,
        session_id: &str,
        status: TunnelStatus,
        stats: TunnelStatsInner,
    ) {
        let (shutdown_tx, _shutdown_rx) = watch::channel(false);
        let handle = TunnelHandle {
            id: id.to_string(),
            config: test_config(TunnelType::Local),
            session_id: session_id.to_string(),
            status: Arc::new(RwLock::new(status)),
            stats,
            shutdown_tx,
            task_handle: tokio::spawn(async {}),
        };
        manager.tunnels.write().await.insert(id.to_string(), handle);
    }

    #[test]
    fn stats_inner_starts_zeroed_for_all_variants() {
        let variants = [
            TunnelStatsInner::Local(Arc::new(LocalForwardStats::new())),
            TunnelStatsInner::Remote(Arc::new(RemoteForwardStats::new())),
            TunnelStatsInner::Dynamic(Arc::new(DynamicForwardStats::new())),
        ];
        for stats in &variants {
            assert_eq!(stats.bytes_in(), 0);
            assert_eq!(stats.bytes_out(), 0);
            assert_eq!(stats.active_connections(), 0);
        }
    }

    #[test]
    fn stats_inner_reads_updated_counters_for_all_variants() {
        let variants = [
            seeded_stats(TunnelType::Local, 1234, 5678, 3),
            seeded_stats(TunnelType::Remote, 1234, 5678, 3),
            seeded_stats(TunnelType::Dynamic, 1234, 5678, 3),
        ];
        for stats in &variants {
            assert_eq!(stats.bytes_in(), 1234);
            assert_eq!(stats.bytes_out(), 5678);
            assert_eq!(stats.active_connections(), 3);
        }
    }

    #[tokio::test]
    async fn list_tunnels_reports_status_accounting_and_error_message() {
        let manager = TunnelManager::new();
        insert_handle(
            &manager,
            "t-local",
            "session-a",
            TunnelStatus::Active,
            seeded_stats(TunnelType::Local, 100, 200, 2),
        )
        .await;
        insert_handle(
            &manager,
            "t-dynamic",
            "session-a",
            TunnelStatus::Error,
            seeded_stats(TunnelType::Dynamic, 5, 6, 0),
        )
        .await;
        insert_handle(
            &manager,
            "t-remote",
            "session-b",
            TunnelStatus::Starting,
            seeded_stats(TunnelType::Remote, 0, 0, 0),
        )
        .await;

        let mut infos = manager.list_tunnels(None).await;
        infos.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(infos.len(), 3);

        let local = infos.iter().find(|t| t.id == "t-local").unwrap();
        assert_eq!(local.status, TunnelStatus::Active);
        assert_eq!(local.bytes_in, 100);
        assert_eq!(local.bytes_out, 200);
        assert_eq!(local.active_connections, 2);
        assert_eq!(local.error_message, None);

        let dynamic = infos.iter().find(|t| t.id == "t-dynamic").unwrap();
        assert_eq!(dynamic.status, TunnelStatus::Error);
        assert_eq!(
            dynamic.error_message,
            Some("Tunnel encountered an error".to_string())
        );

        let remote = infos.iter().find(|t| t.id == "t-remote").unwrap();
        assert_eq!(remote.status, TunnelStatus::Starting);
        assert_eq!(remote.error_message, None);
    }

    #[tokio::test]
    async fn list_tunnels_filters_by_session_id() {
        let manager = TunnelManager::new();
        insert_handle(
            &manager,
            "t-a",
            "session-a",
            TunnelStatus::Active,
            seeded_stats(TunnelType::Local, 1, 1, 1),
        )
        .await;
        insert_handle(
            &manager,
            "t-b",
            "session-b",
            TunnelStatus::Active,
            seeded_stats(TunnelType::Local, 2, 2, 1),
        )
        .await;

        let infos = manager.list_tunnels(Some("session-a")).await;
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, "t-a");
        assert_eq!(infos[0].session_id, "session-a");
    }

    #[tokio::test]
    async fn stop_tunnel_removes_handle_and_errors_when_missing() {
        let manager = TunnelManager::new();
        insert_handle(
            &manager,
            "t-1",
            "session-a",
            TunnelStatus::Active,
            seeded_stats(TunnelType::Local, 0, 0, 0),
        )
        .await;

        assert!(manager.stop_tunnel("t-1").await.is_ok());
        assert!(manager.list_tunnels(None).await.is_empty());

        // Stopping an unknown (or already stopped) tunnel is an error.
        assert!(manager.stop_tunnel("t-1").await.is_err());
        assert!(manager.stop_tunnel("never-existed").await.is_err());
    }

    #[tokio::test]
    async fn stop_all_for_session_only_stops_matching_tunnels() {
        let manager = TunnelManager::new();
        for id in ["t-a1", "t-a2"] {
            insert_handle(
                &manager,
                id,
                "session-a",
                TunnelStatus::Active,
                seeded_stats(TunnelType::Local, 0, 0, 0),
            )
            .await;
        }
        insert_handle(
            &manager,
            "t-b1",
            "session-b",
            TunnelStatus::Active,
            seeded_stats(TunnelType::Local, 0, 0, 0),
        )
        .await;

        manager.stop_all_for_session("session-a").await;

        let remaining = manager.list_tunnels(None).await;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "t-b1");

        manager.stop_all().await;
        assert!(manager.list_tunnels(None).await.is_empty());
    }
}
