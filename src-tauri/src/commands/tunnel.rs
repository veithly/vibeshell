use std::sync::Arc;
use tauri::State;
use serde::Deserialize;

use crate::session::SessionManager;
use crate::storage::{Database, TunnelConfig, TunnelType, TunnelInfo};
use crate::tunnel::TunnelManager;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelConfigInput {
    pub server_id: String,
    pub tunnel_type: String,
    pub local_host: String,
    pub local_port: u16,
    pub remote_host: Option<String>,
    pub remote_port: Option<u16>,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

fn parse_tunnel_type(s: &str) -> TunnelType {
    match s {
        "local" => TunnelType::Local,
        "remote" => TunnelType::Remote,
        "dynamic" => TunnelType::Dynamic,
        _ => TunnelType::Local,
    }
}

// === Tunnel Config CRUD (persistent) ===

/// Get all tunnel configs for a server
#[tauri::command]
pub fn tunnel_config_list(
    db: State<'_, Arc<Database>>,
    server_id: String,
) -> Result<Vec<TunnelConfig>, String> {
    db.tunnel_config_list(&server_id)
        .map_err(|e| format!("Failed to list tunnel configs: {}", e))
}

/// Add a tunnel config
#[tauri::command]
pub fn tunnel_config_add(
    db: State<'_, Arc<Database>>,
    input: TunnelConfigInput,
) -> Result<TunnelConfig, String> {
    let mut config = TunnelConfig {
        id: String::new(),
        server_id: input.server_id,
        tunnel_type: parse_tunnel_type(&input.tunnel_type),
        local_host: input.local_host,
        local_port: input.local_port,
        remote_host: input.remote_host,
        remote_port: input.remote_port,
        auto_start: input.auto_start,
        enabled: input.enabled,
    };

    db.tunnel_config_add(&mut config)
        .map_err(|e| format!("Failed to add tunnel config: {}", e))?;

    Ok(config)
}

/// Update a tunnel config
#[tauri::command]
pub fn tunnel_config_update(
    db: State<'_, Arc<Database>>,
    id: String,
    input: TunnelConfigInput,
) -> Result<(), String> {
    let config = TunnelConfig {
        id,
        server_id: input.server_id,
        tunnel_type: parse_tunnel_type(&input.tunnel_type),
        local_host: input.local_host,
        local_port: input.local_port,
        remote_host: input.remote_host,
        remote_port: input.remote_port,
        auto_start: input.auto_start,
        enabled: input.enabled,
    };

    db.tunnel_config_update(&config)
        .map_err(|e| format!("Failed to update tunnel config: {}", e))
}

/// Delete a tunnel config
#[tauri::command]
pub fn tunnel_config_delete(
    db: State<'_, Arc<Database>>,
    id: String,
) -> Result<(), String> {
    db.tunnel_config_delete(&id)
        .map_err(|e| format!("Failed to delete tunnel config: {}", e))
}

// === Runtime Tunnel Operations ===

/// Start a tunnel for an active session
#[tauri::command]
pub async fn tunnel_start(
    tunnel_mgr: State<'_, Arc<TunnelManager>>,
    session_mgr: State<'_, Arc<SessionManager>>,
    session_id: String,
    config: TunnelConfigInput,
) -> Result<TunnelInfo, String> {
    // Get the session to access SSH handle
    let session = session_mgr.get(&session_id).await
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    // Get SSH session handle Arc from the session's SSH client
    let ssh_handle = session.get_ssh_handle_arc().await
        .ok_or_else(|| "SSH session not connected".to_string())?;

    let tunnel_config = TunnelConfig {
        id: String::new(),
        server_id: config.server_id,
        tunnel_type: parse_tunnel_type(&config.tunnel_type),
        local_host: config.local_host,
        local_port: config.local_port,
        remote_host: config.remote_host,
        remote_port: config.remote_port,
        auto_start: config.auto_start,
        enabled: config.enabled,
    };

    tunnel_mgr.create_tunnel(&session_id, ssh_handle, tunnel_config).await
        .map_err(|e| format!("Failed to start tunnel: {}", e))
}

/// Stop a running tunnel
#[tauri::command]
pub async fn tunnel_stop(
    tunnel_mgr: State<'_, Arc<TunnelManager>>,
    tunnel_id: String,
) -> Result<(), String> {
    tunnel_mgr.stop_tunnel(&tunnel_id).await
        .map_err(|e| format!("Failed to stop tunnel: {}", e))
}

/// List active tunnels, optionally filtered by session_id
#[tauri::command]
pub async fn tunnel_list_active(
    tunnel_mgr: State<'_, Arc<TunnelManager>>,
    session_id: Option<String>,
) -> Result<Vec<TunnelInfo>, String> {
    Ok(tunnel_mgr.list_tunnels(session_id.as_deref()).await)
}

/// Stop all tunnels for a session
#[tauri::command]
pub async fn tunnel_stop_all_for_session(
    tunnel_mgr: State<'_, Arc<TunnelManager>>,
    session_id: String,
) -> Result<(), String> {
    tunnel_mgr.stop_all_for_session(&session_id).await;
    Ok(())
}
