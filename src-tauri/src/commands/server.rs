use std::sync::Arc;
use tauri::State;
use serde::Deserialize;

use crate::storage::{Database, Server, AuthType};
use crate::storage::database::Group;

/// Server input from frontend (without auto-generated fields)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInput {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub credential_id: Option<String>,
    pub group_id: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub jump_host_id: Option<String>,
    #[serde(default)]
    pub post_login_command: Option<String>,
    #[serde(default)]
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
        "key" => AuthType::Key,
        "key_with_passphrase" => AuthType::KeyWithPassphrase,
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

/// Update an existing server
#[tauri::command]
pub fn update_server(
    db: State<'_, Arc<Database>>,
    id: String,
    updates: ServerInput,
) -> Result<(), String> {
    let existing = db.server_get(&id)
        .map_err(|e| format!("Failed to get server: {}", e))?
        .ok_or_else(|| "Server not found".to_string())?;

    let updated_server = Server {
        id: existing.id,
        name: updates.name,
        host: updates.host,
        port: updates.port,
        username: updates.username,
        auth_type: string_to_auth_type(&updates.auth_type),
        credential_id: updates.credential_id,
        group_id: updates.group_id,
        tags: updates.tags,
        created_at: existing.created_at,
        updated_at: 0,
        jump_host_id: updates.jump_host_id,
        post_login_command: updates.post_login_command,
        agent_forwarding: updates.agent_forwarding,
    };

    db.server_update(&updated_server)
        .map_err(|e| format!("Failed to update server: {}", e))
}

/// Delete a server
#[tauri::command]
pub fn delete_server(db: State<'_, Arc<Database>>, id: String) -> Result<(), String> {
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
