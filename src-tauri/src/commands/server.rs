use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

use crate::storage::database::Group;
use crate::storage::{AuthType, Database, Server};

/// Server input from frontend (without auto-generated fields)
/// Frontend sends snake_case field names (auth_type, credential_id, etc.)
#[derive(Debug, Deserialize)]
pub struct ServerInput {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(alias = "authType")]
    pub auth_type: String,
    #[serde(alias = "credentialId")]
    pub credential_id: Option<String>,
    #[serde(alias = "groupId")]
    pub group_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    #[serde(alias = "jumpHostId")]
    pub jump_host_id: Option<String>,
    #[serde(default)]
    #[serde(alias = "postLoginCommand")]
    pub post_login_command: Option<String>,
    #[serde(default)]
    #[serde(alias = "agentForwarding")]
    pub agent_forwarding: bool,
}

/// Group input from frontend
#[derive(Debug, Deserialize)]
pub struct GroupInput {
    pub name: String,
    pub parent_id: Option<String>,
    pub color: String,
}

fn string_to_auth_type(s: &str) -> AuthType {
    match s {
        "password" => AuthType::Password,
        // Standalone "key" auth is deprecated: keys are always handled through
        // the key+passphrase flow (an empty passphrase means an unencrypted key).
        "key" | "key_with_passphrase" => AuthType::KeyWithPassphrase,
        _ => AuthType::Password,
    }
}

/// Get all servers
#[tauri::command]
pub fn get_servers(db: State<'_, Arc<Database>>) -> Result<Vec<Server>, String> {
    db.server_list(None, None)
        .map_err(|e| format!("Failed to get servers: {}", e))
}

/// Add a new server
#[tauri::command]
pub fn add_server(db: State<'_, Arc<Database>>, server: ServerInput) -> Result<Server, String> {
    let mut new_server = Server {
        id: String::new(),
        name: server.name,
        host: server.host,
        port: server.port,
        username: server.username,
        auth_type: string_to_auth_type(&server.auth_type),
        credential_id: server.credential_id,
        group_id: server.group_id,
        tags: server.tags,
        created_at: 0,
        updated_at: 0,
        jump_host_id: server.jump_host_id,
        post_login_command: server.post_login_command,
        agent_forwarding: server.agent_forwarding,
    };

    db.server_add(&mut new_server)
        .map_err(|e| format!("Failed to add server: {}", e))?;

    Ok(new_server)
}

/// Partial server update input — all fields optional except name/host/port/username
#[derive(Deserialize)]
pub struct ServerUpdateInput {
    /// Omitted means preserve credentials; never deserialize into Server itself.
    pub credentials: Option<crate::storage::database::CredentialUpdate>,
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    #[serde(alias = "authType")]
    pub auth_type: Option<String>,
    #[serde(
        default,
        alias = "credentialId",
        deserialize_with = "deserialize_present_option"
    )]
    pub credential_id: Option<Option<String>>,
    #[serde(
        default,
        alias = "groupId",
        deserialize_with = "deserialize_present_option"
    )]
    pub group_id: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        alias = "jumpHostId",
        deserialize_with = "deserialize_present_option"
    )]
    pub jump_host_id: Option<Option<String>>,
    #[serde(
        default,
        alias = "postLoginCommand",
        deserialize_with = "deserialize_present_option"
    )]
    pub post_login_command: Option<Option<String>>,
    #[serde(alias = "agentForwarding")]
    pub agent_forwarding: Option<bool>,
}

fn deserialize_present_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Update an existing server (partial update — only sent fields are changed)
///
/// Also keeps device-local credentials coherent with the new config:
/// - Renaming a server re-keys its saved credentials (they are name-keyed).
/// - Auth method changes require replacement credentials in the same transaction.
/// - Legacy key/key_with_passphrase normalization preserves saved key material.
#[tauri::command]
pub fn update_server(
    db: State<'_, Arc<Database>>,
    id: String,
    updates: ServerUpdateInput,
) -> Result<(), String> {
    let existing = db
        .server_get(&id)
        .map_err(|e| format!("Failed to get server: {}", e))?
        .ok_or_else(|| "Server not found".to_string())?;

    #[cfg(any(target_os = "android", target_os = "ios"))]
    if updates.credentials.is_some() {
        return Err("Saving credentials on mobile requires Keychain or Keystore support".into());
    }
    if updates
        .auth_type
        .as_deref()
        .is_some_and(|auth| !matches!(auth, "password" | "key" | "key_with_passphrase"))
    {
        return Err("Unknown authentication type".into());
    }

    let updated_server = Server {
        id: existing.id,
        name: updates.name.unwrap_or(existing.name),
        host: updates.host.unwrap_or(existing.host),
        port: updates.port.unwrap_or(existing.port),
        username: updates.username.unwrap_or(existing.username),
        auth_type: updates
            .auth_type
            .map(|a| string_to_auth_type(&a))
            .unwrap_or(existing.auth_type),
        credential_id: updates.credential_id.unwrap_or(existing.credential_id),
        group_id: updates.group_id.unwrap_or(existing.group_id),
        tags: updates.tags.unwrap_or(existing.tags),
        created_at: existing.created_at,
        updated_at: 0,
        jump_host_id: updates.jump_host_id.unwrap_or(existing.jump_host_id),
        post_login_command: updates
            .post_login_command
            .unwrap_or(existing.post_login_command),
        agent_forwarding: updates
            .agent_forwarding
            .unwrap_or(existing.agent_forwarding),
    };

    if updated_server.name.trim().is_empty()
        || updated_server.host.trim().is_empty()
        || updated_server.username.trim().is_empty()
        || updated_server.port == 0
    {
        return Err("Name, host, username and a valid port are required".into());
    }
    db.server_update_with_credentials(&updated_server, updates.credentials.as_ref())
        .map_err(|e| format!("Failed to update server: {}", e))
}

/// Delete a server and clean up everything attached to it:
/// active SSH sessions, per-session SFTP state, and saved credentials.
/// Tunnel configs and jump-host references are detached inside `server_delete`.
#[tauri::command]
pub async fn delete_server(
    db: State<'_, Arc<Database>>,
    manager: State<'_, Arc<crate::session::SessionManager>>,
    sftp_state: State<'_, Arc<super::SftpState>>,
    id: String,
) -> Result<(), String> {
    let server = db
        .server_get(&id)
        .map_err(|e| format!("Failed to get server: {}", e))?
        .ok_or_else(|| "Server not found".to_string())?;

    // Terminate live sessions first so nothing keeps using the doomed config.
    let killed_sessions = manager
        .kill_by_server_id(&id)
        .await
        .map_err(|e| format!("Failed to close sessions for server: {}", e))?;
    for session_id in &killed_sessions {
        sftp_state.cleanup_session(session_id).await;
    }

    // Remove device-local credentials tied to this server's name.
    if let Err(e) = db.credential_delete(&server.name) {
        log::warn!("Failed to delete credentials for '{}': {}", server.name, e);
    }

    db.server_delete(&id)
        .map_err(|e| format!("Failed to delete server: {}", e))
}

/// Get all groups
#[tauri::command]
pub fn get_groups(db: State<'_, Arc<Database>>) -> Result<Vec<Group>, String> {
    db.group_list()
        .map_err(|e| format!("Failed to get groups: {}", e))
}

/// Add a new group
#[tauri::command]
pub fn add_group(db: State<'_, Arc<Database>>, group: GroupInput) -> Result<Group, String> {
    let mut new_group = Group {
        id: String::new(),
        name: group.name,
        parent_id: group.parent_id,
        color: group.color,
    };

    db.group_add(&mut new_group)
        .map_err(|e| format!("Failed to add group: {}", e))?;

    Ok(new_group)
}

/// Delete a group
#[tauri::command]
pub fn delete_group(db: State<'_, Arc<Database>>, id: String) -> Result<(), String> {
    db.group_delete(&id)
        .map_err(|e| format!("Failed to delete group: {}", e))
}

/// Input for saving credentials
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCredentialInput {
    pub server_name: String,
    pub auth_type: String,
    pub credential: String,
    pub passphrase: Option<String>,
    pub key_path: Option<String>,
}

/// Input for getting/deleting credentials
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialServerInput {
    pub server_name: String,
}

/// Save credentials for a server
#[tauri::command]
pub fn save_credential(
    db: State<'_, Arc<Database>>,
    request: SaveCredentialInput,
) -> Result<String, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (db, request);
        return Err(
            "Saving credentials on mobile requires Keychain or Keystore support".to_string(),
        );
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    db.credential_save(
        &request.server_name,
        &request.auth_type,
        &request.credential,
        request.passphrase.as_deref(),
        request.key_path.as_deref(),
    )
    .map_err(|e| format!("Failed to save credentials: {}", e))
}

/// Get credentials for a server
#[tauri::command]
pub fn get_credential(
    db: State<'_, Arc<Database>>,
    request: CredentialServerInput,
) -> Result<Option<crate::storage::database::Credential>, String> {
    db.credential_get(&request.server_name)
        .map_err(|e| format!("Failed to get credentials: {}", e))
}

#[cfg(test)]
mod tests {
    use super::{ServerInput, ServerUpdateInput};
    use serde_json::json;

    #[test]
    fn server_input_deserializes_snake_case_auth_type() {
        let value = json!({
            "name": "prod-1",
            "host": "10.0.0.1",
            "port": 22,
            "username": "root",
            "auth_type": "password"
        });

        let parsed: ServerInput =
            serde_json::from_value(value).expect("snake_case should deserialize");
        assert_eq!(parsed.auth_type, "password");
    }

    #[test]
    fn server_input_deserializes_camel_case_auth_type() {
        let value = json!({
            "name": "prod-1",
            "host": "10.0.0.1",
            "port": 22,
            "username": "root",
            "authType": "password"
        });

        let parsed: ServerInput =
            serde_json::from_value(value).expect("camelCase should deserialize");
        assert_eq!(parsed.auth_type, "password");
    }

    #[test]
    fn server_update_distinguishes_missing_relationships_from_explicit_null() {
        let omitted: ServerUpdateInput =
            serde_json::from_value(json!({})).expect("omitted fields should deserialize");
        assert!(omitted.group_id.is_none());
        assert!(omitted.jump_host_id.is_none());

        let explicit_null: ServerUpdateInput = serde_json::from_value(json!({
            "groupId": null,
            "jumpHostId": null
        }))
        .expect("explicit null fields should deserialize");
        assert!(explicit_null.group_id.is_some());
        assert!(explicit_null.jump_host_id.is_some());
    }
}
