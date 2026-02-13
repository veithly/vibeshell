use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub credential_id: Option<String>,
    pub group_id: Option<String>,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Jump host server ID for ProxyJump connections
    #[serde(default)]
    pub jump_host_id: Option<String>,
    /// Command to auto-execute after SSH login
    #[serde(default)]
    pub post_login_command: Option<String>,
    /// Whether to enable SSH agent forwarding
    #[serde(default)]
    pub agent_forwarding: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    Password,
    Key,
    KeyWithPassphrase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub id: String,
    pub credential_type: CredentialType,
    pub encrypted_data: Vec<u8>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialType {
    Password,
    PrivateKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    pub session_id: String,
    pub server_id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub file_path: String,
    pub sync_status: SyncStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    Local,
    Syncing,
    Synced,
}

// =============================================================================
// SSH Tunnel Models
// =============================================================================

/// Type of SSH tunnel
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TunnelType {
    /// Local port forwarding (ssh -L): listen locally, forward to remote
    Local,
    /// Remote port forwarding (ssh -R): listen on remote, forward to local
    Remote,
    /// Dynamic port forwarding (ssh -D): SOCKS5 proxy
    Dynamic,
}

/// Persistent tunnel configuration stored per-server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub id: String,
    pub server_id: String,
    pub tunnel_type: TunnelType,
    pub local_host: String,
    pub local_port: u16,
    /// Remote host (not used for Dynamic tunnels)
    pub remote_host: Option<String>,
    /// Remote port (not used for Dynamic tunnels)
    pub remote_port: Option<u16>,
    /// Whether to auto-start this tunnel when connecting
    pub auto_start: bool,
    /// Whether this config is enabled
    pub enabled: bool,
}

/// Runtime status of an active tunnel
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TunnelStatus {
    Starting,
    Active,
    Stopped,
    Error,
}

/// Runtime information about an active tunnel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelInfo {
    pub id: String,
    pub config: TunnelConfig,
    pub session_id: String,
    pub status: TunnelStatus,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub active_connections: u32,
    pub error_message: Option<String>,
}

// =============================================================================
// Command Snippet Models
// =============================================================================

/// A saved command snippet / template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSnippet {
    pub id: String,
    pub name: String,
    pub command: String,
    pub category: String,
    pub description: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
