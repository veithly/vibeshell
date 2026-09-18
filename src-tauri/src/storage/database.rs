use anyhow::Result;
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

use super::sync::{self, SyncEntityKind};
use crate::storage::models::{
    AuthType, CommandHistoryEntry, CommandSnippet, DatabaseConnection, PluginInstallation,
    Recording, Server, TunnelConfig, TunnelType,
};

pub struct Database {
    pub(super) conn: Mutex<Connection>,
}

fn row_to_database_connection(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatabaseConnection> {
    Ok(DatabaseConnection {
        id: row.get(0)?,
        name: row.get(1)?,
        engine: row.get(2)?,
        host: row.get(3)?,
        port: row.get::<_, i64>(4)? as u16,
        username: row.get(5)?,
        password_encrypted: row.get(6)?,
        default_database: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        last_connected_at: row.get(10)?,
    })
}

fn row_to_plugin_installation(row: &rusqlite::Row<'_>) -> rusqlite::Result<PluginInstallation> {
    Ok(PluginInstallation {
        plugin_id: row.get(0)?,
        version: row.get(1)?,
        manifest_json: row.get(2)?,
        source: row.get(3)?,
        enabled: row.get::<_, i32>(4)? != 0,
        granted_permissions_json: row.get(5)?,
        settings_json: row.get(6)?,
        installed_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

impl Database {
    pub fn new() -> Result<Self> {
        let db_path = Self::get_db_path()?;
        Self::new_at(db_path)
    }

    pub fn new_at(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db_path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn get_db_path() -> Result<PathBuf> {
        crate::platform::default_database_path()
    }

    fn init_schema(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();

        // SQLite does not enforce declared foreign keys unless enabled per connection.
        conn.pragma_update(None, "foreign_keys", "ON")?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                username TEXT NOT NULL,
                auth_type TEXT NOT NULL,
                credential_id TEXT,
                group_id TEXT,
                tags TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                parent_id TEXT,
                color TEXT NOT NULL DEFAULT '#808080'
            );

            CREATE TABLE IF NOT EXISTS credentials (
                id TEXT PRIMARY KEY,
                credential_type TEXT NOT NULL,
                encrypted_data BLOB NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS recordings (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                server_id TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                ended_at INTEGER,
                file_path TEXT NOT NULL,
                sync_status TEXT NOT NULL DEFAULT 'local'
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tunnel_configs (
                id TEXT PRIMARY KEY,
                server_id TEXT NOT NULL,
                tunnel_type TEXT NOT NULL,
                local_host TEXT NOT NULL DEFAULT '127.0.0.1',
                local_port INTEGER NOT NULL,
                remote_host TEXT,
                remote_port INTEGER,
                auto_start INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS command_snippets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                command TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS command_history (
                id TEXT PRIMARY KEY,
                server_id TEXT NOT NULL,
                command TEXT NOT NULL,
                is_favorite INTEGER NOT NULL DEFAULT 0,
                use_count INTEGER NOT NULL DEFAULT 1,
                last_used_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(server_id, command),
                FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_command_history_server_last_used
                ON command_history(server_id, last_used_at DESC);

            CREATE TABLE IF NOT EXISTS agent_activity (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                payload TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS plugin_installations (
                plugin_id TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                manifest_json TEXT NOT NULL,
                source TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 0,
                granted_permissions_json TEXT NOT NULL DEFAULT '[]',
                settings_json TEXT NOT NULL DEFAULT '{}',
                installed_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS database_connections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                engine TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                username TEXT NOT NULL DEFAULT '',
                password_encrypted TEXT NOT NULL DEFAULT '',
                default_database TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_connected_at INTEGER
            );
        "#,
        )?;

        // Run migrations for new columns on existing tables.
        // ALTER TABLE ADD COLUMN will error if column already exists, so we ignore errors.
        let migrations = [
            "ALTER TABLE servers ADD COLUMN jump_host_id TEXT",
            "ALTER TABLE servers ADD COLUMN post_login_command TEXT",
            "ALTER TABLE servers ADD COLUMN agent_forwarding INTEGER NOT NULL DEFAULT 0",
        ];
        for sql in &migrations {
            let _ = conn.execute(sql, []);
        }

        sync::initialize(&mut conn)?;

        migrate_plaintext_credentials(&conn)?;

        Ok(())
    }

    // === Server Operations ===

    /// List all servers, optionally filtered by group_id and/or tags
    pub fn server_list(
        &self,
        group_id: Option<&str>,
        tags: Option<&[String]>,
    ) -> Result<Vec<Server>> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from("SELECT * FROM servers WHERE 1=1");
        if group_id.is_some() {
            sql.push_str(" AND group_id = ?1");
        }

        let mut stmt = conn.prepare(&sql)?;

        let servers: Vec<Server> = if let Some(gid) = group_id {
            stmt.query_map([gid], |row| self.row_to_server(row))?
                .filter_map(|s| s.ok())
                .collect()
        } else {
            stmt.query_map([], |row| self.row_to_server(row))?
                .filter_map(|s| s.ok())
                .collect()
        };

        // Filter by tags if provided
        let result = if let Some(tag_filter) = tags {
            servers
                .into_iter()
                .filter(|s| tag_filter.iter().any(|t| s.tags.contains(t)))
                .collect()
        } else {
            servers
        };

        Ok(result)
    }

    /// Get a server by its ID
    pub fn server_get(&self, id: &str) -> Result<Option<Server>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM servers WHERE id = ?1")?;

        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_server(row)?))
        } else {
            Ok(None)
        }
    }

    /// Get a server by its name
    pub fn server_get_by_name(&self, name: &str) -> Result<Option<Server>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM servers WHERE name = ?1")?;

        let mut rows = stmt.query([name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_server(row)?))
        } else {
            Ok(None)
        }
    }

    /// Add a new server. Generates UUID and sets timestamps automatically.
    pub fn server_add(&self, server: &mut Server) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();

        // Generate UUID if not provided
        if server.id.is_empty() {
            server.id = Uuid::new_v4().to_string();
        }

        // Set timestamps
        let now = Utc::now().timestamp();
        server.created_at = now;
        server.updated_at = now;

        // Serialize tags to JSON
        let tags_json = serde_json::to_string(&server.tags)?;
        let auth_type_str = auth_type_to_string(&server.auth_type);

        let tx = conn.transaction()?;
        tx.execute(
            r#"INSERT INTO servers
               (id, name, host, port, username, auth_type, credential_id, group_id, tags, created_at, updated_at,
                jump_host_id, post_login_command, agent_forwarding)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
            rusqlite::params![
                server.id,
                server.name,
                server.host,
                server.port,
                server.username,
                auth_type_str,
                server.credential_id,
                server.group_id,
                tags_json,
                server.created_at,
                server.updated_at,
                server.jump_host_id,
                server.post_login_command,
                server.agent_forwarding as i32,
            ],
        )?;

        sync::record_server_upsert(&tx, server, server.created_at, server.updated_at)?;
        tx.commit()?;

        Ok(())
    }

    /// Update an existing server. Updates the updated_at timestamp automatically.
    pub fn server_update(&self, server: &Server) -> Result<()> {
        self.server_update_with_credentials(server, None)
    }

    /// Metadata, credential rename and optional secret edits commit together.
    /// Omitted fields preserve the original secret; an empty passphrase clears it.
    pub fn server_update_with_credentials(
        &self,
        server: &Server,
        update: Option<&CredentialUpdate>,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();

        let now = Utc::now().timestamp();
        let tags_json = serde_json::to_string(&server.tags)?;
        let auth_type_str = auth_type_to_string(&server.auth_type);

        let tx = conn.transaction()?;
        let (old_name, old_auth): (String, String) = tx.query_row(
            "SELECT name, auth_type FROM servers WHERE id = ?1",
            [&server.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let same_auth_family = (old_auth == "password") == (auth_type_str == "password");
        if !same_auth_family && update.is_none() {
            anyhow::bail!("Changing authentication requires a new password or private key; existing credentials were not changed");
        }
        // UNIQUE conflicts abort the entire transaction instead of deleting a
        // credential that happened to have the destination name.
        if old_name != server.name {
            tx.execute(
                "UPDATE server_credentials SET server_name = ?2 WHERE server_name = ?1",
                rusqlite::params![old_name, server.name],
            )?;
        }
        if let Some(update) = update {
            use rusqlite::OptionalExtension;
            let previous: Option<(String, Option<String>, Option<String>)> = tx.query_row(
                "SELECT credential, passphrase, key_path FROM server_credentials WHERE server_name = ?1",
                [&server.name], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ).optional()?;
            let (old_secret, old_phrase, old_path) = if same_auth_family {
                previous.unwrap_or_default()
            } else {
                (String::new(), None, None)
            };
            let secret = match &update.credential {
                Some(value) => value.clone(),
                None => decrypt_credential_value(&old_secret)?,
            };
            let phrase = match &update.passphrase {
                Some(value) => Some(value.clone()),
                None => old_phrase
                    .map(|value| decrypt_credential_value(&value))
                    .transpose()?,
            }
            .filter(|value| !value.is_empty());
            let key_path = update
                .key_path
                .clone()
                .or(old_path)
                .filter(|value| !value.is_empty());
            if auth_type_str == "password" && secret.is_empty() {
                anyhow::bail!(
                    "A non-empty password is required; existing credentials were not changed"
                );
            }
            if auth_type_str != "password" {
                if secret.trim().is_empty() && key_path.is_none() {
                    anyhow::bail!("A private key or key path is required");
                }
                if !secret.trim().is_empty() {
                    russh::keys::decode_secret_key(&secret, phrase.as_deref())
                        .map_err(|_| anyhow::anyhow!("Private key or passphrase is invalid; existing credentials were not changed"))?;
                }
            }
            let encrypted = encrypt_credential_value(&secret)?;
            let encrypted_phrase = if auth_type_str == "password" {
                None
            } else {
                phrase
                    .as_deref()
                    .map(encrypt_credential_value)
                    .transpose()?
            };
            let key_path = if auth_type_str == "password" {
                None
            } else {
                key_path
            };
            tx.execute(
                "INSERT INTO server_credentials (id, server_name, auth_type, credential, passphrase, key_path, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(server_name) DO UPDATE SET auth_type=excluded.auth_type,
                 credential=excluded.credential, passphrase=excluded.passphrase, key_path=excluded.key_path",
                rusqlite::params![Uuid::new_v4().to_string(), server.name, auth_type_str, encrypted, encrypted_phrase, key_path, now],
            )?;
        }
        let changed = tx.execute(
            r#"UPDATE servers SET
               name = ?2, host = ?3, port = ?4, username = ?5, auth_type = ?6,
               credential_id = ?7, group_id = ?8, tags = ?9, updated_at = ?10,
               jump_host_id = ?11, post_login_command = ?12, agent_forwarding = ?13
               WHERE id = ?1"#,
            rusqlite::params![
                server.id,
                server.name,
                server.host,
                server.port,
                server.username,
                auth_type_str,
                server.credential_id,
                server.group_id,
                tags_json,
                now,
                server.jump_host_id,
                server.post_login_command,
                server.agent_forwarding as i32,
            ],
        )?;

        if changed > 0 {
            sync::record_server_upsert(&tx, server, server.created_at, now)?;
        }
        tx.commit()?;

        Ok(())
    }

    /// Delete a server by its ID
    pub fn server_delete(&self, id: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        sync::detach_server_references(&tx, id, true)?;
        tx.execute("DELETE FROM servers WHERE id = ?1", [id])?;
        sync::record_local_delete(&tx, SyncEntityKind::Server, id)?;
        tx.commit()?;
        Ok(())
    }

    /// Convert a database row to a Server struct
    fn row_to_server(&self, row: &rusqlite::Row) -> rusqlite::Result<Server> {
        let tags_json: String = row.get(8)?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

        let auth_type_str: String = row.get(5)?;
        let auth_type = string_to_auth_type(&auth_type_str);

        // New columns may not exist in older databases, use unwrap_or_default
        let agent_forwarding_int: i32 = row.get(13).unwrap_or(0);

        Ok(Server {
            id: row.get(0)?,
            name: row.get(1)?,
            host: row.get(2)?,
            port: row.get(3)?,
            username: row.get(4)?,
            auth_type,
            credential_id: row.get(6)?,
            group_id: row.get(7)?,
            tags,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
            jump_host_id: row.get(11).unwrap_or(None),
            post_login_command: row.get(12).unwrap_or(None),
            agent_forwarding: agent_forwarding_int != 0,
        })
    }
}

/// Convert AuthType enum to string for database storage
fn auth_type_to_string(auth_type: &AuthType) -> &'static str {
    match auth_type {
        AuthType::Password => "password",
        AuthType::Key => "key",
        AuthType::KeyWithPassphrase => "key_with_passphrase",
    }
}

/// Convert string from database to AuthType enum
fn string_to_auth_type(s: &str) -> AuthType {
    match s {
        "password" => AuthType::Password,
        // Legacy standalone "key" rows are folded into the key+passphrase flow
        // (an empty passphrase means an unencrypted key).
        "key" | "key_with_passphrase" => AuthType::KeyWithPassphrase,
        _ => AuthType::Password, // Default fallback
    }
}

/// Group model
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub color: String,
}

impl Database {
    // === Group Operations ===

    /// List all groups
    pub fn group_list(&self) -> Result<Vec<Group>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, parent_id, color FROM groups")?;

        let groups = stmt
            .query_map([], |row| {
                Ok(Group {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    parent_id: row.get(2)?,
                    color: row.get(3)?,
                })
            })?
            .filter_map(|g| g.ok())
            .collect();

        Ok(groups)
    }

    /// Add a new group
    pub fn group_add(&self, group: &mut Group) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();

        if group.id.is_empty() {
            group.id = Uuid::new_v4().to_string();
        }

        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO groups (id, name, parent_id, color) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![group.id, group.name, group.parent_id, group.color],
        )?;

        sync::record_group_upsert(&tx, group)?;
        tx.commit()?;

        Ok(())
    }

    /// Delete a group
    pub fn group_delete(&self, id: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        sync::detach_group_references(&tx, id, true)?;
        tx.execute("DELETE FROM groups WHERE id = ?1", [id])?;
        sync::record_local_delete(&tx, SyncEntityKind::Group, id)?;
        tx.commit()?;
        Ok(())
    }
}

/// Credential model for storing server credentials
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Credential {
    pub id: String,
    pub server_name: String,
    pub auth_type: String,
    pub credential: String,
    pub passphrase: Option<String>,
    pub key_path: Option<String>,
    pub created_at: i64,
}

/// Version marker prefixing credential values encrypted with the device key
/// (see `storage::crypto::Crypto::device`). Any stored value without this
/// prefix is treated as legacy plaintext.
const CREDENTIAL_ENC_PREFIX: &str = "enc:v1:";

/// Encrypt a credential field for storage. Empty values are stored as-is so
/// `None`/empty passphrase semantics are preserved.
fn encrypt_credential_value(value: &str) -> Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    let crypto = crate::storage::crypto::Crypto::device()?;
    Ok(format!(
        "{}{}",
        CREDENTIAL_ENC_PREFIX,
        crypto.encrypt_base64(value.as_bytes())?
    ))
}

/// Decrypt a stored credential field. Legacy plaintext values (no version
/// marker) pass through unchanged. Decryption failure is an error, never a
/// panic.
fn decrypt_credential_value(stored: &str) -> Result<String> {
    let Some(encoded) = stored.strip_prefix(CREDENTIAL_ENC_PREFIX) else {
        return Ok(stored.to_string());
    };
    let crypto = crate::storage::crypto::Crypto::device()?;
    let plaintext = crypto.decrypt_base64(encoded).map_err(|error| {
        log::error!("Failed to decrypt a stored credential: {}", error);
        anyhow::anyhow!("Stored credential could not be decrypted on this device")
    })?;
    String::from_utf8(plaintext)
        .map_err(|_| anyhow::anyhow!("Stored credential is not valid UTF-8"))
}

/// One-time upgrade pass: encrypt any `server_credentials` rows still stored
/// as plaintext. Idempotent — encrypted values carry `CREDENTIAL_ENC_PREFIX`.
/// Best-effort: if the device key cannot be used, plaintext rows are left in
/// place (they remain readable) rather than blocking database startup.
fn migrate_plaintext_credentials(conn: &Connection) -> Result<()> {
    conn.execute(
        r#"CREATE TABLE IF NOT EXISTS server_credentials (
                id TEXT PRIMARY KEY,
                server_name TEXT NOT NULL UNIQUE,
                auth_type TEXT NOT NULL,
                credential TEXT NOT NULL,
                passphrase TEXT,
                key_path TEXT,
                created_at INTEGER NOT NULL
            )"#,
        [],
    )?;

    let mut stmt = conn.prepare("SELECT id, credential, passphrase FROM server_credentials")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    let pending: Vec<(String, String, Option<String>)> = rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, credential, passphrase)| {
            let cred_plain =
                !credential.is_empty() && !credential.starts_with(CREDENTIAL_ENC_PREFIX);
            let pass_plain = passphrase
                .as_deref()
                .is_some_and(|p| !p.is_empty() && !p.starts_with(CREDENTIAL_ENC_PREFIX));
            cred_plain || pass_plain
        })
        .collect();
    drop(stmt);

    if pending.is_empty() {
        return Ok(());
    }

    if let Err(error) = crate::storage::crypto::Crypto::device() {
        log::error!(
            "[Storage] Skipping credential encryption migration (device key unavailable): {}",
            error
        );
        return Ok(());
    }

    let mut migrated = 0usize;
    for (id, credential, passphrase) in pending {
        // A legacy row can mix an encrypted key and a plaintext passphrase
        // (or vice versa). Never wrap existing ciphertext a second time.
        let migrate_field = |value: &str| {
            if value.starts_with(CREDENTIAL_ENC_PREFIX) {
                Ok(value.to_string())
            } else {
                encrypt_credential_value(value)
            }
        };
        let encrypted: Result<(String, Option<String>)> = (|| {
            Ok((
                migrate_field(&credential)?,
                passphrase.as_deref().map(migrate_field).transpose()?,
            ))
        })();
        match encrypted {
            Ok((enc_credential, enc_passphrase)) => {
                // Another GUI/daemon may have saved a new credential since
                // the read. Only migrate the exact row we inspected.
                migrated += conn.execute(
                    "UPDATE server_credentials SET credential = ?2, passphrase = ?3 WHERE id = ?1 AND credential = ?4 AND passphrase IS ?5",
                    rusqlite::params![id, enc_credential, enc_passphrase, credential, passphrase],
                )?;
            }
            Err(error) => {
                log::warn!(
                    "[Storage] Could not encrypt stored credential row: {}",
                    error
                );
            }
        }
    }
    if migrated > 0 {
        log::info!("[Storage] Encrypted {} stored credential row(s)", migrated);
    }
    Ok(())
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredAgentActivity {
    pub sequence: i64,
    #[serde(flatten)]
    pub event: crate::mcp::server::AgentActivityEvent,
}

/// Explicit credential patch. Never place secrets in server-list responses or logs.
#[derive(Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialUpdate {
    pub credential: Option<String>,
    pub passphrase: Option<String>,
    pub key_path: Option<String>,
}

impl Database {
    /// Append a durable lifecycle event. Command text is encrypted at rest and
    /// is not part of cloud sync or diagnostic logs. Every run keeps its own id.
    pub fn agent_activity_record(
        &self,
        event: &crate::mcp::server::AgentActivityEvent,
    ) -> Result<()> {
        if event.summary.len() > 256 * 1024 {
            anyhow::bail!("Agent command exceeds the 256 KiB audit limit");
        }
        let payload = encrypt_credential_value(&serde_json::to_string(event)?)?;
        self.conn.lock().unwrap().execute(
            "INSERT INTO agent_activity (payload) VALUES (?1)",
            [payload],
        )?;
        Ok(())
    }

    pub fn agent_activity_list(
        &self,
        after: Option<i64>,
        before: Option<i64>,
        limit: u32,
    ) -> Result<Vec<StoredAgentActivity>> {
        let conn = self.conn.lock().unwrap();
        // Incremental reads are ascending so a busy agent cannot skip events
        // when one page fills; history pages read newest first.
        let order = if after.is_some() { "ASC" } else { "DESC" };
        let mut query = conn.prepare(&format!(
            "SELECT sequence, payload FROM agent_activity
             WHERE (?1 IS NULL OR sequence > ?1) AND (?2 IS NULL OR sequence < ?2)
             ORDER BY sequence {order} LIMIT ?3"
        ))?;
        let rows = query
            .query_map(
                rusqlite::params![after, before, limit.clamp(1, 500)],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(sequence, payload)| {
                Ok(StoredAgentActivity {
                    sequence,
                    event: serde_json::from_str(&decrypt_credential_value(&payload)?)?,
                })
            })
            .collect()
    }

    // === Credential Operations ===

    /// Save credentials for a server (creates or updates)
    pub fn credential_save(
        &self,
        server_name: &str,
        auth_type: &str,
        credential: &str,
        passphrase: Option<&str>,
        key_path: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();

        // Check if credential already exists for this server
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM server_credentials WHERE server_name = ?1",
                [server_name],
                |row| row.get(0),
            )
            .ok();

        // Create server_credentials table if it doesn't exist
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS server_credentials (
                id TEXT PRIMARY KEY,
                server_name TEXT NOT NULL UNIQUE,
                auth_type TEXT NOT NULL,
                credential TEXT NOT NULL,
                passphrase TEXT,
                key_path TEXT,
                created_at INTEGER NOT NULL
            )"#,
            [],
        )?;

        // Secrets are encrypted at rest with the device-local key; the
        // `enc:v1:` prefix keeps ciphertext distinguishable from legacy
        // plaintext rows.
        let stored_credential = encrypt_credential_value(credential)?;
        let stored_passphrase = passphrase.map(encrypt_credential_value).transpose()?;

        if let Some(id) = existing {
            // Update existing
            conn.execute(
                r#"UPDATE server_credentials SET
                   auth_type = ?2, credential = ?3, passphrase = ?4, key_path = ?5
                   WHERE id = ?1"#,
                rusqlite::params![
                    id,
                    auth_type,
                    stored_credential,
                    stored_passphrase,
                    key_path
                ],
            )?;
            Ok(id)
        } else {
            // Insert new
            let id = Uuid::new_v4().to_string();
            conn.execute(
                r#"INSERT INTO server_credentials (id, server_name, auth_type, credential, passphrase, key_path, created_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
                rusqlite::params![id, server_name, auth_type, stored_credential, stored_passphrase, key_path, now],
            )?;
            Ok(id)
        }
    }

    /// Get credentials for a server by name
    pub fn credential_get(&self, server_name: &str) -> Result<Option<Credential>> {
        let conn = self.conn.lock().unwrap();

        // Ensure table exists
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS server_credentials (
                id TEXT PRIMARY KEY,
                server_name TEXT NOT NULL UNIQUE,
                auth_type TEXT NOT NULL,
                credential TEXT NOT NULL,
                passphrase TEXT,
                key_path TEXT,
                created_at INTEGER NOT NULL
            )"#,
            [],
        )?;

        let result = conn.query_row(
            "SELECT id, server_name, auth_type, credential, passphrase, key_path, created_at FROM server_credentials WHERE server_name = ?1",
            [server_name],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        );

        match result {
            Ok((id, server_name, auth_type, credential, passphrase, key_path, created_at)) => {
                // Decryption failure is surfaced as an error, never a panic.
                // Legacy plaintext values pass through unchanged.
                let credential = decrypt_credential_value(&credential)?;
                let passphrase = passphrase
                    .map(|value| decrypt_credential_value(&value))
                    .transpose()?;
                Ok(Some(Credential {
                    id,
                    server_name,
                    auth_type,
                    credential,
                    passphrase,
                    key_path,
                    created_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete credentials for a server
    pub fn credential_delete(&self, server_name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Ensure table exists (it is created lazily by credential_save/get)
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS server_credentials (
                id TEXT PRIMARY KEY,
                server_name TEXT NOT NULL UNIQUE,
                auth_type TEXT NOT NULL,
                credential TEXT NOT NULL,
                passphrase TEXT,
                key_path TEXT,
                created_at INTEGER NOT NULL
            )"#,
            [],
        )?;
        conn.execute(
            "DELETE FROM server_credentials WHERE server_name = ?1",
            [server_name],
        )?;
        Ok(())
    }

    /// Re-key saved credentials when a server is renamed so they stay attached
    /// to the server instead of becoming orphaned under the old name.
    pub fn credential_rename_server(&self, old_name: &str, new_name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Ensure table exists (it is created lazily by credential_save/get)
        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS server_credentials (
                id TEXT PRIMARY KEY,
                server_name TEXT NOT NULL UNIQUE,
                auth_type TEXT NOT NULL,
                credential TEXT NOT NULL,
                passphrase TEXT,
                key_path TEXT,
                created_at INTEGER NOT NULL
            )"#,
            [],
        )?;
        // Drop any credential already stored under the new name to satisfy the
        // UNIQUE(server_name) constraint before re-keying.
        conn.execute(
            "DELETE FROM server_credentials WHERE server_name = ?1",
            [new_name],
        )?;
        conn.execute(
            "UPDATE server_credentials SET server_name = ?2 WHERE server_name = ?1",
            rusqlite::params![old_name, new_name],
        )?;
        Ok(())
    }
}

// =============================================================================
// Tunnel Config Operations
// =============================================================================

fn tunnel_type_to_string(t: &TunnelType) -> &'static str {
    match t {
        TunnelType::Local => "local",
        TunnelType::Remote => "remote",
        TunnelType::Dynamic => "dynamic",
    }
}

fn string_to_tunnel_type(s: &str) -> TunnelType {
    match s {
        "local" => TunnelType::Local,
        "remote" => TunnelType::Remote,
        "dynamic" => TunnelType::Dynamic,
        _ => TunnelType::Local,
    }
}

impl Database {
    /// List tunnel configs for a server
    pub fn tunnel_config_list(&self, server_id: &str) -> Result<Vec<TunnelConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, server_id, tunnel_type, local_host, local_port, remote_host, remote_port, auto_start, enabled FROM tunnel_configs WHERE server_id = ?1"
        )?;

        let configs = stmt
            .query_map([server_id], |row| {
                let tt_str: String = row.get(2)?;
                let auto_start_int: i32 = row.get(7)?;
                let enabled_int: i32 = row.get(8)?;
                Ok(TunnelConfig {
                    id: row.get(0)?,
                    server_id: row.get(1)?,
                    tunnel_type: string_to_tunnel_type(&tt_str),
                    local_host: row.get(3)?,
                    local_port: row.get(4)?,
                    remote_host: row.get(5)?,
                    remote_port: row.get(6)?,
                    auto_start: auto_start_int != 0,
                    enabled: enabled_int != 0,
                })
            })?
            .filter_map(|c| c.ok())
            .collect();

        Ok(configs)
    }

    /// Add a tunnel config
    pub fn tunnel_config_add(&self, config: &mut TunnelConfig) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if config.id.is_empty() {
            config.id = Uuid::new_v4().to_string();
        }

        conn.execute(
            r#"INSERT INTO tunnel_configs (id, server_id, tunnel_type, local_host, local_port, remote_host, remote_port, auto_start, enabled)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            rusqlite::params![
                config.id,
                config.server_id,
                tunnel_type_to_string(&config.tunnel_type),
                config.local_host,
                config.local_port,
                config.remote_host,
                config.remote_port,
                config.auto_start as i32,
                config.enabled as i32,
            ],
        )?;
        Ok(())
    }

    /// Update a tunnel config
    pub fn tunnel_config_update(&self, config: &TunnelConfig) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"UPDATE tunnel_configs SET
               tunnel_type = ?2, local_host = ?3, local_port = ?4, remote_host = ?5, remote_port = ?6,
               auto_start = ?7, enabled = ?8
               WHERE id = ?1"#,
            rusqlite::params![
                config.id,
                tunnel_type_to_string(&config.tunnel_type),
                config.local_host,
                config.local_port,
                config.remote_host,
                config.remote_port,
                config.auto_start as i32,
                config.enabled as i32,
            ],
        )?;
        Ok(())
    }

    /// Delete a tunnel config
    pub fn tunnel_config_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tunnel_configs WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Delete all tunnel configs for a server
    pub fn tunnel_config_delete_for_server(&self, server_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM tunnel_configs WHERE server_id = ?1",
            [server_id],
        )?;
        Ok(())
    }
}

// =============================================================================
// Command Snippet Operations
// =============================================================================

impl Database {
    /// List all command snippets, optionally filtered by category
    pub fn snippet_list(&self, category: Option<&str>) -> Result<Vec<CommandSnippet>> {
        let conn = self.conn.lock().unwrap();

        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(cat) =
            category
        {
            (
                "SELECT id, name, command, category, description, tags, created_at, updated_at FROM command_snippets WHERE category = ?1 ORDER BY updated_at DESC".to_string(),
                vec![Box::new(cat.to_string())],
            )
        } else {
            (
                "SELECT id, name, command, category, description, tags, created_at, updated_at FROM command_snippets ORDER BY updated_at DESC".to_string(),
                vec![],
            )
        };

        let mut stmt = conn.prepare(&sql)?;
        let snippets = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                let tags_json: String = row.get(5)?;
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Ok(CommandSnippet {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    command: row.get(2)?,
                    category: row.get(3)?,
                    description: row.get(4)?,
                    tags,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })?
            .filter_map(|s| s.ok())
            .collect();

        Ok(snippets)
    }

    /// Add a command snippet
    pub fn snippet_add(&self, snippet: &mut CommandSnippet) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        if snippet.id.is_empty() {
            snippet.id = Uuid::new_v4().to_string();
        }
        let now = Utc::now().timestamp();
        snippet.created_at = now;
        snippet.updated_at = now;

        let tags_json = serde_json::to_string(&snippet.tags)?;

        let tx = conn.transaction()?;
        tx.execute(
            r#"INSERT INTO command_snippets (id, name, command, category, description, tags, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            rusqlite::params![
                snippet.id,
                snippet.name,
                snippet.command,
                snippet.category,
                snippet.description,
                tags_json,
                snippet.created_at,
                snippet.updated_at,
            ],
        )?;
        sync::record_snippet_upsert(&tx, snippet, snippet.created_at, snippet.updated_at)?;
        tx.commit()?;
        Ok(())
    }

    /// Update a command snippet
    pub fn snippet_update(&self, snippet: &CommandSnippet) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        let tags_json = serde_json::to_string(&snippet.tags)?;

        let tx = conn.transaction()?;
        let changed = tx.execute(
            r#"UPDATE command_snippets SET
               name = ?2, command = ?3, category = ?4, description = ?5, tags = ?6, updated_at = ?7
               WHERE id = ?1"#,
            rusqlite::params![
                snippet.id,
                snippet.name,
                snippet.command,
                snippet.category,
                snippet.description,
                tags_json,
                now,
            ],
        )?;
        if changed > 0 {
            sync::record_snippet_upsert(&tx, snippet, snippet.created_at, now)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Delete a command snippet
    pub fn snippet_delete(&self, id: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM command_snippets WHERE id = ?1", [id])?;
        sync::record_local_delete(&tx, SyncEntityKind::CommandSnippet, id)?;
        tx.commit()?;
        Ok(())
    }

    /// Search snippets by name, command, or tags
    pub fn snippet_search(&self, query: &str) -> Result<Vec<CommandSnippet>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query);

        let mut stmt = conn.prepare(
            r#"SELECT id, name, command, category, description, tags, created_at, updated_at
               FROM command_snippets
               WHERE name LIKE ?1 OR command LIKE ?1 OR description LIKE ?1 OR tags LIKE ?1
               ORDER BY updated_at DESC"#,
        )?;

        let snippets = stmt
            .query_map([&pattern], |row| {
                let tags_json: String = row.get(5)?;
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Ok(CommandSnippet {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    command: row.get(2)?,
                    category: row.get(3)?,
                    description: row.get(4)?,
                    tags,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })?
            .filter_map(|s| s.ok())
            .collect();

        Ok(snippets)
    }

    /// List command history for one server, optionally filtered by text and favorites.
    pub fn history_list(
        &self,
        server_id: &str,
        query: Option<&str>,
        favorites_only: bool,
        limit: u32,
    ) -> Result<Vec<CommandHistoryEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            "SELECT id, server_id, command, is_favorite, use_count, last_used_at, created_at \
             FROM command_history WHERE server_id = ?1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(server_id.to_string())];

        if favorites_only {
            sql.push_str(" AND is_favorite = 1");
        }
        if let Some(search) = query.filter(|value| !value.trim().is_empty()) {
            sql.push_str(" AND command LIKE ?2 ESCAPE '\\'");
            let escaped = search
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            params.push(Box::new(format!("%{}%", escaped)));
        }
        let limit_param = params.len() + 1;
        sql.push_str(&format!(
            " ORDER BY is_favorite DESC, last_used_at DESC LIMIT ?{}",
            limit_param
        ));
        params.push(Box::new(i64::from(limit.clamp(1, 500))));

        let mut stmt = conn.prepare(&sql)?;
        let entries = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                Ok(CommandHistoryEntry {
                    id: row.get(0)?,
                    server_id: row.get(1)?,
                    command: row.get(2)?,
                    is_favorite: row.get::<_, i32>(3)? != 0,
                    use_count: row.get(4)?,
                    last_used_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }

    /// Record a command execution, merging repeated commands for the same server.
    pub fn history_record(&self, server_id: &str, command: &str) -> Result<CommandHistoryEntry> {
        let command = command.trim();
        if command.is_empty() {
            anyhow::bail!("Cannot record an empty command");
        }

        let mut conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        let id = Uuid::new_v4().to_string();
        let tx = conn.transaction()?;
        tx.execute(
            r#"INSERT INTO command_history
               (id, server_id, command, is_favorite, use_count, last_used_at, created_at)
               VALUES (?1, ?2, ?3, 0, 1, ?4, ?4)
               ON CONFLICT(server_id, command) DO UPDATE SET
                 use_count = command_history.use_count + 1,
                 last_used_at = excluded.last_used_at"#,
            rusqlite::params![id, server_id, command, now],
        )?;
        let entry = tx.query_row(
            "SELECT id, server_id, command, is_favorite, use_count, last_used_at, created_at \
             FROM command_history WHERE server_id = ?1 AND command = ?2",
            rusqlite::params![server_id, command],
            |row| {
                Ok(CommandHistoryEntry {
                    id: row.get(0)?,
                    server_id: row.get(1)?,
                    command: row.get(2)?,
                    is_favorite: row.get::<_, i32>(3)? != 0,
                    use_count: row.get(4)?,
                    last_used_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )?;
        tx.commit()?;
        Ok(entry)
    }

    /// Mark or unmark a history entry as a favorite.
    pub fn history_set_favorite(&self, id: &str, is_favorite: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE command_history SET is_favorite = ?2 WHERE id = ?1",
            rusqlite::params![id, is_favorite as i32],
        )?;
        Ok(())
    }

    /// Delete a single history entry.
    pub fn history_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM command_history WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Clear history for one server. Favorites are retained by default.
    pub fn history_clear(&self, server_id: &str, include_favorites: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if include_favorites {
            conn.execute(
                "DELETE FROM command_history WHERE server_id = ?1",
                [server_id],
            )?;
        } else {
            conn.execute(
                "DELETE FROM command_history WHERE server_id = ?1 AND is_favorite = 0",
                [server_id],
            )?;
        }
        Ok(())
    }
}

// =============================================================================
// Recording Operations (table already exists from init_schema)
// =============================================================================

impl Database {
    /// List recordings, optionally filtered by server_id
    pub fn recording_list(&self, server_id: Option<&str>) -> Result<Vec<Recording>> {
        let conn = self.conn.lock().unwrap();
        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(sid) =
            server_id
        {
            (
                "SELECT id, session_id, server_id, started_at, ended_at, file_path, sync_status FROM recordings WHERE server_id = ?1 ORDER BY started_at DESC",
                vec![Box::new(sid.to_string())],
            )
        } else {
            (
                "SELECT id, session_id, server_id, started_at, ended_at, file_path, sync_status FROM recordings ORDER BY started_at DESC",
                vec![],
            )
        };

        let mut stmt = conn.prepare(sql)?;
        let recordings = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                let sync_str: String = row.get(6)?;
                let sync_status = match sync_str.as_str() {
                    "syncing" => crate::storage::models::SyncStatus::Syncing,
                    "synced" => crate::storage::models::SyncStatus::Synced,
                    _ => crate::storage::models::SyncStatus::Local,
                };
                Ok(Recording {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    server_id: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    file_path: row.get(5)?,
                    sync_status,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(recordings)
    }

    /// Add a recording
    pub fn recording_add(&self, recording: &mut Recording) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if recording.id.is_empty() {
            recording.id = Uuid::new_v4().to_string();
        }

        let sync_str = match &recording.sync_status {
            crate::storage::models::SyncStatus::Local => "local",
            crate::storage::models::SyncStatus::Syncing => "syncing",
            crate::storage::models::SyncStatus::Synced => "synced",
        };

        conn.execute(
            r#"INSERT INTO recordings (id, session_id, server_id, started_at, ended_at, file_path, sync_status)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            rusqlite::params![
                recording.id,
                recording.session_id,
                recording.server_id,
                recording.started_at,
                recording.ended_at,
                recording.file_path,
                sync_str,
            ],
        )?;
        Ok(())
    }

    /// Update recording (mainly to set ended_at)
    pub fn recording_update_ended(&self, id: &str, ended_at: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE recordings SET ended_at = ?2 WHERE id = ?1",
            rusqlite::params![id, ended_at],
        )?;
        Ok(())
    }

    /// Delete a recording
    pub fn recording_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM recordings WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Get a recording by ID
    pub fn recording_get(&self, id: &str) -> Result<Option<Recording>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT id, session_id, server_id, started_at, ended_at, file_path, sync_status FROM recordings WHERE id = ?1",
            [id],
            |row| {
                let sync_str: String = row.get(6)?;
                let sync_status = match sync_str.as_str() {
                    "syncing" => crate::storage::models::SyncStatus::Syncing,
                    "synced" => crate::storage::models::SyncStatus::Synced,
                    _ => crate::storage::models::SyncStatus::Local,
                };
                Ok(Recording {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    server_id: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    file_path: row.get(5)?,
                    sync_status,
                })
            },
        );

        match result {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // === Plugin Operations ===

    // -----------------------------------------------------------------------
    // Database connections
    // -----------------------------------------------------------------------

    pub fn database_connection_list(&self) -> Result<Vec<DatabaseConnection>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, engine, host, port, username, password_encrypted, \
             default_database, created_at, updated_at, last_connected_at \
             FROM database_connections ORDER BY name COLLATE NOCASE ASC",
        )?;
        let records = stmt
            .query_map([], row_to_database_connection)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(records)
    }

    pub fn database_connection_get(&self, id: &str) -> Result<Option<DatabaseConnection>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT id, name, engine, host, port, username, password_encrypted, \
             default_database, created_at, updated_at, last_connected_at \
             FROM database_connections WHERE id = ?1",
            [id],
            row_to_database_connection,
        );
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn database_connection_upsert(&self, connection: &DatabaseConnection) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO database_connections \
             (id, name, engine, host, port, username, password_encrypted, \
              default_database, created_at, updated_at, last_connected_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
             ON CONFLICT(id) DO UPDATE SET \
               name = excluded.name, engine = excluded.engine, host = excluded.host, \
               port = excluded.port, username = excluded.username, \
               password_encrypted = excluded.password_encrypted, \
               default_database = excluded.default_database, updated_at = excluded.updated_at",
            rusqlite::params![
                connection.id,
                connection.name,
                connection.engine,
                connection.host,
                connection.port,
                connection.username,
                connection.password_encrypted,
                connection.default_database,
                connection.created_at,
                connection.updated_at,
                connection.last_connected_at,
            ],
        )?;
        Ok(())
    }

    pub fn database_connection_touch(&self, id: &str, connected_at: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE database_connections SET last_connected_at = ?2 WHERE id = ?1",
            rusqlite::params![id, connected_at],
        )?;
        Ok(())
    }

    pub fn database_connection_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM database_connections WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn plugin_installation_list(&self) -> Result<Vec<PluginInstallation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT plugin_id, version, manifest_json, source, enabled, \
             granted_permissions_json, settings_json, installed_at, updated_at \
             FROM plugin_installations ORDER BY installed_at ASC",
        )?;

        let records = stmt
            .query_map([], row_to_plugin_installation)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(records)
    }

    pub fn plugin_installation_get(&self, plugin_id: &str) -> Result<Option<PluginInstallation>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT plugin_id, version, manifest_json, source, enabled, \
             granted_permissions_json, settings_json, installed_at, updated_at \
             FROM plugin_installations WHERE plugin_id = ?1",
            [plugin_id],
            row_to_plugin_installation,
        );

        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn plugin_installation_upsert(&self, installation: &PluginInstallation) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            r#"INSERT INTO plugin_installations
               (plugin_id, version, manifest_json, source, enabled,
                granted_permissions_json, settings_json, installed_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
               ON CONFLICT(plugin_id) DO UPDATE SET
                 version = excluded.version,
                 manifest_json = excluded.manifest_json,
                 source = excluded.source,
                 enabled = excluded.enabled,
                 granted_permissions_json = excluded.granted_permissions_json,
                 settings_json = excluded.settings_json,
                 updated_at = excluded.updated_at"#,
            rusqlite::params![
                installation.plugin_id,
                installation.version,
                installation.manifest_json,
                installation.source,
                installation.enabled as i32,
                installation.granted_permissions_json,
                installation.settings_json,
                installation.installed_at,
                installation.updated_at,
            ],
        )?;

        sync::record_plugin_installation_upsert(&tx, installation)?;
        tx.commit()?;
        Ok(())
    }

    pub fn plugin_installation_update_settings(
        &self,
        plugin_id: &str,
        settings_json: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE plugin_installations SET settings_json = ?2, updated_at = ?3 WHERE plugin_id = ?1",
            rusqlite::params![plugin_id, settings_json, Utc::now().timestamp()],
        )?;

        // Re-record from the stored row so the synced payload reflects the
        // bumped updated_at timestamp.
        sync::record_current_upsert(&tx, SyncEntityKind::PluginInstallation, plugin_id)?;
        tx.commit()?;
        Ok(())
    }

    pub fn plugin_installation_delete(&self, plugin_id: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM plugin_installations WHERE plugin_id = ?1",
            [plugin_id],
        )?;

        sync::record_local_delete(&tx, SyncEntityKind::PluginInstallation, plugin_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Read a value from the generic key-value `settings` table.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let value = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(value)
    }

    /// Upsert a value into the generic key-value `settings` table.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)\n             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::models::AuthType;

    /// Create an in-memory database for testing
    fn test_db() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        let db = Database {
            conn: Mutex::new(conn),
        };
        db.init_schema().unwrap();
        db
    }

    #[test]
    fn agent_activity_preserves_every_run_and_paginates_without_gaps() {
        use crate::mcp::server::{AgentActivityEvent, AgentActivityStatus};
        let db = test_db();
        for index in 0..5 {
            db.agent_activity_record(&AgentActivityEvent {
                id: format!("run-{index}"),
                tool: "cli.exec".into(),
                summary: "printf repeated-command".into(),
                status: AgentActivityStatus::Started,
                session_id: Some("session".into()),
                timestamp: index,
            })
            .unwrap();
        }
        let first = db.agent_activity_list(Some(0), None, 2).unwrap();
        assert_eq!(
            first.iter().map(|event| event.sequence).collect::<Vec<_>>(),
            [1, 2]
        );
        let next = db.agent_activity_list(Some(2), None, 2).unwrap();
        assert_eq!(
            next.iter().map(|event| event.sequence).collect::<Vec<_>>(),
            [3, 4]
        );
        let history = db.agent_activity_list(None, Some(4), 2).unwrap();
        assert_eq!(
            history
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            [3, 2]
        );
        assert_eq!(db.agent_activity_list(Some(0), None, 500).unwrap().len(), 5);
        let latest = db.agent_activity_list(None, None, 1).unwrap();
        assert_eq!(latest[0].event.id, "run-4");
        let payload: String = db
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT payload FROM agent_activity LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(payload.starts_with(CREDENTIAL_ENC_PREFIX));
        assert!(!payload.contains("repeated-command"));
    }

    #[test]
    fn agent_activity_reloads_full_multiline_commands_from_a_private_database() {
        use crate::mcp::server::{AgentActivityEvent, AgentActivityStatus};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("activity.db");
        let command = format!("printf '%s' '{}'\nwhoami", "x".repeat(500));
        {
            let db = Database::new_at(&path).unwrap();
            let mut event = AgentActivityEvent {
                id: "same-run".into(),
                tool: "exec".into(),
                summary: command.clone(),
                status: AgentActivityStatus::Started,
                session_id: None,
                timestamp: 1,
            };
            db.agent_activity_record(&event).unwrap();
            event.status = AgentActivityStatus::Succeeded;
            db.agent_activity_record(&event).unwrap();
            event.summary = "x".repeat(256 * 1024 + 1);
            assert!(db.agent_activity_record(&event).is_err());
        }
        let events = Database::new_at(path)
            .unwrap()
            .agent_activity_list(None, None, 500)
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.summary, command);
        assert_eq!(events[0].event.status, AgentActivityStatus::Succeeded);
        assert_eq!(events[1].event.status, AgentActivityStatus::Started);
    }

    fn credential_test_server(db: &Database, name: &str, auth_type: AuthType) -> Server {
        let mut server = Server {
            id: String::new(),
            name: name.into(),
            host: "test.invalid".into(),
            port: 22,
            username: "test".into(),
            auth_type,
            credential_id: None,
            group_id: None,
            tags: vec![],
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: None,
            agent_forwarding: false,
        };
        db.server_add(&mut server).unwrap();
        server
    }

    #[test]
    fn credential_edits_commit_with_metadata_and_preserve_omitted_secrets() {
        let db = test_db();
        let mut server = credential_test_server(&db, "old", AuthType::Password);
        db.credential_save("old", "password", "original", None, None)
            .unwrap();
        server.name = "renamed".into();
        db.server_update(&server).unwrap();
        assert!(db.credential_get("old").unwrap().is_none());
        assert!(db.credential_get("renamed").unwrap().unwrap().credential == "original");
        db.server_update_with_credentials(
            &server,
            Some(&CredentialUpdate {
                credential: Some("replacement".into()),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(db.credential_get("renamed").unwrap().unwrap().credential == "replacement");
        let conn = db.conn.lock().unwrap();
        let stored: String = conn
            .query_row("SELECT credential FROM server_credentials", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(stored.starts_with(CREDENTIAL_ENC_PREFIX));
        assert!(!stored.contains("replacement"));
    }

    #[test]
    fn failed_credential_edit_rolls_back_name_auth_and_secret() {
        let db = test_db();
        let mut server = credential_test_server(&db, "original", AuthType::Password);
        db.credential_save("original", "password", "keep-me", None, None)
            .unwrap();
        server.name = "attempt".into();
        server.auth_type = AuthType::KeyWithPassphrase;
        assert!(db.server_update(&server).is_err());
        assert!(db
            .server_update_with_credentials(
                &server,
                Some(&CredentialUpdate {
                    credential: Some("not-a-private-key".into()),
                    ..Default::default()
                })
            )
            .is_err());
        let actual = db.server_get(&server.id).unwrap().unwrap();
        assert_eq!(actual.name, "original");
        assert_eq!(actual.auth_type, AuthType::Password);
        assert!(db.credential_get("original").unwrap().unwrap().credential == "keep-me");
        assert!(db.credential_get("attempt").unwrap().is_none());
    }

    #[test]
    fn credential_rename_collision_never_deletes_another_secret() {
        let db = test_db();
        let mut server = credential_test_server(&db, "original", AuthType::Password);
        db.credential_save("original", "password", "one", None, None)
            .unwrap();
        db.credential_save("occupied", "password", "two", None, None)
            .unwrap();
        server.name = "occupied".into();
        assert!(db.server_update(&server).is_err());
        assert_eq!(db.server_get(&server.id).unwrap().unwrap().name, "original");
        assert!(db.credential_get("original").unwrap().unwrap().credential == "one");
        assert!(db.credential_get("occupied").unwrap().unwrap().credential == "two");
    }

    #[test]
    fn legacy_key_normalization_does_not_clear_credentials() {
        let db = test_db();
        let mut server = credential_test_server(&db, "legacy", AuthType::Key);
        db.credential_save(
            "legacy",
            "key",
            "existing-material",
            None,
            Some("/test/key"),
        )
        .unwrap();
        server.auth_type = AuthType::KeyWithPassphrase;
        db.server_update(&server).unwrap();
        let saved = db.credential_get("legacy").unwrap().unwrap();
        assert!(saved.credential == "existing-material");
        assert_eq!(saved.key_path.as_deref(), Some("/test/key"));
    }

    #[test]
    fn test_database_init() {
        let _db = test_db();
        // If we get here, schema was created successfully
    }

    #[test]
    fn test_server_crud() {
        let db = test_db();

        // Create
        let mut server = Server {
            id: String::new(),
            name: "test-server".to_string(),
            host: "192.168.1.1".to_string(),
            port: 22,
            username: "root".to_string(),
            auth_type: AuthType::Password,
            credential_id: None,
            group_id: None,
            tags: vec!["prod".to_string()],
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: None,
            agent_forwarding: false,
        };

        db.server_add(&mut server).unwrap();
        assert!(!server.id.is_empty(), "Server ID should be generated");
        assert!(server.created_at > 0, "Created timestamp should be set");
        assert!(server.updated_at > 0, "Updated timestamp should be set");

        // Read by ID
        let fetched = db.server_get(&server.id).unwrap().unwrap();
        assert_eq!(fetched.name, "test-server");
        assert_eq!(fetched.host, "192.168.1.1");
        assert_eq!(fetched.port, 22);
        assert_eq!(fetched.username, "root");
        assert_eq!(fetched.tags, vec!["prod".to_string()]);

        // Read by name
        let by_name = db.server_get_by_name("test-server").unwrap().unwrap();
        assert_eq!(by_name.id, server.id);

        // List all
        let all = db.server_list(None, None).unwrap();
        assert_eq!(all.len(), 1);

        // List with tag filter
        let by_tag = db.server_list(None, Some(&["prod".to_string()])).unwrap();
        assert_eq!(by_tag.len(), 1);

        let by_wrong_tag = db.server_list(None, Some(&["dev".to_string()])).unwrap();
        assert_eq!(by_wrong_tag.len(), 0);

        // Update
        let mut updated = fetched;
        updated.host = "192.168.1.2".to_string();
        updated.tags = vec!["prod".to_string(), "updated".to_string()];
        db.server_update(&updated).unwrap();

        let fetched2 = db.server_get(&server.id).unwrap().unwrap();
        assert_eq!(fetched2.host, "192.168.1.2");
        assert_eq!(
            fetched2.tags,
            vec!["prod".to_string(), "updated".to_string()]
        );

        // Delete
        db.server_delete(&server.id).unwrap();
        let deleted = db.server_get(&server.id).unwrap();
        assert!(deleted.is_none(), "Server should be deleted");

        // Verify list is empty after delete
        let all_after_delete = db.server_list(None, None).unwrap();
        assert_eq!(all_after_delete.len(), 0);
    }

    #[test]
    fn test_command_history_is_scoped_and_favorites_survive_clear() {
        let db = test_db();
        let mut server_a = Server {
            id: String::new(),
            name: "history-a".to_string(),
            host: "history-a.example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth_type: AuthType::Password,
            credential_id: None,
            group_id: None,
            tags: vec![],
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: None,
            agent_forwarding: false,
        };
        let mut server_b = Server {
            name: "history-b".to_string(),
            host: "history-b.example.com".to_string(),
            ..server_a.clone()
        };
        db.server_add(&mut server_a).unwrap();
        db.server_add(&mut server_b).unwrap();

        let first = db
            .history_record(&server_a.id, "systemctl status nginx")
            .unwrap();
        let repeated = db
            .history_record(&server_a.id, "systemctl status nginx")
            .unwrap();
        db.history_record(&server_a.id, "journalctl -u nginx")
            .unwrap();
        db.history_record(&server_b.id, "systemctl status nginx")
            .unwrap();

        assert_eq!(first.id, repeated.id);
        assert_eq!(repeated.use_count, 2);
        assert_eq!(
            db.history_list(&server_a.id, None, false, 200)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            db.history_list(&server_b.id, None, false, 200)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            db.history_list(&server_a.id, Some("journal"), false, 200)
                .unwrap()
                .len(),
            1
        );

        db.history_set_favorite(&first.id, true).unwrap();
        db.history_clear(&server_a.id, false).unwrap();
        let favorites = db.history_list(&server_a.id, None, true, 200).unwrap();
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].command, "systemctl status nginx");
    }

    #[test]
    fn test_server_list_with_group_filter() {
        let db = test_db();

        // Add server with group
        let mut server1 = Server {
            id: String::new(),
            name: "server1".to_string(),
            host: "192.168.1.1".to_string(),
            port: 22,
            username: "root".to_string(),
            auth_type: AuthType::Password,
            credential_id: None,
            group_id: Some("group1".to_string()),
            tags: vec![],
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: None,
            agent_forwarding: false,
        };

        let mut server2 = Server {
            id: String::new(),
            name: "server2".to_string(),
            host: "192.168.1.2".to_string(),
            port: 22,
            username: "root".to_string(),
            auth_type: AuthType::Key,
            credential_id: None,
            group_id: Some("group2".to_string()),
            tags: vec![],
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: None,
            agent_forwarding: false,
        };

        db.server_add(&mut server1).unwrap();
        db.server_add(&mut server2).unwrap();

        // List all
        let all = db.server_list(None, None).unwrap();
        assert_eq!(all.len(), 2);

        // List by group1
        let group1_servers = db.server_list(Some("group1"), None).unwrap();
        assert_eq!(group1_servers.len(), 1);
        assert_eq!(group1_servers[0].name, "server1");

        // List by group2
        let group2_servers = db.server_list(Some("group2"), None).unwrap();
        assert_eq!(group2_servers.len(), 1);
        assert_eq!(group2_servers[0].name, "server2");

        // List by non-existent group
        let no_group = db.server_list(Some("group3"), None).unwrap();
        assert_eq!(no_group.len(), 0);
    }

    #[test]
    fn test_auth_type_conversion() {
        assert_eq!(auth_type_to_string(&AuthType::Password), "password");
        assert_eq!(auth_type_to_string(&AuthType::Key), "key");
        assert_eq!(
            auth_type_to_string(&AuthType::KeyWithPassphrase),
            "key_with_passphrase"
        );

        // Test string to auth type — legacy "key" rows normalize to the
        // key+passphrase flow (empty passphrase == unencrypted key).
        assert!(matches!(
            string_to_auth_type("password"),
            AuthType::Password
        ));
        assert!(matches!(
            string_to_auth_type("key"),
            AuthType::KeyWithPassphrase
        ));
        assert!(matches!(
            string_to_auth_type("key_with_passphrase"),
            AuthType::KeyWithPassphrase
        ));
        // Default fallback
        assert!(matches!(string_to_auth_type("unknown"), AuthType::Password));
    }

    #[test]
    fn test_credential_encryption_roundtrip_and_legacy_migration() {
        let db = test_db();

        // Roundtrip: save → get returns the original secrets.
        db.credential_save(
            "roundtrip",
            "password",
            "s3cret-password",
            Some("s3cret-passphrase"),
            None,
        )
        .unwrap();
        let got = db.credential_get("roundtrip").unwrap().unwrap();
        assert_eq!(got.credential, "s3cret-password");
        assert_eq!(got.passphrase.as_deref(), Some("s3cret-passphrase"));

        // At rest the secrets are ciphertext carrying the version marker.
        {
            let conn = db.conn.lock().unwrap();
            let stored: String = conn
                .query_row(
                    "SELECT credential FROM server_credentials WHERE server_name = 'roundtrip'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(
                stored.starts_with("enc:v1:"),
                "credential must be stored encrypted"
            );
            assert!(!stored.contains("s3cret-password"));
            let stored_passphrase: Option<String> = conn
                .query_row(
                    "SELECT passphrase FROM server_credentials WHERE server_name = 'roundtrip'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let stored_passphrase = stored_passphrase.expect("passphrase should be stored");
            assert!(
                stored_passphrase.starts_with("enc:v1:"),
                "passphrase must be stored encrypted"
            );
            assert!(!stored_passphrase.contains("s3cret-passphrase"));
        }

        // Legacy plaintext row: the startup migration encrypts it and
        // credential_get transparently returns the original value.
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                r#"INSERT INTO server_credentials
                   (id, server_name, auth_type, credential, passphrase, key_path, created_at)
                   VALUES ('legacy-id', 'legacy-server', 'password', 'legacy-plaintext', NULL, NULL, 0)"#,
                [],
            )
            .unwrap();
        }
        db.init_schema().unwrap(); // re-runs the migration pass
        {
            let conn = db.conn.lock().unwrap();
            let stored: String = conn
                .query_row(
                    "SELECT credential FROM server_credentials WHERE server_name = 'legacy-server'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(
                stored.starts_with("enc:v1:"),
                "migration should encrypt legacy plaintext"
            );
        }
        let got = db.credential_get("legacy-server").unwrap().unwrap();
        assert_eq!(got.credential, "legacy-plaintext");
        assert!(got.passphrase.is_none());
    }

    #[test]
    fn credential_migration_preserves_mixed_ciphertext_and_is_idempotent() {
        let db = test_db();
        let encrypted_key = encrypt_credential_value("original-key").unwrap();
        let encrypted_pass = encrypt_credential_value("original-passphrase").unwrap();
        for (name, key, pass) in [
            (
                "encrypted-key",
                encrypted_key.as_str(),
                "original-passphrase",
            ),
            ("encrypted-pass", "original-key", encrypted_pass.as_str()),
        ] {
            db.conn.lock().unwrap().execute(
                "INSERT INTO server_credentials (id, server_name, auth_type, credential, passphrase, created_at) VALUES (?1, ?1, 'key_with_passphrase', ?2, ?3, 0)",
                rusqlite::params![name, key, pass],
            ).unwrap();
        }
        db.init_schema().unwrap();
        db.init_schema().unwrap();
        for name in ["encrypted-key", "encrypted-pass"] {
            let loaded = db.credential_get(name).unwrap().unwrap();
            assert_eq!(loaded.credential, "original-key");
            assert_eq!(loaded.passphrase.as_deref(), Some("original-passphrase"));
        }
        let conn = db.conn.lock().unwrap();
        let stored_key: String = conn
            .query_row(
                "SELECT credential FROM server_credentials WHERE id = 'encrypted-key'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let stored_pass: String = conn
            .query_row(
                "SELECT passphrase FROM server_credentials WHERE id = 'encrypted-pass'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_key, encrypted_key);
        assert_eq!(stored_pass, encrypted_pass);
    }

    #[test]
    fn test_plugin_installation_lifecycle() {
        let db = test_db();
        let now = Utc::now().timestamp();
        let installation = PluginInstallation {
            plugin_id: "example.plugin".to_string(),
            version: "1.0.0".to_string(),
            manifest_json: r#"{"schemaVersion":1}"#.to_string(),
            source: "external".to_string(),
            enabled: false,
            granted_permissions_json: "[]".to_string(),
            settings_json: "{}".to_string(),
            installed_at: now,
            updated_at: now,
        };

        db.plugin_installation_upsert(&installation).unwrap();
        let stored = db
            .plugin_installation_get("example.plugin")
            .unwrap()
            .unwrap();
        assert_eq!(stored, installation);

        let mut enabled = stored;
        enabled.enabled = true;
        db.plugin_installation_upsert(&enabled).unwrap();
        db.plugin_installation_update_settings("example.plugin", r#"{"rows":50}"#)
            .unwrap();
        let updated = db
            .plugin_installation_get("example.plugin")
            .unwrap()
            .unwrap();
        assert!(updated.enabled);
        assert_eq!(updated.settings_json, r#"{"rows":50}"#);
        assert_eq!(db.plugin_installation_list().unwrap().len(), 1);

        db.plugin_installation_delete("example.plugin").unwrap();
        assert!(db
            .plugin_installation_get("example.plugin")
            .unwrap()
            .is_none());
    }
}
