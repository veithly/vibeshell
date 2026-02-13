use anyhow::Result;
use chrono::Utc;
use directories::ProjectDirs;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

use crate::storage::models::{AuthType, Server, TunnelConfig, TunnelType, CommandSnippet, Recording};

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new() -> Result<Self> {
        let db_path = Self::get_db_path()?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn get_db_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "vibeshell", "VibeShell")
            .ok_or_else(|| anyhow::anyhow!("Could not determine project directories"))?;
        Ok(proj_dirs.data_dir().join("vibeshell.db"))
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

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
        let conn = self.conn.lock().unwrap();

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

        conn.execute(
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

        Ok(())
    }

    /// Update an existing server. Updates the updated_at timestamp automatically.
    pub fn server_update(&self, server: &Server) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let now = Utc::now().timestamp();
        let tags_json = serde_json::to_string(&server.tags)?;
        let auth_type_str = auth_type_to_string(&server.auth_type);

        conn.execute(
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

        Ok(())
    }

    /// Delete a server by its ID
    pub fn server_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM servers WHERE id = ?1", [id])?;
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
        "key" => AuthType::Key,
        "key_with_passphrase" => AuthType::KeyWithPassphrase,
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

        let groups = stmt.query_map([], |row| {
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
        let conn = self.conn.lock().unwrap();

        if group.id.is_empty() {
            group.id = Uuid::new_v4().to_string();
        }

        conn.execute(
            "INSERT INTO groups (id, name, parent_id, color) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![group.id, group.name, group.parent_id, group.color],
        )?;

        Ok(())
    }

    /// Delete a group
    pub fn group_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM groups WHERE id = ?1", [id])?;
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

impl Database {
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

        if let Some(id) = existing {
            // Update existing
            conn.execute(
                r#"UPDATE server_credentials SET
                   auth_type = ?2, credential = ?3, passphrase = ?4, key_path = ?5
                   WHERE id = ?1"#,
                rusqlite::params![id, auth_type, credential, passphrase, key_path],
            )?;
            Ok(id)
        } else {
            // Insert new
            let id = Uuid::new_v4().to_string();
            conn.execute(
                r#"INSERT INTO server_credentials (id, server_name, auth_type, credential, passphrase, key_path, created_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
                rusqlite::params![id, server_name, auth_type, credential, passphrase, key_path, now],
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
                Ok(Credential {
                    id: row.get(0)?,
                    server_name: row.get(1)?,
                    auth_type: row.get(2)?,
                    credential: row.get(3)?,
                    passphrase: row.get(4)?,
                    key_path: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        );

        match result {
            Ok(cred) => Ok(Some(cred)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete credentials for a server
    pub fn credential_delete(&self, server_name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM server_credentials WHERE server_name = ?1", [server_name])?;
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

        let configs = stmt.query_map([server_id], |row| {
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
        conn.execute("DELETE FROM tunnel_configs WHERE server_id = ?1", [server_id])?;
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

        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(cat) = category {
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
        let snippets = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
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
        let conn = self.conn.lock().unwrap();
        if snippet.id.is_empty() {
            snippet.id = Uuid::new_v4().to_string();
        }
        let now = Utc::now().timestamp();
        snippet.created_at = now;
        snippet.updated_at = now;

        let tags_json = serde_json::to_string(&snippet.tags)?;

        conn.execute(
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
        Ok(())
    }

    /// Update a command snippet
    pub fn snippet_update(&self, snippet: &CommandSnippet) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        let tags_json = serde_json::to_string(&snippet.tags)?;

        conn.execute(
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
        Ok(())
    }

    /// Delete a command snippet
    pub fn snippet_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM command_snippets WHERE id = ?1", [id])?;
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

        let snippets = stmt.query_map([&pattern], |row| {
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
}

// =============================================================================
// Recording Operations (table already exists from init_schema)
// =============================================================================

impl Database {
    /// List recordings, optionally filtered by server_id
    pub fn recording_list(&self, server_id: Option<&str>) -> Result<Vec<Recording>> {
        let conn = self.conn.lock().unwrap();
        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(sid) = server_id {
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
        let recordings = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
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
    fn test_database_init() {
        let _db = Database::new().expect("Failed to create database");
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
        let by_tag = db
            .server_list(None, Some(&["prod".to_string()]))
            .unwrap();
        assert_eq!(by_tag.len(), 1);

        let by_wrong_tag = db
            .server_list(None, Some(&["dev".to_string()]))
            .unwrap();
        assert_eq!(by_wrong_tag.len(), 0);

        // Update
        let mut updated = fetched;
        updated.host = "192.168.1.2".to_string();
        updated.tags = vec!["prod".to_string(), "updated".to_string()];
        db.server_update(&updated).unwrap();

        let fetched2 = db.server_get(&server.id).unwrap().unwrap();
        assert_eq!(fetched2.host, "192.168.1.2");
        assert_eq!(fetched2.tags, vec!["prod".to_string(), "updated".to_string()]);

        // Delete
        db.server_delete(&server.id).unwrap();
        let deleted = db.server_get(&server.id).unwrap();
        assert!(deleted.is_none(), "Server should be deleted");

        // Verify list is empty after delete
        let all_after_delete = db.server_list(None, None).unwrap();
        assert_eq!(all_after_delete.len(), 0);
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

        // Test string to auth type
        assert!(matches!(string_to_auth_type("password"), AuthType::Password));
        assert!(matches!(string_to_auth_type("key"), AuthType::Key));
        assert!(matches!(
            string_to_auth_type("key_with_passphrase"),
            AuthType::KeyWithPassphrase
        ));
        // Default fallback
        assert!(matches!(string_to_auth_type("unknown"), AuthType::Password));
    }
}
