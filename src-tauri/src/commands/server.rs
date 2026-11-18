use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::storage::database::Group;
use crate::storage::{AuthType, ConnectionKind, Database, Server};

/// Shared add-server payload used by the GUI command and the CLI IPC path.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct AddServerSpec {
    #[serde(default)]
    pub connection_kind: Option<String>,
    #[serde(default)]
    pub teleport_proxy: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    #[serde(default)]
    pub credential_id: Option<String>,
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
    pub credential: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
}

// This payload can contain passwords, private keys, passphrases and commands.
// Make even accidental Debug logging safe, not just the normal IPC log path.
impl std::fmt::Debug for AddServerSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AddServerSpec { payload: [REDACTED] }")
    }
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
    if !matches!(
        spec.auth_type.as_str(),
        "password" | "key" | "key_with_passphrase"
    ) {
        return Err("Unknown authentication type".into());
    }
    if host.starts_with('-')
        || host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "@/[]".contains(c))
    {
        return Err("Invalid host".into());
    }
    if host.contains(':') && host.parse::<std::net::Ipv6Addr>().is_err() {
        return Err("Host must not include a port; use the separate port field".into());
    }
    if username.starts_with('-')
        || username
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '@')
    {
        return Err("Invalid username".into());
    }
    if spec.group_id.is_some() && spec.group_name.is_some() {
        return Err("Specify a group ID or a group name, not both".into());
    }
    if spec.jump_host_id.is_some() && spec.jump_host.is_some() {
        return Err("Specify a jump host ID or name, not both".into());
    }
    if spec.credential_id.is_some() && spec.credential.is_some() {
        return Err("Specify an existing credential or a new secret, not both".into());
    }
    if spec.credential.is_none() && (spec.passphrase.is_some() || spec.key_path.is_some()) {
        return Err("A password or private key is required with credential options".into());
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    if spec.credential.is_some() {
        return Err("Saving credentials on mobile requires Keychain or Keystore support".into());
    }

    if db
        .server_get_by_name(&name)
        .map_err(|e| format!("Failed to check existing servers: {e}"))?
        .is_some()
    {
        return Err(format!("A server named '{name}' already exists"));
    }

    if let Some(id) = &spec.group_id {
        if !db
            .group_list()
            .map_err(|e| e.to_string())?
            .iter()
            .any(|g| &g.id == id)
        {
            return Err("Group not found".into());
        }
    }

    let jump_host_id = if spec
        .jump_host_id
        .as_deref()
        .is_some_and(|id| !id.is_empty())
    {
        Some(resolve_server_id(
            db,
            spec.jump_host_id.as_deref().unwrap(),
        )?)
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

    let connection_kind = parse_connection_kind(spec.connection_kind.as_deref())?;
    if connection_kind == ConnectionKind::Teleport
        && (spec.credential.is_some()
            || spec.credential_id.is_some()
            || spec.passphrase.is_some()
            || spec.key_path.is_some())
    {
        return Err("Teleport uses tsh login; do not attach SSH passwords or private keys".into());
    }
    let mut new_server = Server {
        connection_kind,
        teleport_proxy: spec.teleport_proxy,
        id: String::new(),
        name: name.clone(),
        host,
        port: spec.port,
        username,
        auth_type: string_to_auth_type(&spec.auth_type),
        credential_id: spec.credential_id,
        group_id: spec.group_id,
        tags: spec.tags,
        created_at: 0,
        updated_at: 0,
        jump_host_id,
        post_login_command: spec.post_login_command.filter(|value| !value.is_empty()),
        agent_forwarding: spec.agent_forwarding,
    };
    validate_transport(&new_server)?;

    let credentials =
        spec.credential
            .map(|credential| crate::storage::database::CredentialUpdate {
                credential: Some(credential),
                passphrase: spec.passphrase,
                key_path: spec.key_path,
            });
    db.server_add_with_credentials(
        &mut new_server,
        spec.group_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        credentials.as_ref(),
    )
    .map_err(|e| format!("Failed to add server: {e}"))?;

    Ok(new_server)
}

pub(crate) fn resolve_server_id(db: &Database, name_or_id: &str) -> Result<String, String> {
    let matches: Vec<_> = db
        .server_list(None, None)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|s| s.id == name_or_id || s.name == name_or_id)
        .collect();
    match matches.as_slice() {
        [server] => Ok(server.id.clone()),
        [] => Err("Server not found".into()),
        _ => Err("Ambiguous server name or ID; rename the conflicting profile first".into()),
    }
}

/// Server input from frontend (without auto-generated fields)
/// Frontend sends snake_case field names (auth_type, credential_id, etc.)
#[derive(Debug, Deserialize)]
pub struct ServerInput {
    #[serde(default, alias = "connectionKind")]
    pub connection_kind: Option<String>,
    #[serde(default, alias = "teleportProxy")]
    pub teleport_proxy: Option<String>,
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
    add_server_spec(db.inner(), server.into())
}

impl From<ServerInput> for AddServerSpec {
    fn from(server: ServerInput) -> Self {
        Self {
            connection_kind: server.connection_kind,
            teleport_proxy: server.teleport_proxy,
            name: server.name,
            host: server.host,
            port: server.port,
            username: server.username,
            auth_type: server.auth_type,
            credential_id: server.credential_id,
            group_id: server.group_id,
            group_name: None,
            tags: server.tags,
            jump_host_id: server.jump_host_id,
            jump_host: None,
            post_login_command: server.post_login_command,
            agent_forwarding: server.agent_forwarding,
            credential: None,
            passphrase: None,
            key_path: None,
        }
    }
}

/// Partial server update input — all fields optional except name/host/port/username
#[derive(Deserialize)]
pub struct ServerUpdateInput {
    #[serde(default, alias = "connectionKind")]
    pub connection_kind: Option<String>,
    #[serde(
        default,
        alias = "teleportProxy",
        deserialize_with = "deserialize_present_option"
    )]
    pub teleport_proxy: Option<Option<String>>,
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

    if let Some(kind) = updates.connection_kind.as_deref() {
        if parse_connection_kind(Some(kind))? != existing.connection_kind {
            return Err("Create a new profile to change its transport; existing credentials and live sessions are preserved".into());
        }
    }
    if existing.is_teleport() && updates.credentials.is_some() {
        return Err("Teleport credentials are managed by tsh login".into());
    }
    let updated_server = Server {
        connection_kind: existing.connection_kind,
        teleport_proxy: updates.teleport_proxy.unwrap_or(existing.teleport_proxy),
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
    validate_transport(&updated_server)?;
    db.server_update_with_credentials(&updated_server, updates.credentials.as_ref())
        .map_err(|e| format!("Failed to update server: {}", e))
}

fn parse_connection_kind(value: Option<&str>) -> Result<ConnectionKind, String> {
    match value.unwrap_or("ssh") {
        "ssh" => Ok(ConnectionKind::Ssh),
        "teleport" => Ok(ConnectionKind::Teleport),
        _ => Err("Unknown connection type; expected ssh or teleport".into()),
    }
}

pub(crate) fn validate_transport(server: &Server) -> Result<(), String> {
    if server.is_teleport() {
        if server.jump_host_id.is_some()
            || server.agent_forwarding
            || server.port != 22
            || server.credential_id.is_some()
        {
            return Err("Teleport profiles do not accept raw SSH jump hosts, agent forwarding, ports or credentials".into());
        }
        crate::teleport::TeleportTarget::from_server(server).map_err(|e| e.to_string())?;
    } else if server.teleport_proxy.is_some() {
        return Err("A Teleport proxy requires connection type teleport".into());
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
    access: State<'_, Arc<super::session::SessionAccessState>>,
    id: String,
) -> Result<(), String> {
    // When the GUI is attached to a daemon, teardown must happen in the owner.
    if access.is_remote() {
        return match super::session::ipc_send(crate::ipc::IpcMessage::DeleteServer {
            name: id,
            confirmed: true,
        })
        .await?
        {
            crate::ipc::IpcMessage::Ok => Ok(()),
            crate::ipc::IpcMessage::Error { message } => Err(message),
            _ => Err("Update both the VibeShell GUI and service before deleting profiles".into()),
        };
    }
    db.server_get(&id)
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

    #[test]
    fn add_server_spec_saves_optional_credentials_and_jump_host() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::storage::Database::new_at(dir.path().join("vibeshell.db")).unwrap();

        let jump = super::add_server_spec(
            &db,
            super::AddServerSpec {
                name: "bastion".to_string(),
                connection_kind: None,
                teleport_proxy: None,
                host: "bastion.example.com".to_string(),
                port: 22,
                username: "jump".to_string(),
                auth_type: "password".to_string(),
                credential_id: None,
                group_id: None,
                group_name: None,
                tags: vec![],
                jump_host_id: None,
                jump_host: None,
                post_login_command: None,
                agent_forwarding: false,
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
                connection_kind: None,
                teleport_proxy: None,
                host: "prod.example.com".to_string(),
                port: 2222,
                username: "root".to_string(),
                auth_type: "key".to_string(),
                credential_id: None,
                group_id: None,
                group_name: Some("production".to_string()),
                tags: vec!["web".to_string()],
                jump_host_id: None,
                jump_host: Some("bastion".to_string()),
                post_login_command: Some("uptime".to_string()),
                agent_forwarding: true,
                credential: Some(include_str!("../../tests/fixtures/server-add-test-key").into()),
                passphrase: Some(" phrase ".to_string()),
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
        assert!(cred.passphrase.as_deref() == Some(" phrase "));
        assert!(cred.credential == include_str!("../../tests/fixtures/server-add-test-key"));
        assert_eq!(cred.key_path.as_deref(), Some("/tmp/id_ed25519"));

        let duplicate = super::add_server_spec(
            &db,
            super::AddServerSpec {
                name: "prod-web".to_string(),
                host: "other.example.com".to_string(),
                connection_kind: None,
                teleport_proxy: None,
                port: 22,
                username: "root".to_string(),
                auth_type: "password".to_string(),
                credential_id: None,
                group_id: None,
                group_name: None,
                tags: vec![],
                jump_host_id: None,
                jump_host: None,
                post_login_command: None,
                agent_forwarding: false,
                credential: None,
                passphrase: None,
                key_path: None,
            },
        );
        assert!(duplicate.unwrap_err().contains("already exists"));
    }

    #[test]
    fn gui_adapter_preserves_saved_credential_association() {
        let input: ServerInput = serde_json::from_value(json!({
            "name":"gui", "host":"host.invalid", "port":22, "username":"user",
            "auth_type":"password", "credential_id":"existing-credential"
        }))
        .unwrap();
        let spec = super::AddServerSpec::from(input);
        assert_eq!(spec.credential_id.as_deref(), Some("existing-credential"));
        let dir = tempfile::tempdir().unwrap();
        let db = crate::storage::Database::new_at(dir.path().join("gui.db")).unwrap();
        let server = super::add_server_spec(&db, spec).unwrap();
        assert_eq!(
            db.server_get(&server.id)
                .unwrap()
                .unwrap()
                .credential_id
                .as_deref(),
            Some("existing-credential")
        );
    }

    #[test]
    fn whitespace_passwords_are_preserved_and_unknown_auth_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::storage::Database::new_at(dir.path().join("password.db")).unwrap();
        for (i, secret) in [" padded password \n", "   "].into_iter().enumerate() {
            let name = format!("server-{i}");
            super::add_server_spec(
                &db,
                super::AddServerSpec {
                    name: name.clone(),
                    host: "host.invalid".into(),
                    port: 22,
                    username: "u".into(),
                    auth_type: "password".into(),
                    credential: Some(secret.into()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(db.credential_get(&name).unwrap().unwrap().credential == secret);
        }
        assert!(super::add_server_spec(
            &db,
            super::AddServerSpec {
                name: "bad".into(),
                host: "host.invalid".into(),
                port: 22,
                username: "u".into(),
                auth_type: "typo".into(),
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(db.server_list(None, None).unwrap().len(), 2);
    }

    #[test]
    fn name_id_collision_is_not_silently_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::storage::Database::new_at(dir.path().join("ambiguous.db")).unwrap();
        let spec = super::AddServerSpec {
            name: "one".into(),
            host: "host.invalid".into(),
            port: 22,
            username: "u".into(),
            auth_type: "password".into(),
            ..Default::default()
        };
        let first = super::add_server_spec(&db, spec.clone()).unwrap();
        super::add_server_spec(
            &db,
            super::AddServerSpec {
                name: first.id.clone(),
                ..spec
            },
        )
        .unwrap();
        assert!(super::resolve_server_id(&db, &first.id)
            .unwrap_err()
            .contains("Ambiguous"));
    }

    #[test]
    fn teleport_profiles_roundtrip_without_ssh_credentials_or_transport_downgrade() {
        use crate::storage::{ConnectionKind, Database};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("transport.db");
        let db = Database::new_at(path.clone()).unwrap();
        let spec = super::AddServerSpec {
            connection_kind: Some("teleport".into()),
            teleport_proxy: Some("proxy.example:443".into()),
            name: "teleport".into(),
            host: "node-uuid".into(),
            port: 22,
            username: "CaseSensitive".into(),
            auth_type: "password".into(),
            ..Default::default()
        };
        for invalid in [
            super::AddServerSpec {
                credential: Some("must-not-be-saved".into()),
                ..spec.clone()
            },
            super::AddServerSpec {
                agent_forwarding: true,
                ..spec.clone()
            },
            super::AddServerSpec {
                port: 2222,
                ..spec.clone()
            },
            super::AddServerSpec {
                teleport_proxy: None,
                ..spec.clone()
            },
            super::AddServerSpec {
                host: "env=production".into(),
                ..spec.clone()
            },
        ] {
            assert!(super::add_server_spec(&db, invalid).is_err());
            assert!(db.server_list(None, None).unwrap().is_empty());
            assert!(db.credential_get("teleport").unwrap().is_none());
        }
        let server = super::add_server_spec(&db, spec).unwrap();
        assert!(server.is_teleport());
        assert!(server.credential_id.is_none());
        let mut edited = server.clone();
        edited.name = "renamed".into();
        edited.teleport_proxy = Some("other-proxy:443".into());
        db.server_update(&edited).unwrap();
        let secret_update = crate::storage::database::CredentialUpdate {
            credential: Some("not-a-tsh-password".into()),
            passphrase: None,
            key_path: None,
        };
        assert!(db
            .server_update_with_credentials(&edited, Some(&secret_update))
            .is_err());
        let mut downgrade = edited.clone();
        downgrade.connection_kind = ConnectionKind::Ssh;
        downgrade.teleport_proxy = None;
        assert!(db.server_update(&downgrade).is_err());
        drop(db);
        let db = Database::new_at(path).unwrap();
        let restored = db.server_get(&server.id).unwrap().unwrap();
        assert!(restored.is_teleport());
        assert_eq!(restored.username, "CaseSensitive");
        assert_eq!(restored.teleport_proxy.as_deref(), Some("other-proxy:443"));
        assert!(db.credential_get("renamed").unwrap().is_none());
        let mut legacy = serde_json::to_value(&restored).unwrap();
        legacy.as_object_mut().unwrap().remove("connection_kind");
        legacy.as_object_mut().unwrap().remove("teleport_proxy");
        let legacy: crate::storage::Server = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.connection_kind, ConnectionKind::Ssh);
        assert_eq!(legacy.teleport_proxy, None);
    }
}
