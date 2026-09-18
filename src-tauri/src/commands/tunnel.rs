use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

use crate::commands::SessionAccessState;
use crate::ipc::runtime_services::{self, RuntimeRequest};
use crate::session::SessionManager;
use crate::storage::{Database, TunnelConfig, TunnelInfo, TunnelType};
use crate::tunnel::TunnelManager;

// The frontend (TunnelPanel/TunnelConfigInput in types/tunnel.ts) sends
// snake_case field names, so each field accepts both its camelCase rename and
// the snake_case alias.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelConfigInput {
    #[serde(alias = "server_id")]
    pub server_id: String,
    #[serde(alias = "tunnel_type")]
    pub tunnel_type: String,
    #[serde(alias = "local_host")]
    pub local_host: String,
    #[serde(alias = "local_port")]
    pub local_port: u16,
    #[serde(alias = "remote_host")]
    pub remote_host: Option<String>,
    #[serde(alias = "remote_port")]
    pub remote_port: Option<u16>,
    #[serde(default, alias = "auto_start")]
    pub auto_start: bool,
    #[serde(default = "default_true", alias = "enabled")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn ensure_runtime_tunnels_supported() -> Result<(), String> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        Err("SSH tunnels are unavailable on mobile in this release".to_string())
    } else {
        Ok(())
    }
}

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
pub fn tunnel_config_delete(db: State<'_, Arc<Database>>, id: String) -> Result<(), String> {
    db.tunnel_config_delete(&id)
        .map_err(|e| format!("Failed to delete tunnel config: {}", e))
}

// === Runtime Tunnel Operations ===

/// Start a tunnel for an active session
#[tauri::command]
pub async fn tunnel_start(
    tunnel_mgr: State<'_, Arc<TunnelManager>>,
    session_mgr: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    session_id: String,
    config: TunnelConfigInput,
) -> Result<TunnelInfo, String> {
    ensure_runtime_tunnels_supported()?;

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

    if access_state.is_remote_session(&session_id).await {
        return runtime_services::call(RuntimeRequest::TunnelStart {
            session_id,
            config: tunnel_config,
        })
        .await;
    }
    let session = session_mgr
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session {session_id} not found"))?;
    if session.server_id != tunnel_config.server_id {
        return Err("Tunnel server does not match the session".into());
    }
    let ssh_handle = session
        .get_ssh_client()
        .await
        .ok_or("SSH session not connected")?;
    tunnel_mgr
        .create_tunnel(&session_id, ssh_handle, tunnel_config)
        .await
        .map_err(|e| format!("Failed to start tunnel: {}", e))
}

/// Stop a running tunnel
#[tauri::command]
pub async fn tunnel_stop(
    tunnel_mgr: State<'_, Arc<TunnelManager>>,
    tunnel_id: String,
    access_state: State<'_, Arc<SessionAccessState>>,
) -> Result<(), String> {
    ensure_runtime_tunnels_supported()?;
    if access_state.is_remote()
        && !tunnel_mgr
            .list_tunnels(None)
            .await
            .iter()
            .any(|tunnel| tunnel.id == tunnel_id)
    {
        return runtime_services::call(RuntimeRequest::TunnelStop { tunnel_id }).await;
    }
    tunnel_mgr
        .stop_tunnel(&tunnel_id)
        .await
        .map_err(|e| format!("Failed to stop tunnel: {}", e))
}

/// List active tunnels, optionally filtered by session_id
#[tauri::command]
pub async fn tunnel_list_active(
    tunnel_mgr: State<'_, Arc<TunnelManager>>,
    session_id: Option<String>,
    access_state: State<'_, Arc<SessionAccessState>>,
) -> Result<Vec<TunnelInfo>, String> {
    ensure_runtime_tunnels_supported()?;
    let mut tunnels = tunnel_mgr.list_tunnels(session_id.as_deref()).await;
    let include_remote = match &session_id {
        Some(id) => access_state.is_remote_session(id).await,
        None => access_state.is_remote(),
    };
    if include_remote {
        let remote: Vec<TunnelInfo> =
            runtime_services::call(RuntimeRequest::TunnelList { session_id }).await?;
        for tunnel in remote {
            if !tunnels.iter().any(|existing| existing.id == tunnel.id) {
                tunnels.push(tunnel);
            }
        }
    }
    Ok(tunnels)
}

/// Stop all tunnels for a session
#[tauri::command]
pub async fn tunnel_stop_all_for_session(
    tunnel_mgr: State<'_, Arc<TunnelManager>>,
    session_id: String,
    access_state: State<'_, Arc<SessionAccessState>>,
) -> Result<(), String> {
    ensure_runtime_tunnels_supported()?;
    if access_state.is_remote_session(&session_id).await {
        return runtime_services::call(RuntimeRequest::TunnelStopSession { session_id }).await;
    }
    tunnel_mgr.stop_all_for_session(&session_id).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_runtime_tunnels_supported, TunnelConfigInput};

    #[test]
    fn runtime_tunnel_support_matches_the_target() {
        let result = ensure_runtime_tunnels_supported();
        if cfg!(any(target_os = "android", target_os = "ios")) {
            assert!(result.is_err());
        } else {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn tunnel_config_input_accepts_snake_case_payload_from_frontend() {
        // TunnelPanel.tsx builds exactly this shape (types/tunnel.ts declares
        // snake_case fields) and sends it to tunnel_config_add/update/start.
        let payload = r#"{
            "server_id": "srv-1",
            "tunnel_type": "local",
            "local_host": "127.0.0.1",
            "local_port": 8080,
            "remote_host": "localhost",
            "remote_port": 80,
            "auto_start": false,
            "enabled": true
        }"#;

        let input: TunnelConfigInput = serde_json::from_str(payload).expect("deserialize");
        assert_eq!(input.server_id, "srv-1");
        assert_eq!(input.tunnel_type, "local");
        assert_eq!(input.local_host, "127.0.0.1");
        assert_eq!(input.local_port, 8080);
        assert_eq!(input.remote_host.as_deref(), Some("localhost"));
        assert_eq!(input.remote_port, Some(80));
        assert!(!input.auto_start);
        assert!(input.enabled);
    }

    #[test]
    fn tunnel_config_input_accepts_camel_case_payload() {
        let payload = r#"{
            "serverId": "srv-1",
            "tunnelType": "dynamic",
            "localHost": "127.0.0.1",
            "localPort": 1080
        }"#;

        let input: TunnelConfigInput = serde_json::from_str(payload).expect("deserialize");
        assert_eq!(input.server_id, "srv-1");
        assert_eq!(input.tunnel_type, "dynamic");
        assert_eq!(input.local_port, 1080);
        assert_eq!(input.remote_host, None);
        assert_eq!(input.remote_port, None);
        assert!(!input.auto_start);
        // `enabled` defaults to true when omitted.
        assert!(input.enabled);
    }
}
