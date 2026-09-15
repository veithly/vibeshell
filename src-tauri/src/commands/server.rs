use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::storage::database::Group;
use crate::storage::{AuthType, ConnectionKind, Database, Server};

/// Shared add-server payload used by the GUI command and the CLI IPC path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddServerSpec {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub group_id: Option<String>,
    #[serde(default)]
    pub group_name: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub jump_host_id: Option<String>,
    #[serde(default)]
    pub jump_host: Option<String>,
    #[serde(default)]
    pub post_login_command: Option<String>,
    #[serde(default)]
    pub agent_forwarding: bool,
    #[serde(default)]
    pub connection_kind: Option<String>,
    #[serde(default)]
    pub teleport_proxy: Option<String>,
    #[serde(default)]
    pub credential: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
}

/// Insert a server row and optionally save device-local credentials.
///
/// `group_name` / `jump_host` are resolved by unique name when the corresponding
/// ID is not provided (CLI convenience).
pub fn add_server_spec(db: &Database, spec: AddServerSpec) -> Result<Server, String> {
    let name = spec.name.trim().to_string();
    let host = spec.host.trim().to_string();
    let username = spec.username.trim().to_string();
    if name.is_empty() {
        return Err("Server name is required".to_string());
    }
    if host.is_empty() {
        return Err("Host is required".to_string());
    }
    if username.is_empty() {
        return Err("Username is required".to_string());
    }
    if spec.port == 0 {
        return Err("Port must be between 1 and 65535".to_string());
    }

    if db
        .server_get_by_name(&name)
        .map_err(|e| format!("Failed to check existing servers: {e}"))?
        .is_some()
    {
        return Err(format!("A server named '{name}' already exists"));
    }

    let group_id = if spec.group_id.as_deref().is_some_and(|id| !id.is_empty()) {
        spec.group_id
    } else if let Some(group_name) = spec
        .group_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(resolve_or_create_group(db, group_name)?)
    } else {
        None
    };

    let jump_host_id = if spec
        .jump_host_id
        .as_deref()
        .is_some_and(|id| !id.is_empty())
    {
        spec.jump_host_id
    } else if let Some(jump_name) = spec
        .jump_host
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(resolve_server_id(db, jump_name)?)
    } else {
        None
    };

    let connection_kind = parse_connection_kind(spec.connection_kind.as_deref());
    let teleport_proxy = spec
        .teleport_proxy
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if connection_kind == ConnectionKind::Teleport {
        if teleport_proxy.is_none() {
            return Err(
                "Teleport servers require --proxy (for example teleport.example.com:443)"
                    .to_string(),
            );
        }
    }

    let mut new_server = Server {
        id: String::new(),
        name: name.clone(),
        host,
        port: spec.port,
        username,
        auth_type: string_to_auth_type(&spec.auth_type),
        credential_id: None,
        group_id,
        tags: spec.tags,
        created_at: 0,
        updated_at: 0,
        jump_host_id,
        post_login_command: spec
            .post_login_command
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        agent_forwarding: spec.agent_forwarding,
        connection_kind,
        teleport_proxy,
    };

    db.server_add(&mut new_server)
        .map_err(|e| format!("Failed to add server: {e}"))?;

    if new_server.is_teleport() {
        return Ok(new_server);
    }

    if let Some(credential) = spec
        .credential
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let auth_type = match new_server.auth_type {
            AuthType::Password => "password",
            AuthType::Key | AuthType::KeyWithPassphrase => "key_with_passphrase",
        };
        let credential_id = db
            .credential_save(
                &new_server.name,
                auth_type,
                credential,
                spec.passphrase
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                spec.key_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
            )
            .map_err(|e| format!("Failed to save credentials: {e}"))?;
        new_server.credential_id = Some(credential_id);
        db.server_update(&new_server)
            .map_err(|e| format!("Failed to attach credentials: {e}"))?;
    }

    Ok(new_server)
}

fn resolve_or_create_group(db: &Database, name: &str) -> Result<String, String> {
    if let Some(existing) = db
        .group_list()
        .map_err(|e| format!("Failed to list groups: {e}"))?
        .into_iter()
        .find(|group| group.name == name)
    {
        return Ok(existing.id);
    }

    let mut group = Group {
        id: String::new(),
        name: name.to_string(),
        parent_id: None,
        color: "#808080".to_string(),
    };
    db.group_add(&mut group)
        .map_err(|e| format!("Failed to create group '{name}': {e}"))?;
    Ok(group.id)
}

fn resolve_server_id(db: &Database, name_or_id: &str) -> Result<String, String> {
    if let Some(server) = db
        .server_get(name_or_id)
        .map_err(|e| format!("Failed to look up server: {e}"))?
    {
        return Ok(server.id);
    }
    db.server_get_by_name(name_or_id)
        .map_err(|e| format!("Failed to look up server '{name_or_id}': {e}"))?
        .map(|server| server.id)
        .ok_or_else(|| format!("Server '{name_or_id}' not found"))
}

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
    #[serde(default)]
    #[serde(alias = "connectionKind")]
    pub connection_kind: Option<String>,
    #[serde(default)]
    #[serde(alias = "teleportProxy")]
    pub teleport_proxy: Option<String>,
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

fn parse_connection_kind(value: Option<&str>) -> ConnectionKind {
    match value.map(str::trim).unwrap_or("ssh") {
        "teleport" | "tsh" => ConnectionKind::Teleport,
        _ => ConnectionKind::Ssh,
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
    add_server_spec(
        db.inner(),
        AddServerSpec {
            name: server.name,
            host: server.host,
            port: server.port,
            username: server.username,
            auth_type: server.auth_type,
            group_id: server.group_id,
            group_name: None,
            tags: server.tags,
            jump_host_id: server.jump_host_id,
            jump_host: None,
            post_login_command: server.post_login_command,
            agent_forwarding: server.agent_forwarding,
            connection_kind: server.connection_kind,
            teleport_proxy: server.teleport_proxy,
            credential: None,
            passphrase: None,
            key_path: None,
        },
    )
}

/// Partial server update input — all fields optional except name/host/port/username
#[derive(Debug, Deserialize)]
pub struct ServerUpdateInput {
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
    #[serde(default)]
    #[serde(alias = "connectionKind")]
    pub connection_kind: Option<String>,
    #[serde(
        default,
        alias = "teleportProxy",
        deserialize_with = "deserialize_present_option"
    )]
    pub teleport_proxy: Option<Option<String>>,
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
/// - Switching auth method drops the saved credentials so stale secrets are
///   never silently reused for the new auth type.
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

    let previous_name = existing.name.clone();
    let previous_auth_type = existing.auth_type.clone();

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
        connection_kind: updates
            .connection_kind
            .map(|kind| parse_connection_kind(Some(&kind)))
            .unwrap_or(existing.connection_kind),
        teleport_proxy: updates.teleport_proxy.unwrap_or(existing.teleport_proxy),
    };

    db.server_update(&updated_server)
        .map_err(|e| format!("Failed to update server: {}", e))?;

    // Keep name-keyed credentials attached to the renamed server.
    if updated_server.name != previous_name {
        if let Err(e) = db.credential_rename_server(&previous_name, &updated_server.name) {
            log::warn!(
                "Failed to migrate credentials from '{}' to '{}': {}",
                previous_name,
                updated_server.name,
                e
            );
        }
    }

    // Auth method changed: stored secret no longer matches, drop it so the
    // next connect prompts for fresh credentials instead of failing silently.
    if updated_server.auth_type != previous_auth_type {
        if let Err(e) = db.credential_delete(&updated_server.name) {
            log::warn!(
                "Failed to clear stale credentials for '{}': {}",
                updated_server.name,
                e
            );
        }
    }

    Ok(())
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

/// Delete credentials for a server
#[tauri::command]
pub fn delete_credential(
    db: State<'_, Arc<Database>>,
    request: CredentialServerInput,
) -> Result<(), String> {
    db.credential_delete(&request.server_name)
        .map_err(|e| format!("Failed to delete credentials: {}", e))
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

    #[test]
    fn add_server_spec_saves_optional_credentials_and_jump_host() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::storage::Database::new_at(dir.path().join("vibeshell.db")).unwrap();

        let jump = super::add_server_spec(
            &db,
            super::AddServerSpec {
                name: "bastion".to_string(),
                host: "bastion.example.com".to_string(),
                port: 22,
                username: "jump".to_string(),
                auth_type: "password".to_string(),
                group_id: None,
                group_name: None,
                tags: vec![],
                jump_host_id: None,
                jump_host: None,
                post_login_command: None,
                agent_forwarding: false,
                connection_kind: None,
                teleport_proxy: None,
                credential: Some("jump-secret".to_string()),
                passphrase: None,
                key_path: None,
            },
        )
        .unwrap();

        let added = super::add_server_spec(
            &db,
            super::AddServerSpec {
                name: "prod-web".to_string(),
                host: "prod.example.com".to_string(),
                port: 2222,
                username: "root".to_string(),
                auth_type: "key".to_string(),
                group_id: None,
                group_name: Some("production".to_string()),
                tags: vec!["web".to_string()],
                jump_host_id: None,
                jump_host: Some("bastion".to_string()),
                post_login_command: Some("uptime".to_string()),
                agent_forwarding: true,
                connection_kind: None,
                teleport_proxy: None,
                credential: Some(
                    "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----"
                        .to_string(),
                ),
                passphrase: Some("phrase".to_string()),
                key_path: Some("/tmp/id_ed25519".to_string()),
            },
        )
        .unwrap();

        assert_eq!(added.host, "prod.example.com");
        assert_eq!(added.port, 2222);
        assert_eq!(added.jump_host_id.as_deref(), Some(jump.id.as_str()));
        assert!(added.agent_forwarding);
        assert_eq!(added.post_login_command.as_deref(), Some("uptime"));
        assert_eq!(added.tags, vec!["web".to_string()]);
        assert!(added.credential_id.is_some());
        assert!(added.group_id.is_some());

        let cred = db.credential_get("prod-web").unwrap().unwrap();
        assert_eq!(cred.auth_type, "key_with_passphrase");
        assert_eq!(cred.passphrase.as_deref(), Some("phrase"));
        assert_eq!(cred.key_path.as_deref(), Some("/tmp/id_ed25519"));

        let duplicate = super::add_server_spec(
            &db,
            super::AddServerSpec {
                name: "prod-web".to_string(),
                host: "other.example.com".to_string(),
                port: 22,
                username: "root".to_string(),
                auth_type: "password".to_string(),
                group_id: None,
                group_name: None,
                tags: vec![],
                jump_host_id: None,
                jump_host: None,
                post_login_command: None,
                agent_forwarding: false,
                connection_kind: None,
                teleport_proxy: None,
                credential: None,
                passphrase: None,
                key_path: None,
            },
        );
        assert!(duplicate.unwrap_err().contains("already exists"));
    }
}
