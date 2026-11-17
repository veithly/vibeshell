//! Operations that must run in the process that owns the SSH session.
use crate::{session::SessionManager, storage::TunnelConfig};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub enum RuntimeRequest {
    TunnelStart {
        session_id: String,
        config: TunnelConfig,
    },
    TunnelStop {
        tunnel_id: String,
    },
    TunnelList {
        session_id: Option<String>,
    },
    TunnelStopSession {
        session_id: String,
    },
    RecordingStart {
        session_id: String,
        server_id: String,
    },
    RecordingStop {
        recording_id: String,
    },
    RecordingStatus {
        session_id: String,
    },
}

pub async fn call<T: serde::de::DeserializeOwned>(request: RuntimeRequest) -> Result<T, String> {
    use super::IpcMessage;
    match crate::commands::session::ipc_send(IpcMessage::SessionService { request }).await? {
        IpcMessage::ServiceResult { value } => serde_json::from_value(value).map_err(|error| error.to_string()),
        IpcMessage::Error { message } => Err(message),
        _ => Err("Running VibeShell service does not support this session operation; update both GUI and daemon".into()),
    }
}

pub async fn dispatch(manager: &SessionManager, request: RuntimeRequest) -> Result<Value, String> {
    match request {
        RuntimeRequest::TunnelStart { session_id, config } => {
            let service = manager
                .tunnel_service()
                .ok_or("Tunnel service is unavailable")?;
            let session = manager.get(&session_id).await.ok_or("Session not found")?;
            if session.server_id != config.server_id {
                return Err("Tunnel server does not match the session".into());
            }
            let client = session
                .get_ssh_client()
                .await
                .ok_or("SSH session not connected")?;
            let result = service
                .create_tunnel(&session_id, client, config)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!(result))
        }
        RuntimeRequest::TunnelStop { tunnel_id } => {
            manager
                .tunnel_service()
                .ok_or("Tunnel service is unavailable")?
                .stop_tunnel(&tunnel_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        RuntimeRequest::TunnelList { session_id } => Ok(json!(
            manager
                .tunnel_service()
                .ok_or("Tunnel service is unavailable")?
                .list_tunnels(session_id.as_deref())
                .await
        )),
        RuntimeRequest::TunnelStopSession { session_id } => {
            manager
                .tunnel_service()
                .ok_or("Tunnel service is unavailable")?
                .stop_all_for_session(&session_id)
                .await;
            Ok(Value::Null)
        }
        RuntimeRequest::RecordingStart {
            session_id,
            server_id,
        } => {
            let service = manager
                .recording_service()
                .ok_or("Recording service is unavailable")?;
            let session = manager.get(&session_id).await.ok_or("Session not found")?;
            if session.server_id != server_id {
                return Err("Recording server does not match the session".into());
            }
            Ok(json!(service
                .start_recording(session, &server_id)
                .await
                .map_err(|e| e.to_string())?))
        }
        RuntimeRequest::RecordingStop { recording_id } => {
            manager
                .recording_service()
                .ok_or("Recording service is unavailable")?
                .stop_recording(&recording_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        RuntimeRequest::RecordingStatus { session_id } => Ok(json!(
            manager
                .recording_service()
                .ok_or("Recording service is unavailable")?
                .get_recording_id(&session_id)
                .await
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[tokio::test]
    async fn services_use_injected_owners_and_reject_missing_sessions() {
        let temporary = tempfile::tempdir().unwrap();
        let database =
            Arc::new(crate::Database::new_at(temporary.path().join("runtime.db")).unwrap());
        let manager = SessionManager::new(database);
        assert!(
            dispatch(&manager, RuntimeRequest::TunnelList { session_id: None })
                .await
                .is_err()
        );
        manager.set_tunnel_manager(Arc::new(crate::tunnel::TunnelManager::new()));
        assert_eq!(
            dispatch(&manager, RuntimeRequest::TunnelList { session_id: None })
                .await
                .unwrap(),
            json!([])
        );
        assert!(dispatch(
            &manager,
            RuntimeRequest::TunnelStop {
                tunnel_id: "missing".into()
            }
        )
        .await
        .is_err());
        assert!(dispatch(
            &manager,
            RuntimeRequest::RecordingStart {
                session_id: "missing".into(),
                server_id: "missing".into()
            }
        )
        .await
        .is_err());
        let request = RuntimeRequest::TunnelStopSession {
            session_id: "missing".into(),
        };
        let message = super::super::IpcMessage::SessionService { request };
        let encoded = serde_json::to_string(&message).unwrap();
        let decoded: super::super::IpcMessage = serde_json::from_str(&encoded).unwrap();
        assert!(matches!(
            decoded,
            super::super::IpcMessage::SessionService { .. }
        ));
    }
}
