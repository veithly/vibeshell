# VibeShell Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a high-performance SSH/SFTP terminal using Tauri with session sharing between UI and CLI, MCP integration for AI tools, and one-click installation to Claude Code/Cursor/Codex/Open Code.

**Architecture:** Tauri app with shared Rust core library used by both GUI and standalone CLI. Session Manager maintains persistent sessions that multiple clients (GUI/CLI/MCP) can attach to. All functionality exposed via MCP Server for AI tool integration.

**Tech Stack:** Tauri 2.x, React 18 + TypeScript + Vite, xterm.js, russh (SSH/SFTP), SQLite (rusqlite), ring + argon2 (encryption), Zustand, Tailwind CSS + shadcn/ui

---

## Phase 1: Project Foundation

### Task 1.1: Initialize Tauri Project

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `package.json`
- Create: `vite.config.ts`
- Create: `tsconfig.json`

**Step 1: Create Tauri project with React template**

Run:
```bash
pnpm create tauri-app vibeshell-init --template react-ts
```

**Step 2: Copy generated files to project root**

Run:
```bash
cp -r vibeshell-init/* . && rm -rf vibeshell-init
```

**Step 3: Update package.json with project name**

```json
{
  "name": "vibeshell",
  "version": "0.1.0",
  "description": "High-performance SSH/SFTP terminal with AI integration",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "tauri": "tauri"
  }
}
```

**Step 4: Install dependencies**

Run:
```bash
pnpm install
```

**Step 5: Verify project runs**

Run:
```bash
pnpm tauri dev
```
Expected: Tauri window opens with React app

**Step 6: Commit**

```bash
git add .
git commit -m "feat: initialize Tauri project with React template"
```

---

### Task 1.2: Setup Rust Workspace for CLI

**Files:**
- Modify: `Cargo.toml` (root)
- Create: `cli/Cargo.toml`
- Create: `cli/src/main.rs`
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/lib.rs`

**Step 1: Create root Cargo.toml for workspace**

Create `Cargo.toml` at project root:
```toml
[workspace]
members = ["src-tauri", "cli"]
resolver = "2"

[workspace.package]
version = "0.1.0"
authors = ["VibeShell Team"]
edition = "2021"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
anyhow = "1"
```

**Step 2: Create cli/Cargo.toml**

```toml
[package]
name = "vshell"
version.workspace = true
edition.workspace = true

[[bin]]
name = "vshell"
path = "src/main.rs"

[dependencies]
vibeshell-core = { path = "../src-tauri", package = "vibeshell" }
clap = { version = "4", features = ["derive"] }
tokio.workspace = true
```

**Step 3: Update src-tauri/Cargo.toml to expose lib**

Add to `src-tauri/Cargo.toml`:
```toml
[lib]
name = "vibeshell_core"
crate-type = ["lib", "cdylib", "staticlib"]
```

**Step 4: Create minimal lib.rs**

Create `src-tauri/src/lib.rs`:
```rust
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 5: Create minimal CLI main.rs**

Create `cli/src/main.rs`:
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vshell")]
#[command(about = "VibeShell - High-performance SSH/SFTP terminal")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show version
    Version,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Version) => {
            println!("vshell {}", vibeshell_core::version());
        }
        None => {
            println!("VibeShell - Run 'vshell --help' for usage");
        }
    }
}
```

**Step 6: Build and test CLI**

Run:
```bash
cargo build -p vshell
./target/debug/vshell version
```
Expected: `vshell 0.1.0`

**Step 7: Commit**

```bash
git add .
git commit -m "feat: setup Rust workspace with CLI binary"
```

---

### Task 1.3: Setup SQLite Database

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/storage/mod.rs`
- Create: `src-tauri/src/storage/database.rs`
- Create: `src-tauri/src/storage/models.rs`

**Step 1: Add rusqlite dependency**

Add to `src-tauri/Cargo.toml`:
```toml
[dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
directories = "5"
```

**Step 2: Create storage module structure**

Create `src-tauri/src/storage/mod.rs`:
```rust
pub mod database;
pub mod models;

pub use database::Database;
pub use models::*;
```

**Step 3: Create models**

Create `src-tauri/src/storage/models.rs`:
```rust
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
```

**Step 4: Create database module**

Create `src-tauri/src/storage/database.rs`:
```rust
use anyhow::Result;
use directories::ProjectDirs;
use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::Mutex;

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
        let db = Self { conn: Mutex::new(conn) };
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

        conn.execute_batch(r#"
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
        "#)?;

        Ok(())
    }
}
```

**Step 5: Export storage module from lib.rs**

Update `src-tauri/src/lib.rs`:
```rust
pub mod storage;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 6: Test database creation**

Add test to `src-tauri/src/storage/database.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_init() {
        let db = Database::new().expect("Failed to create database");
        // If we get here, schema was created successfully
    }
}
```

Run:
```bash
cargo test -p vibeshell test_database_init
```
Expected: PASS

**Step 7: Commit**

```bash
git add .
git commit -m "feat: setup SQLite database with schema"
```

---

### Task 1.4: Implement Credential Encryption

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/storage/crypto.rs`
- Modify: `src-tauri/src/storage/mod.rs`

**Step 1: Add crypto dependencies**

Add to `src-tauri/Cargo.toml`:
```toml
[dependencies]
ring = "0.17"
argon2 = "0.5"
rand = "0.8"
base64 = "0.21"
```

**Step 2: Create crypto module**

Create `src-tauri/src/storage/crypto.rs`:
```rust
use anyhow::{Result, anyhow};
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
use rand::rngs::OsRng;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

pub struct Crypto {
    key: LessSafeKey,
}

impl Crypto {
    /// Create crypto instance from master password
    pub fn from_password(password: &str, salt: &[u8]) -> Result<Self> {
        let argon2 = Argon2::default();
        let mut key_bytes = [0u8; KEY_LEN];

        argon2.hash_password_into(
            password.as_bytes(),
            salt,
            &mut key_bytes,
        ).map_err(|e| anyhow!("Key derivation failed: {}", e))?;

        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
            .map_err(|_| anyhow!("Failed to create encryption key"))?;

        Ok(Self {
            key: LessSafeKey::new(unbound_key),
        })
    }

    /// Generate a new random salt
    pub fn generate_salt() -> Vec<u8> {
        let salt = SaltString::generate(&mut OsRng);
        salt.as_bytes().to_vec()
    }

    /// Encrypt data
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| anyhow!("Failed to generate nonce"))?;

        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut ciphertext = plaintext.to_vec();

        self.key.seal_in_place_append_tag(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| anyhow!("Encryption failed"))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);
        Ok(result)
    }

    /// Decrypt data
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < NONCE_LEN {
            return Err(anyhow!("Ciphertext too short"));
        }

        let (nonce_bytes, encrypted) = ciphertext.split_at(NONCE_LEN);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes.try_into().unwrap());

        let mut plaintext = encrypted.to_vec();
        self.key.open_in_place(nonce, Aad::empty(), &mut plaintext)
            .map_err(|_| anyhow!("Decryption failed - wrong password?"))?;

        // Remove auth tag
        plaintext.truncate(plaintext.len() - 16);
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let salt = Crypto::generate_salt();
        let crypto = Crypto::from_password("test_password", &salt).unwrap();

        let plaintext = b"Hello, World!";
        let ciphertext = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&ciphertext).unwrap();

        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn test_wrong_password_fails() {
        let salt = Crypto::generate_salt();
        let crypto1 = Crypto::from_password("password1", &salt).unwrap();
        let crypto2 = Crypto::from_password("password2", &salt).unwrap();

        let ciphertext = crypto1.encrypt(b"secret").unwrap();
        let result = crypto2.decrypt(&ciphertext);

        assert!(result.is_err());
    }
}
```

**Step 3: Export crypto module**

Update `src-tauri/src/storage/mod.rs`:
```rust
pub mod database;
pub mod models;
pub mod crypto;

pub use database::Database;
pub use models::*;
pub use crypto::Crypto;
```

**Step 4: Run tests**

Run:
```bash
cargo test -p vibeshell crypto
```
Expected: All tests pass

**Step 5: Commit**

```bash
git add .
git commit -m "feat: implement AES-256-GCM credential encryption"
```

---

### Task 1.5: Implement Server CRUD Operations

**Files:**
- Modify: `src-tauri/src/storage/database.rs`

**Step 1: Add UUID dependency**

Add to `src-tauri/Cargo.toml`:
```toml
[dependencies]
uuid = { version = "1", features = ["v4"] }
chrono = "0.4"
```

**Step 2: Implement server CRUD**

Add to `src-tauri/src/storage/database.rs`:
```rust
use crate::storage::models::{Server, AuthType, Group};
use chrono::Utc;
use uuid::Uuid;

impl Database {
    // === Server Operations ===

    pub fn server_list(&self, group_id: Option<&str>, tags: Option<&[String]>) -> Result<Vec<Server>> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from("SELECT * FROM servers WHERE 1=1");

        if group_id.is_some() {
            sql.push_str(" AND group_id = ?1");
        }

        let mut stmt = conn.prepare(&sql)?;

        let servers = if let Some(gid) = group_id {
            stmt.query_map([gid], |row| self.row_to_server(row))?
        } else {
            stmt.query_map([], |row| self.row_to_server(row))?
        };

        let mut result: Vec<Server> = servers.filter_map(|s| s.ok()).collect();

        // Filter by tags if provided
        if let Some(tag_filter) = tags {
            result.retain(|s| tag_filter.iter().any(|t| s.tags.contains(t)));
        }

        Ok(result)
    }

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

    pub fn server_add(&self, server: &mut Server) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();

        server.id = Uuid::new_v4().to_string();
        server.created_at = now;
        server.updated_at = now;

        conn.execute(
            "INSERT INTO servers (id, name, host, port, username, auth_type, credential_id, group_id, tags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                server.id,
                server.name,
                server.host,
                server.port,
                server.username,
                serde_json::to_string(&server.auth_type)?,
                server.credential_id,
                server.group_id,
                serde_json::to_string(&server.tags)?,
                server.created_at,
                server.updated_at,
            ],
        )?;

        Ok(())
    }

    pub fn server_update(&self, server: &Server) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();

        conn.execute(
            "UPDATE servers SET name=?2, host=?3, port=?4, username=?5, auth_type=?6, credential_id=?7, group_id=?8, tags=?9, updated_at=?10 WHERE id=?1",
            params![
                server.id,
                server.name,
                server.host,
                server.port,
                server.username,
                serde_json::to_string(&server.auth_type)?,
                server.credential_id,
                server.group_id,
                serde_json::to_string(&server.tags)?,
                now,
            ],
        )?;

        Ok(())
    }

    pub fn server_delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM servers WHERE id = ?1", [id])?;
        Ok(())
    }

    fn row_to_server(&self, row: &rusqlite::Row) -> rusqlite::Result<Server> {
        let auth_type_str: String = row.get(5)?;
        let tags_str: String = row.get(8)?;

        Ok(Server {
            id: row.get(0)?,
            name: row.get(1)?,
            host: row.get(2)?,
            port: row.get(3)?,
            username: row.get(4)?,
            auth_type: serde_json::from_str(&auth_type_str).unwrap_or(AuthType::Password),
            credential_id: row.get(6)?,
            group_id: row.get(7)?,
            tags: serde_json::from_str(&tags_str).unwrap_or_default(),
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }
}
```

**Step 3: Add tests**

Add to `src-tauri/src/storage/database.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::models::AuthType;

    fn test_db() -> Database {
        // Use in-memory database for tests
        let conn = Connection::open_in_memory().unwrap();
        let db = Database { conn: Mutex::new(conn) };
        db.init_schema().unwrap();
        db
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
        };

        db.server_add(&mut server).unwrap();
        assert!(!server.id.is_empty());

        // Read
        let fetched = db.server_get(&server.id).unwrap().unwrap();
        assert_eq!(fetched.name, "test-server");

        // Read by name
        let by_name = db.server_get_by_name("test-server").unwrap().unwrap();
        assert_eq!(by_name.id, server.id);

        // List
        let all = db.server_list(None, None).unwrap();
        assert_eq!(all.len(), 1);

        // Update
        let mut updated = fetched;
        updated.host = "192.168.1.2".to_string();
        db.server_update(&updated).unwrap();

        let fetched2 = db.server_get(&server.id).unwrap().unwrap();
        assert_eq!(fetched2.host, "192.168.1.2");

        // Delete
        db.server_delete(&server.id).unwrap();
        let deleted = db.server_get(&server.id).unwrap();
        assert!(deleted.is_none());
    }
}
```

**Step 4: Run tests**

Run:
```bash
cargo test -p vibeshell server_crud
```
Expected: PASS

**Step 5: Commit**

```bash
git add .
git commit -m "feat: implement server CRUD operations"
```

---

## Phase 2: SSH Connection

### Task 2.1: Setup russh SSH Client

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/ssh/mod.rs`
- Create: `src-tauri/src/ssh/client.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add russh dependencies**

Add to `src-tauri/Cargo.toml`:
```toml
[dependencies]
russh = "0.44"
russh-keys = "0.44"
async-trait = "0.1"
```

**Step 2: Create SSH module structure**

Create `src-tauri/src/ssh/mod.rs`:
```rust
pub mod client;

pub use client::SshClient;
```

**Step 3: Create SSH client handler**

Create `src-tauri/src/ssh/client.rs`:
```rust
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use russh::*;
use russh_keys::*;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct SshClient {
    session: Option<client::Handle<ClientHandler>>,
    output_tx: mpsc::Sender<Vec<u8>>,
}

struct ClientHandler {
    output_tx: mpsc::Sender<Vec<u8>>,
}

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // TODO: Implement proper host key verification
        Ok(true)
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let _ = self.output_tx.send(data.to_vec()).await;
        Ok(())
    }
}

impl SshClient {
    pub fn new(output_tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            session: None,
            output_tx,
        }
    }

    pub async fn connect_password(
        &mut self,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let config = client::Config::default();
        let config = Arc::new(config);

        let handler = ClientHandler {
            output_tx: self.output_tx.clone(),
        };

        let mut session = client::connect(config, (host, port), handler).await?;

        let auth_result = session.authenticate_password(username, password).await?;

        if !auth_result {
            return Err(anyhow!("Authentication failed"));
        }

        self.session = Some(session);
        Ok(())
    }

    pub async fn connect_key(
        &mut self,
        host: &str,
        port: u16,
        username: &str,
        private_key: &str,
        passphrase: Option<&str>,
    ) -> Result<()> {
        let config = client::Config::default();
        let config = Arc::new(config);

        let handler = ClientHandler {
            output_tx: self.output_tx.clone(),
        };

        let mut session = client::connect(config, (host, port), handler).await?;

        let key_pair = if let Some(pass) = passphrase {
            decode_secret_key(private_key, Some(pass))?
        } else {
            decode_secret_key(private_key, None)?
        };

        let auth_result = session
            .authenticate_publickey(username, Arc::new(key_pair))
            .await?;

        if !auth_result {
            return Err(anyhow!("Key authentication failed"));
        }

        self.session = Some(session);
        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(session) = self.session.take() {
            session.disconnect(Disconnect::ByApplication, "", "en").await?;
        }
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }
}
```

**Step 4: Export SSH module**

Update `src-tauri/src/lib.rs`:
```rust
pub mod storage;
pub mod ssh;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 5: Build to verify**

Run:
```bash
cargo build -p vibeshell
```
Expected: Build succeeds

**Step 6: Commit**

```bash
git add .
git commit -m "feat: setup russh SSH client"
```

---

### Task 2.2: Implement Session Manager

**Files:**
- Create: `src-tauri/src/session/mod.rs`
- Create: `src-tauri/src/session/manager.rs`
- Create: `src-tauri/src/session/session.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create session module structure**

Create `src-tauri/src/session/mod.rs`:
```rust
pub mod manager;
pub mod session;

pub use manager::SessionManager;
pub use session::{Session, SessionState, SessionInfo};
```

**Step 2: Create session struct**

Create `src-tauri/src/session/session.rs`:
```rust
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Connecting,
    Connected,
    Disconnected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    pub state: SessionState,
    pub created_at: i64,
    pub clients: usize,
}

pub struct Session {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    state: Arc<RwLock<SessionState>>,
    created_at: i64,

    // Channel for sending input to SSH
    input_tx: mpsc::Sender<Vec<u8>>,

    // Broadcast channel for output to all clients
    output_tx: broadcast::Sender<Vec<u8>>,

    // Track connected clients
    client_count: Arc<RwLock<usize>>,
}

impl Session {
    pub fn new(
        server_id: String,
        server_name: String,
        input_tx: mpsc::Sender<Vec<u8>>,
        output_tx: broadcast::Sender<Vec<u8>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            server_id,
            server_name,
            state: Arc::new(RwLock::new(SessionState::Connecting)),
            created_at: Utc::now().timestamp(),
            input_tx,
            output_tx,
            client_count: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn set_state(&self, state: SessionState) {
        let mut s = self.state.write().await;
        *s = state;
    }

    pub async fn get_state(&self) -> SessionState {
        self.state.read().await.clone()
    }

    pub async fn get_info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            server_id: self.server_id.clone(),
            server_name: self.server_name.clone(),
            state: self.get_state().await,
            created_at: self.created_at,
            clients: *self.client_count.read().await,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.output_tx.subscribe()
    }

    pub async fn send_input(&self, data: Vec<u8>) -> Result<()> {
        self.input_tx.send(data).await?;
        Ok(())
    }

    pub async fn attach(&self) {
        let mut count = self.client_count.write().await;
        *count += 1;
    }

    pub async fn detach(&self) {
        let mut count = self.client_count.write().await;
        if *count > 0 {
            *count -= 1;
        }
    }

    pub async fn client_count(&self) -> usize {
        *self.client_count.read().await
    }
}
```

**Step 3: Create session manager**

Create `src-tauri/src/session/manager.rs`:
```rust
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::session::{Session, SessionInfo, SessionState};
use crate::storage::{Database, Server};

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<Session>>>>,
    database: Arc<Database>,
}

impl SessionManager {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            database,
        }
    }

    pub async fn list(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut infos = Vec::new();

        for session in sessions.values() {
            infos.push(session.get_info().await);
        }

        infos
    }

    pub async fn get(&self, id: &str) -> Option<Arc<Session>> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    pub async fn create(&self, server_id: &str) -> Result<Arc<Session>> {
        let server = self.database.server_get(server_id)?
            .ok_or_else(|| anyhow!("Server not found: {}", server_id))?;

        self.create_for_server(&server).await
    }

    pub async fn create_by_name(&self, server_name: &str) -> Result<Arc<Session>> {
        let server = self.database.server_get_by_name(server_name)?
            .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;

        self.create_for_server(&server).await
    }

    async fn create_for_server(&self, server: &Server) -> Result<Arc<Session>> {
        let (input_tx, _input_rx) = tokio::sync::mpsc::channel(256);
        let (output_tx, _) = tokio::sync::broadcast::channel(256);

        let session = Arc::new(Session::new(
            server.id.clone(),
            server.name.clone(),
            input_tx,
            output_tx,
        ));

        // TODO: Actually connect SSH here
        session.set_state(SessionState::Connected).await;

        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());

        Ok(session)
    }

    pub async fn kill(&self, id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.remove(id) {
            session.set_state(SessionState::Disconnected).await;
            // TODO: Disconnect SSH
        }

        Ok(())
    }

    pub async fn kill_all(&self) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        for session in sessions.values() {
            session.set_state(SessionState::Disconnected).await;
        }

        sessions.clear();
        Ok(())
    }
}
```

**Step 4: Export session module**

Update `src-tauri/src/lib.rs`:
```rust
pub mod storage;
pub mod ssh;
pub mod session;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 5: Build to verify**

Run:
```bash
cargo build -p vibeshell
```
Expected: Build succeeds

**Step 6: Commit**

```bash
git add .
git commit -m "feat: implement session manager with multi-client support"
```

---

### Task 2.3: Setup React Frontend with xterm.js

**Files:**
- Modify: `package.json`
- Create: `src/components/Terminal/Terminal.tsx`
- Create: `src/components/Terminal/index.ts`
- Modify: `src/App.tsx`
- Create: `src/App.css`

**Step 1: Install frontend dependencies**

Run:
```bash
pnpm add xterm @xterm/addon-fit @xterm/addon-web-links zustand
pnpm add -D tailwindcss postcss autoprefixer @types/node
pnpm exec tailwindcss init -p
```

**Step 2: Configure Tailwind**

Create `tailwind.config.js`:
```javascript
/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {},
  },
  plugins: [],
}
```

**Step 3: Update src/index.css**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  font-family: Inter, system-ui, -apple-system, sans-serif;
}

body {
  margin: 0;
  padding: 0;
  min-height: 100vh;
}

#root {
  height: 100vh;
}
```

**Step 4: Create Terminal component**

Create `src/components/Terminal/Terminal.tsx`:
```tsx
import { useEffect, useRef } from 'react';
import { Terminal as XTerm } from 'xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import 'xterm/css/xterm.css';

interface TerminalProps {
  sessionId?: string;
  onData?: (data: string) => void;
}

export function Terminal({ sessionId, onData }: TerminalProps) {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

  useEffect(() => {
    if (!terminalRef.current) return;

    const xterm = new XTerm({
      theme: {
        background: '#1a1b26',
        foreground: '#a9b1d6',
        cursor: '#c0caf5',
        cursorAccent: '#1a1b26',
        selectionBackground: '#33467c',
        black: '#32344a',
        red: '#f7768e',
        green: '#9ece6a',
        yellow: '#e0af68',
        blue: '#7aa2f7',
        magenta: '#ad8ee6',
        cyan: '#449dab',
        white: '#787c99',
        brightBlack: '#444b6a',
        brightRed: '#ff7a93',
        brightGreen: '#b9f27c',
        brightYellow: '#ff9e64',
        brightBlue: '#7da6ff',
        brightMagenta: '#bb9af7',
        brightCyan: '#0db9d7',
        brightWhite: '#acb0d0',
      },
      fontSize: 14,
      fontFamily: 'JetBrains Mono, Menlo, Monaco, Consolas, monospace',
      cursorBlink: true,
      cursorStyle: 'block',
    });

    const fitAddon = new FitAddon();
    const webLinksAddon = new WebLinksAddon();

    xterm.loadAddon(fitAddon);
    xterm.loadAddon(webLinksAddon);
    xterm.open(terminalRef.current);
    fitAddon.fit();

    xtermRef.current = xterm;
    fitAddonRef.current = fitAddon;

    xterm.onData((data) => {
      onData?.(data);
    });

    // Welcome message
    xterm.writeln('\x1b[1;34mVibeShell Terminal\x1b[0m');
    xterm.writeln('Type to interact with your session.\n');

    const handleResize = () => {
      fitAddon.fit();
    };

    window.addEventListener('resize', handleResize);

    return () => {
      window.removeEventListener('resize', handleResize);
      xterm.dispose();
    };
  }, [sessionId]);

  return (
    <div
      ref={terminalRef}
      className="w-full h-full bg-[#1a1b26]"
    />
  );
}
```

**Step 5: Create index export**

Create `src/components/Terminal/index.ts`:
```typescript
export { Terminal } from './Terminal';
```

**Step 6: Update App.tsx**

```tsx
import { Terminal } from './components/Terminal';

function App() {
  const handleData = (data: string) => {
    console.log('Terminal input:', data);
  };

  return (
    <div className="h-screen flex flex-col bg-gray-900">
      <header className="h-12 flex items-center px-4 bg-gray-800 border-b border-gray-700">
        <h1 className="text-white font-semibold">VibeShell</h1>
      </header>
      <main className="flex-1 p-2">
        <Terminal onData={handleData} />
      </main>
    </div>
  );
}

export default App;
```

**Step 7: Test frontend**

Run:
```bash
pnpm tauri dev
```
Expected: Window opens with terminal component displayed

**Step 8: Commit**

```bash
git add .
git commit -m "feat: setup React frontend with xterm.js terminal"
```

---

### Task 2.4: Create Tauri Commands for SSH

**Files:**
- Create: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/session.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create commands module**

Create `src-tauri/src/commands/mod.rs`:
```rust
pub mod session;

pub use session::*;
```

**Step 2: Create session commands**

Create `src-tauri/src/commands/session.rs`:
```rust
use serde::{Deserialize, Serialize};
use tauri::State;
use std::sync::Arc;

use crate::session::{SessionManager, SessionInfo};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub server_name: String,
}

#[tauri::command]
pub async fn session_list(
    manager: State<'_, Arc<SessionManager>>,
) -> Result<Vec<SessionInfo>, String> {
    Ok(manager.list().await)
}

#[tauri::command]
pub async fn session_create(
    manager: State<'_, Arc<SessionManager>>,
    server_name: String,
) -> Result<SessionInfo, String> {
    let session = manager
        .create_by_name(&server_name)
        .await
        .map_err(|e| e.to_string())?;

    Ok(session.get_info().await)
}

#[tauri::command]
pub async fn session_kill(
    manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<(), String> {
    manager
        .kill(&session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn session_kill_all(
    manager: State<'_, Arc<SessionManager>>,
) -> Result<(), String> {
    manager
        .kill_all()
        .await
        .map_err(|e| e.to_string())
}
```

**Step 3: Export commands module**

Update `src-tauri/src/lib.rs`:
```rust
pub mod storage;
pub mod ssh;
pub mod session;
pub mod commands;

pub use storage::Database;
pub use session::SessionManager;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 4: Update main.rs to register commands**

Update `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use vibeshell_core::{Database, SessionManager};
use vibeshell_core::commands::{session_list, session_create, session_kill, session_kill_all};

fn main() {
    let database = Arc::new(Database::new().expect("Failed to initialize database"));
    let session_manager = Arc::new(SessionManager::new(database.clone()));

    tauri::Builder::default()
        .manage(database)
        .manage(session_manager)
        .invoke_handler(tauri::generate_handler![
            session_list,
            session_create,
            session_kill,
            session_kill_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 5: Build and test**

Run:
```bash
cargo build -p vibeshell
pnpm tauri dev
```
Expected: App runs without errors

**Step 6: Commit**

```bash
git add .
git commit -m "feat: add Tauri commands for session management"
```

---

## Phase 3: Session Sharing (CLI ↔ GUI)

### Task 3.1: Implement IPC for Session Sharing

**Files:**
- Create: `src-tauri/src/ipc/mod.rs`
- Create: `src-tauri/src/ipc/socket.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add IPC dependencies**

Add to `src-tauri/Cargo.toml`:
```toml
[dependencies]
interprocess = "2"
```

**Step 2: Create IPC module**

Create `src-tauri/src/ipc/mod.rs`:
```rust
pub mod socket;

pub use socket::{IpcServer, IpcClient};
```

**Step 3: Create IPC socket implementation**

Create `src-tauri/src/ipc/socket.rs`:
```rust
use anyhow::Result;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerOptions, Stream,
    traits::{ListenerExt, Stream as StreamTrait},
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};

#[cfg(windows)]
const SOCKET_NAME: &str = "@vibeshell.sock";
#[cfg(not(windows))]
const SOCKET_NAME: &str = "/tmp/vibeshell.sock";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcMessage {
    // Requests
    ListSessions,
    CreateSession { server_name: String },
    AttachSession { session_id: String },
    DetachSession { session_id: String },
    KillSession { session_id: String },
    SendInput { session_id: String, data: Vec<u8> },

    // Responses
    SessionList { sessions: Vec<String> },
    SessionCreated { session_id: String },
    SessionOutput { session_id: String, data: Vec<u8> },
    Error { message: String },
    Ok,
}

pub struct IpcServer {
    // Server state managed externally
}

impl IpcServer {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn start<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(IpcMessage) -> IpcMessage + Send + Sync + 'static,
    {
        let name = SOCKET_NAME.to_ns_name::<GenericNamespaced>()?;
        let opts = ListenerOptions::new().name(name);
        let listener = opts.create_sync()?;

        loop {
            let conn = listener.accept()?;
            let response = Self::handle_connection(conn, &handler)?;
            // Response sent in handle_connection
        }
    }

    fn handle_connection<F>(mut conn: Stream, handler: &F) -> Result<IpcMessage>
    where
        F: Fn(IpcMessage) -> IpcMessage,
    {
        let mut reader = BufReader::new(&mut conn);
        let mut line = String::new();
        reader.read_line(&mut line)?;

        let message: IpcMessage = serde_json::from_str(&line)?;
        let response = handler(message);

        let response_json = serde_json::to_string(&response)? + "\n";
        conn.write_all(response_json.as_bytes())?;

        Ok(response)
    }
}

pub struct IpcClient;

impl IpcClient {
    pub fn send(message: &IpcMessage) -> Result<IpcMessage> {
        let name = SOCKET_NAME.to_ns_name::<GenericNamespaced>()?;
        let mut conn = Stream::connect(name)?;

        let request = serde_json::to_string(message)? + "\n";
        conn.write_all(request.as_bytes())?;

        let mut reader = BufReader::new(&mut conn);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;

        let response: IpcMessage = serde_json::from_str(&response_line)?;
        Ok(response)
    }
}
```

**Step 4: Export IPC module**

Update `src-tauri/src/lib.rs`:
```rust
pub mod storage;
pub mod ssh;
pub mod session;
pub mod commands;
pub mod ipc;

pub use storage::Database;
pub use session::SessionManager;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 5: Build to verify**

Run:
```bash
cargo build -p vibeshell
```
Expected: Build succeeds

**Step 6: Commit**

```bash
git add .
git commit -m "feat: implement IPC socket for CLI-GUI session sharing"
```

---

### Task 3.2: Implement CLI Session Commands

**Files:**
- Modify: `cli/src/main.rs`
- Create: `cli/src/commands/mod.rs`
- Create: `cli/src/commands/session.rs`
- Create: `cli/src/commands/ssh.rs`

**Step 1: Create CLI commands structure**

Create `cli/src/commands/mod.rs`:
```rust
pub mod session;
pub mod ssh;
```

**Step 2: Create session commands**

Create `cli/src/commands/session.rs`:
```rust
use anyhow::Result;
use vibeshell_core::ipc::{IpcClient, IpcMessage};

pub fn list() -> Result<()> {
    let response = IpcClient::send(&IpcMessage::ListSessions)?;

    match response {
        IpcMessage::SessionList { sessions } => {
            if sessions.is_empty() {
                println!("No active sessions");
            } else {
                println!("Active sessions:");
                for session in sessions {
                    println!("  {}", session);
                }
            }
        }
        IpcMessage::Error { message } => {
            eprintln!("Error: {}", message);
        }
        _ => {
            eprintln!("Unexpected response");
        }
    }

    Ok(())
}

pub fn attach(session_id: &str) -> Result<()> {
    let response = IpcClient::send(&IpcMessage::AttachSession {
        session_id: session_id.to_string(),
    })?;

    match response {
        IpcMessage::Ok => {
            println!("Attached to session: {}", session_id);
            // TODO: Start interactive mode
        }
        IpcMessage::Error { message } => {
            eprintln!("Error: {}", message);
        }
        _ => {
            eprintln!("Unexpected response");
        }
    }

    Ok(())
}

pub fn kill(session_id: &str) -> Result<()> {
    let response = IpcClient::send(&IpcMessage::KillSession {
        session_id: session_id.to_string(),
    })?;

    match response {
        IpcMessage::Ok => {
            println!("Session killed: {}", session_id);
        }
        IpcMessage::Error { message } => {
            eprintln!("Error: {}", message);
        }
        _ => {
            eprintln!("Unexpected response");
        }
    }

    Ok(())
}
```

**Step 3: Create SSH command**

Create `cli/src/commands/ssh.rs`:
```rust
use anyhow::Result;
use vibeshell_core::ipc::{IpcClient, IpcMessage};

pub fn connect(server_name: &str) -> Result<()> {
    let response = IpcClient::send(&IpcMessage::CreateSession {
        server_name: server_name.to_string(),
    })?;

    match response {
        IpcMessage::SessionCreated { session_id } => {
            println!("Connected to {} (session: {})", server_name, session_id);
            // TODO: Enter interactive mode
        }
        IpcMessage::Error { message } => {
            eprintln!("Error: {}", message);
        }
        _ => {
            eprintln!("Unexpected response");
        }
    }

    Ok(())
}
```

**Step 4: Update CLI main.rs**

```rust
use clap::{Parser, Subcommand};
use anyhow::Result;

mod commands;

#[derive(Parser)]
#[command(name = "vshell")]
#[command(about = "VibeShell - High-performance SSH/SFTP terminal")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show version
    Version,

    /// Connect to a server via SSH
    Ssh {
        /// Server name
        server: String,
    },

    /// List active sessions
    Sessions,

    /// Attach to an existing session
    Attach {
        /// Session ID
        session_id: String,
    },

    /// Kill a session
    Kill {
        /// Session ID (or --all)
        session_id: Option<String>,

        /// Kill all sessions
        #[arg(long)]
        all: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Version) => {
            println!("vshell {}", vibeshell_core::version());
        }
        Some(Commands::Ssh { server }) => {
            commands::ssh::connect(&server)?;
        }
        Some(Commands::Sessions) => {
            commands::session::list()?;
        }
        Some(Commands::Attach { session_id }) => {
            commands::session::attach(&session_id)?;
        }
        Some(Commands::Kill { session_id, all }) => {
            if all {
                println!("Killing all sessions...");
                // TODO: Implement kill all
            } else if let Some(id) = session_id {
                commands::session::kill(&id)?;
            } else {
                eprintln!("Specify a session ID or use --all");
            }
        }
        None => {
            println!("VibeShell - Run 'vshell --help' for usage");
        }
    }

    Ok(())
}
```

**Step 5: Build CLI**

Run:
```bash
cargo build -p vshell
./target/debug/vshell --help
```
Expected: Help message displays all commands

**Step 6: Commit**

```bash
git add .
git commit -m "feat: implement CLI session commands"
```

---

## Phase 4: SFTP Implementation

### Task 4.1: Implement SFTP Client

**Files:**
- Create: `src-tauri/src/sftp/mod.rs`
- Create: `src-tauri/src/sftp/client.rs`
- Create: `src-tauri/src/sftp/operations.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create SFTP module**

Create `src-tauri/src/sftp/mod.rs`:
```rust
pub mod client;
pub mod operations;

pub use client::SftpClient;
pub use operations::*;
```

**Step 2: Create SFTP client**

Create `src-tauri/src/sftp/client.rs`:
```rust
use anyhow::{Result, anyhow};
use russh::client;
use russh_sftp::client::SftpSession;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SftpClient {
    sftp: Arc<Mutex<Option<SftpSession>>>,
}

impl SftpClient {
    pub fn new() -> Self {
        Self {
            sftp: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn connect(&self, session: &client::Handle<impl client::Handler>) -> Result<()> {
        let channel = session.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;

        let sftp = SftpSession::new(channel.into_stream()).await?;

        let mut guard = self.sftp.lock().await;
        *guard = Some(sftp);

        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        let mut guard = self.sftp.lock().await;
        *guard = None;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        // Check synchronously if possible
        false // Simplified for now
    }
}
```

**Step 3: Create SFTP operations**

Create `src-tauri/src/sftp/operations.rs`:
```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
    pub permissions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub id: String,
    pub filename: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

// Operations will be implemented with actual SFTP session
pub async fn list_directory(_path: &str) -> Result<Vec<FileInfo>> {
    // TODO: Implement with SftpSession
    Ok(vec![])
}

pub async fn upload(_local_path: &str, _remote_path: &str) -> Result<TransferProgress> {
    // TODO: Implement
    Ok(TransferProgress {
        id: uuid::Uuid::new_v4().to_string(),
        filename: String::new(),
        total_bytes: 0,
        transferred_bytes: 0,
        status: TransferStatus::Pending,
    })
}

pub async fn download(_remote_path: &str, _local_path: &str) -> Result<TransferProgress> {
    // TODO: Implement
    Ok(TransferProgress {
        id: uuid::Uuid::new_v4().to_string(),
        filename: String::new(),
        total_bytes: 0,
        transferred_bytes: 0,
        status: TransferStatus::Pending,
    })
}

pub async fn mkdir(_path: &str) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub async fn remove(_path: &str) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub async fn rename(_from: &str, _to: &str) -> Result<()> {
    // TODO: Implement
    Ok(())
}
```

**Step 4: Export SFTP module**

Update `src-tauri/src/lib.rs`:
```rust
pub mod storage;
pub mod ssh;
pub mod session;
pub mod commands;
pub mod ipc;
pub mod sftp;

pub use storage::Database;
pub use session::SessionManager;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 5: Add russh-sftp dependency**

Add to `src-tauri/Cargo.toml`:
```toml
[dependencies]
russh-sftp = "2"
```

**Step 6: Build to verify**

Run:
```bash
cargo build -p vibeshell
```
Expected: Build succeeds

**Step 7: Commit**

```bash
git add .
git commit -m "feat: implement SFTP client structure"
```

---

## Phase 5: MCP Integration

### Task 5.1: Create MCP Server

**Files:**
- Create: `src-tauri/src/mcp/mod.rs`
- Create: `src-tauri/src/mcp/server.rs`
- Create: `src-tauri/src/mcp/tools.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add MCP dependencies**

Add to `src-tauri/Cargo.toml`:
```toml
[dependencies]
mcp-server = "0.1"  # Or implement custom JSON-RPC
axum = "0.7"
tower = "0.4"
```

**Step 2: Create MCP module**

Create `src-tauri/src/mcp/mod.rs`:
```rust
pub mod server;
pub mod tools;

pub use server::McpServer;
```

**Step 3: Create MCP tool definitions**

Create `src-tauri/src/mcp/tools.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // Server Management
        ToolDefinition {
            name: "server_list".to_string(),
            description: "List all configured servers".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "group": { "type": "string", "description": "Filter by group" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        ToolDefinition {
            name: "server_add".to_string(),
            description: "Add a new server".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["name", "host", "username"],
                "properties": {
                    "name": { "type": "string" },
                    "host": { "type": "string" },
                    "port": { "type": "integer", "default": 22 },
                    "username": { "type": "string" }
                }
            }),
        },
        // Session Management
        ToolDefinition {
            name: "session_list".to_string(),
            description: "List all active sessions".to_string(),
            parameters: serde_json::json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "session_create".to_string(),
            description: "Create a new SSH session".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["server"],
                "properties": {
                    "server": { "type": "string", "description": "Server name or ID" }
                }
            }),
        },
        ToolDefinition {
            name: "session_kill".to_string(),
            description: "Kill a session".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["session_id"],
                "properties": {
                    "session_id": { "type": "string" }
                }
            }),
        },
        // Command Execution
        ToolDefinition {
            name: "exec".to_string(),
            description: "Execute a command in a session".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["session_id", "command"],
                "properties": {
                    "session_id": { "type": "string" },
                    "command": { "type": "string" }
                }
            }),
        },
        // SFTP
        ToolDefinition {
            name: "sftp_ls".to_string(),
            description: "List remote directory".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["session_id", "path"],
                "properties": {
                    "session_id": { "type": "string" },
                    "path": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "sftp_upload".to_string(),
            description: "Upload a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["session_id", "local_path", "remote_path"],
                "properties": {
                    "session_id": { "type": "string" },
                    "local_path": { "type": "string" },
                    "remote_path": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "sftp_download".to_string(),
            description: "Download a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "required": ["session_id", "remote_path", "local_path"],
                "properties": {
                    "session_id": { "type": "string" },
                    "remote_path": { "type": "string" },
                    "local_path": { "type": "string" }
                }
            }),
        },
    ]
}
```

**Step 4: Create MCP server**

Create `src-tauri/src/mcp/server.rs`:
```rust
use anyhow::Result;
use axum::{
    extract::State,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::session::SessionManager;
use crate::storage::Database;
use super::tools::{get_tool_definitions, ToolDefinition};

#[derive(Clone)]
pub struct McpState {
    pub database: Arc<Database>,
    pub session_manager: Arc<SessionManager>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

pub struct McpServer {
    state: McpState,
}

impl McpServer {
    pub fn new(database: Arc<Database>, session_manager: Arc<SessionManager>) -> Self {
        Self {
            state: McpState {
                database,
                session_manager,
            },
        }
    }

    pub async fn run(&self, port: u16) -> Result<()> {
        let app = Router::new()
            .route("/", post(handle_request))
            .with_state(self.state.clone());

        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
        println!("MCP Server listening on port {}", port);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn handle_request(
    State(state): State<McpState>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let result = match request.method.as_str() {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "vibeshell",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(serde_json::json!({
            "tools": get_tool_definitions()
        })),
        "tools/call" => handle_tool_call(&state, request.params).await,
        _ => Err(JsonRpcError {
            code: -32601,
            message: format!("Method not found: {}", request.method),
        }),
    };

    Json(JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: request.id,
        result: result.ok(),
        error: result.err(),
    })
}

async fn handle_tool_call(
    state: &McpState,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, JsonRpcError> {
    let params = params.ok_or(JsonRpcError {
        code: -32602,
        message: "Missing params".to_string(),
    })?;

    let tool_name = params.get("name")
        .and_then(|v| v.as_str())
        .ok_or(JsonRpcError {
            code: -32602,
            message: "Missing tool name".to_string(),
        })?;

    let arguments = params.get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    match tool_name {
        "server_list" => {
            let servers = state.database.server_list(None, None)
                .map_err(|e| JsonRpcError {
                    code: -32000,
                    message: e.to_string(),
                })?;
            Ok(serde_json::json!({ "content": [{ "type": "text", "text": serde_json::to_string(&servers).unwrap() }] }))
        }
        "session_list" => {
            let sessions = state.session_manager.list().await;
            Ok(serde_json::json!({ "content": [{ "type": "text", "text": serde_json::to_string(&sessions).unwrap() }] }))
        }
        _ => Err(JsonRpcError {
            code: -32601,
            message: format!("Unknown tool: {}", tool_name),
        }),
    }
}
```

**Step 5: Export MCP module**

Update `src-tauri/src/lib.rs`:
```rust
pub mod storage;
pub mod ssh;
pub mod session;
pub mod commands;
pub mod ipc;
pub mod sftp;
pub mod mcp;

pub use storage::Database;
pub use session::SessionManager;
pub use mcp::McpServer;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 6: Add CLI command for MCP server**

Update `cli/src/main.rs` to add:
```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Start MCP server
    McpServer {
        /// Port to listen on
        #[arg(long, default_value = "3000")]
        port: u16,
    },
}

// In main():
Some(Commands::McpServer { port }) => {
    println!("Starting MCP server on port {}...", port);
    // TODO: Start server
}
```

**Step 7: Build to verify**

Run:
```bash
cargo build -p vibeshell
```
Expected: Build succeeds

**Step 8: Commit**

```bash
git add .
git commit -m "feat: implement MCP server with tool definitions"
```

---

### Task 5.2: Implement One-Click Installation

**Files:**
- Create: `src-tauri/src/install/mod.rs`
- Create: `src-tauri/src/install/detector.rs`
- Create: `src-tauri/src/install/installer.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create install module**

Create `src-tauri/src/install/mod.rs`:
```rust
pub mod detector;
pub mod installer;

pub use detector::*;
pub use installer::*;
```

**Step 2: Create tool detector**

Create `src-tauri/src/install/detector.rs`:
```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTool {
    pub id: String,
    pub name: String,
    pub config_path: PathBuf,
    pub installed: bool,
    pub vibeshell_installed: bool,
}

pub fn detect_ai_tools() -> Vec<AiTool> {
    let home = dirs::home_dir().unwrap_or_default();

    vec![
        AiTool {
            id: "claude-code".to_string(),
            name: "Claude Code".to_string(),
            config_path: home.join(".claude").join("mcp.json"),
            installed: home.join(".claude").exists(),
            vibeshell_installed: false,
        },
        AiTool {
            id: "cursor".to_string(),
            name: "Cursor".to_string(),
            config_path: home.join(".cursor").join("mcp.json"),
            installed: home.join(".cursor").exists(),
            vibeshell_installed: false,
        },
        AiTool {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            config_path: home.join(".codex").join("config.json"),
            installed: home.join(".codex").exists(),
            vibeshell_installed: false,
        },
        AiTool {
            id: "opencode".to_string(),
            name: "Open Code".to_string(),
            config_path: home.join(".opencode").join("mcp.json"),
            installed: home.join(".opencode").exists(),
            vibeshell_installed: false,
        },
    ].into_iter()
        .map(|mut tool| {
            tool.vibeshell_installed = check_vibeshell_installed(&tool.config_path);
            tool
        })
        .collect()
}

fn check_vibeshell_installed(config_path: &PathBuf) -> bool {
    if let Ok(content) = std::fs::read_to_string(config_path) {
        content.contains("vibeshell")
    } else {
        false
    }
}
```

**Step 3: Create installer**

Create `src-tauri/src/install/installer.rs`:
```rust
use anyhow::{Result, anyhow};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const VIBESHELL_MCP_CONFIG: &str = r#"{
    "command": "vshell",
    "args": ["mcp-server"],
    "description": "SSH/SFTP server management via VibeShell"
}"#;

pub fn install_to_tool(tool_id: &str, config_path: &PathBuf) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Read existing config or create new
    let mut config: Value = if config_path.exists() {
        let content = fs::read_to_string(config_path)?;
        serde_json::from_str(&content).unwrap_or(json!({}))
    } else {
        json!({})
    };

    // Backup existing config
    if config_path.exists() {
        let backup_path = config_path.with_extension("json.bak");
        fs::copy(config_path, backup_path)?;
    }

    // Add VibeShell configuration
    let vibeshell_config: Value = serde_json::from_str(VIBESHELL_MCP_CONFIG)?;

    if let Some(obj) = config.as_object_mut() {
        // Handle different config formats
        match tool_id {
            "claude-code" | "cursor" | "opencode" => {
                // MCP format: { "mcpServers": { "vibeshell": {...} } }
                if !obj.contains_key("mcpServers") {
                    obj.insert("mcpServers".to_string(), json!({}));
                }
                if let Some(mcp_servers) = obj.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
                    mcp_servers.insert("vibeshell".to_string(), vibeshell_config);
                }
            }
            "codex" => {
                // Codex format may differ
                obj.insert("vibeshell".to_string(), vibeshell_config);
            }
            _ => {
                return Err(anyhow!("Unknown tool: {}", tool_id));
            }
        }
    }

    // Write config
    let content = serde_json::to_string_pretty(&config)?;
    fs::write(config_path, content)?;

    Ok(())
}

pub fn uninstall_from_tool(tool_id: &str, config_path: &PathBuf) -> Result<()> {
    if !config_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(config_path)?;
    let mut config: Value = serde_json::from_str(&content)?;

    if let Some(obj) = config.as_object_mut() {
        match tool_id {
            "claude-code" | "cursor" | "opencode" => {
                if let Some(mcp_servers) = obj.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
                    mcp_servers.remove("vibeshell");
                }
            }
            "codex" => {
                obj.remove("vibeshell");
            }
            _ => {}
        }
    }

    let content = serde_json::to_string_pretty(&config)?;
    fs::write(config_path, content)?;

    Ok(())
}
```

**Step 4: Add dirs dependency**

Add to `src-tauri/Cargo.toml`:
```toml
[dependencies]
dirs = "5"
```

**Step 5: Export install module**

Update `src-tauri/src/lib.rs`:
```rust
pub mod storage;
pub mod ssh;
pub mod session;
pub mod commands;
pub mod ipc;
pub mod sftp;
pub mod mcp;
pub mod install;

pub use storage::Database;
pub use session::SessionManager;
pub use mcp::McpServer;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 6: Add install commands to CLI**

Update `cli/src/main.rs`:
```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Install VibeShell to AI tools
    Install {
        /// Tool name (claude-code, cursor, codex, opencode) or "all"
        tool: String,
    },

    /// Uninstall VibeShell from AI tools
    Uninstall {
        /// Tool name
        tool: String,
    },
}

// In main():
Some(Commands::Install { tool }) => {
    use vibeshell_core::install::{detect_ai_tools, install_to_tool};

    let tools = detect_ai_tools();

    if tool == "all" {
        for t in tools.iter().filter(|t| t.installed) {
            match install_to_tool(&t.id, &t.config_path) {
                Ok(_) => println!("✅ Installed to {}", t.name),
                Err(e) => eprintln!("❌ Failed to install to {}: {}", t.name, e),
            }
        }
    } else if let Some(t) = tools.iter().find(|t| t.id == tool) {
        install_to_tool(&t.id, &t.config_path)?;
        println!("✅ Installed to {}", t.name);
    } else {
        eprintln!("Unknown tool: {}", tool);
    }
}
Some(Commands::Uninstall { tool }) => {
    use vibeshell_core::install::{detect_ai_tools, uninstall_from_tool};

    let tools = detect_ai_tools();
    if let Some(t) = tools.iter().find(|t| t.id == tool) {
        uninstall_from_tool(&t.id, &t.config_path)?;
        println!("✅ Uninstalled from {}", t.name);
    }
}
```

**Step 7: Build and test**

Run:
```bash
cargo build -p vshell
./target/debug/vshell install --help
```
Expected: Help shows install options

**Step 8: Commit**

```bash
git add .
git commit -m "feat: implement one-click installation for AI tools"
```

---

## Phase 6: Frontend UI

### Task 6.1: Create Server List Component

**Files:**
- Create: `src/components/ServerList/ServerList.tsx`
- Create: `src/components/ServerList/ServerItem.tsx`
- Create: `src/components/ServerList/index.ts`
- Create: `src/stores/serverStore.ts`

**Step 1: Install shadcn/ui**

Run:
```bash
pnpm add @radix-ui/react-icons @radix-ui/react-slot class-variance-authority clsx tailwind-merge
pnpm add lucide-react
```

**Step 2: Create server store**

Create `src/stores/serverStore.ts`:
```typescript
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface Server {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: 'password' | 'key' | 'key_with_passphrase';
  groupId?: string;
  tags: string[];
}

export interface Group {
  id: string;
  name: string;
  parentId?: string;
  color: string;
}

interface ServerStore {
  servers: Server[];
  groups: Group[];
  loading: boolean;
  error: string | null;

  fetchServers: () => Promise<void>;
  fetchGroups: () => Promise<void>;
  addServer: (server: Omit<Server, 'id'>) => Promise<void>;
  deleteServer: (id: string) => Promise<void>;
}

export const useServerStore = create<ServerStore>((set, get) => ({
  servers: [],
  groups: [],
  loading: false,
  error: null,

  fetchServers: async () => {
    set({ loading: true, error: null });
    try {
      const servers = await invoke<Server[]>('server_list');
      set({ servers, loading: false });
    } catch (error) {
      set({ error: String(error), loading: false });
    }
  },

  fetchGroups: async () => {
    try {
      const groups = await invoke<Group[]>('group_list');
      set({ groups });
    } catch (error) {
      console.error('Failed to fetch groups:', error);
    }
  },

  addServer: async (server) => {
    try {
      await invoke('server_add', { server });
      await get().fetchServers();
    } catch (error) {
      set({ error: String(error) });
    }
  },

  deleteServer: async (id) => {
    try {
      await invoke('server_delete', { id });
      await get().fetchServers();
    } catch (error) {
      set({ error: String(error) });
    }
  },
}));
```

**Step 3: Create ServerItem component**

Create `src/components/ServerList/ServerItem.tsx`:
```tsx
import { Server } from '../../stores/serverStore';
import { Monitor, MoreVertical } from 'lucide-react';

interface ServerItemProps {
  server: Server;
  onConnect: (server: Server) => void;
  onDelete: (id: string) => void;
}

export function ServerItem({ server, onConnect, onDelete }: ServerItemProps) {
  return (
    <div
      className="flex items-center gap-3 px-3 py-2 hover:bg-gray-700 rounded cursor-pointer group"
      onClick={() => onConnect(server)}
    >
      <Monitor className="w-4 h-4 text-gray-400" />
      <div className="flex-1 min-w-0">
        <div className="text-sm text-white truncate">{server.name}</div>
        <div className="text-xs text-gray-500 truncate">
          {server.username}@{server.host}:{server.port}
        </div>
      </div>
      <button
        className="opacity-0 group-hover:opacity-100 p-1 hover:bg-gray-600 rounded"
        onClick={(e) => {
          e.stopPropagation();
          // Show context menu
        }}
      >
        <MoreVertical className="w-4 h-4 text-gray-400" />
      </button>
    </div>
  );
}
```

**Step 4: Create ServerList component**

Create `src/components/ServerList/ServerList.tsx`:
```tsx
import { useEffect } from 'react';
import { useServerStore, Server } from '../../stores/serverStore';
import { ServerItem } from './ServerItem';
import { Plus, FolderOpen } from 'lucide-react';

interface ServerListProps {
  onConnect: (server: Server) => void;
}

export function ServerList({ onConnect }: ServerListProps) {
  const { servers, groups, loading, fetchServers, fetchGroups, deleteServer } = useServerStore();

  useEffect(() => {
    fetchServers();
    fetchGroups();
  }, []);

  const groupedServers = groups.map(group => ({
    group,
    servers: servers.filter(s => s.groupId === group.id),
  }));

  const ungroupedServers = servers.filter(s => !s.groupId);

  return (
    <div className="flex flex-col h-full bg-gray-800">
      <div className="p-3 border-b border-gray-700">
        <h2 className="text-sm font-semibold text-gray-300">Servers</h2>
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        {loading ? (
          <div className="text-gray-500 text-sm p-3">Loading...</div>
        ) : (
          <>
            {groupedServers.map(({ group, servers }) => (
              <div key={group.id} className="mb-2">
                <div className="flex items-center gap-2 px-2 py-1 text-xs text-gray-400">
                  <FolderOpen className="w-3 h-3" style={{ color: group.color }} />
                  {group.name}
                </div>
                {servers.map(server => (
                  <ServerItem
                    key={server.id}
                    server={server}
                    onConnect={onConnect}
                    onDelete={deleteServer}
                  />
                ))}
              </div>
            ))}

            {ungroupedServers.length > 0 && (
              <div className="mt-2">
                {ungroupedServers.map(server => (
                  <ServerItem
                    key={server.id}
                    server={server}
                    onConnect={onConnect}
                    onDelete={deleteServer}
                  />
                ))}
              </div>
            )}
          </>
        )}
      </div>

      <div className="p-2 border-t border-gray-700">
        <button className="w-full flex items-center justify-center gap-2 px-3 py-2 bg-blue-600 hover:bg-blue-700 text-white text-sm rounded">
          <Plus className="w-4 h-4" />
          Add Server
        </button>
      </div>
    </div>
  );
}
```

**Step 5: Create index export**

Create `src/components/ServerList/index.ts`:
```typescript
export { ServerList } from './ServerList';
export { ServerItem } from './ServerItem';
```

**Step 6: Update App.tsx**

```tsx
import { useState } from 'react';
import { Terminal } from './components/Terminal';
import { ServerList } from './components/ServerList';
import { Server } from './stores/serverStore';

function App() {
  const [activeServer, setActiveServer] = useState<Server | null>(null);

  const handleConnect = (server: Server) => {
    setActiveServer(server);
    console.log('Connecting to:', server.name);
  };

  const handleData = (data: string) => {
    console.log('Terminal input:', data);
  };

  return (
    <div className="h-screen flex flex-col bg-gray-900">
      <header className="h-12 flex items-center px-4 bg-gray-800 border-b border-gray-700">
        <h1 className="text-white font-semibold">VibeShell</h1>
      </header>
      <div className="flex-1 flex">
        <aside className="w-64 border-r border-gray-700">
          <ServerList onConnect={handleConnect} />
        </aside>
        <main className="flex-1 p-2">
          <Terminal sessionId={activeServer?.id} onData={handleData} />
        </main>
      </div>
    </div>
  );
}

export default App;
```

**Step 7: Run and verify**

Run:
```bash
pnpm tauri dev
```
Expected: App shows server list sidebar and terminal

**Step 8: Commit**

```bash
git add .
git commit -m "feat: create server list UI component"
```

---

### Task 6.2: Create Settings Page with AI Tool Integration

**Files:**
- Create: `src/components/Settings/Settings.tsx`
- Create: `src/components/Settings/IntegrationCard.tsx`
- Create: `src/components/Settings/index.ts`
- Create: `src/stores/settingsStore.ts`

**Step 1: Create settings store**

Create `src/stores/settingsStore.ts`:
```typescript
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface AiTool {
  id: string;
  name: string;
  configPath: string;
  installed: boolean;
  vibeshellInstalled: boolean;
}

interface SettingsStore {
  aiTools: AiTool[];
  loading: boolean;

  fetchAiTools: () => Promise<void>;
  installTo: (toolId: string) => Promise<void>;
  uninstallFrom: (toolId: string) => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  aiTools: [],
  loading: false,

  fetchAiTools: async () => {
    set({ loading: true });
    try {
      const tools = await invoke<AiTool[]>('detect_ai_tools');
      set({ aiTools: tools, loading: false });
    } catch (error) {
      console.error('Failed to detect AI tools:', error);
      set({ loading: false });
    }
  },

  installTo: async (toolId) => {
    try {
      await invoke('install_to_tool', { toolId });
      await get().fetchAiTools();
    } catch (error) {
      console.error('Failed to install:', error);
    }
  },

  uninstallFrom: async (toolId) => {
    try {
      await invoke('uninstall_from_tool', { toolId });
      await get().fetchAiTools();
    } catch (error) {
      console.error('Failed to uninstall:', error);
    }
  },
}));
```

**Step 2: Create IntegrationCard component**

Create `src/components/Settings/IntegrationCard.tsx`:
```tsx
import { AiTool } from '../../stores/settingsStore';
import { Check, Download, X } from 'lucide-react';

interface IntegrationCardProps {
  tool: AiTool;
  onInstall: () => void;
  onUninstall: () => void;
}

export function IntegrationCard({ tool, onInstall, onUninstall }: IntegrationCardProps) {
  return (
    <div className="bg-gray-700 rounded-lg p-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-white font-medium">{tool.name}</h3>
          <p className="text-sm text-gray-400">
            {tool.installed ? 'Detected' : 'Not installed'}
          </p>
        </div>

        {tool.installed && (
          <div>
            {tool.vibeshellInstalled ? (
              <button
                onClick={onUninstall}
                className="flex items-center gap-2 px-3 py-1.5 bg-green-600 hover:bg-red-600 text-white text-sm rounded transition-colors"
              >
                <Check className="w-4 h-4" />
                Installed
              </button>
            ) : (
              <button
                onClick={onInstall}
                className="flex items-center gap-2 px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white text-sm rounded"
              >
                <Download className="w-4 h-4" />
                Install
              </button>
            )}
          </div>
        )}

        {!tool.installed && (
          <span className="text-gray-500 text-sm">Not available</span>
        )}
      </div>
    </div>
  );
}
```

**Step 3: Create Settings component**

Create `src/components/Settings/Settings.tsx`:
```tsx
import { useEffect } from 'react';
import { useSettingsStore } from '../../stores/settingsStore';
import { IntegrationCard } from './IntegrationCard';
import { Plug } from 'lucide-react';

export function Settings() {
  const { aiTools, loading, fetchAiTools, installTo, uninstallFrom } = useSettingsStore();

  useEffect(() => {
    fetchAiTools();
  }, []);

  return (
    <div className="p-6 max-w-2xl mx-auto">
      <h1 className="text-2xl font-bold text-white mb-6">Settings</h1>

      <section className="mb-8">
        <div className="flex items-center gap-2 mb-4">
          <Plug className="w-5 h-5 text-blue-400" />
          <h2 className="text-lg font-semibold text-white">AI Tool Integrations</h2>
        </div>

        <p className="text-gray-400 text-sm mb-4">
          Install VibeShell MCP server to your AI coding tools for SSH/SFTP management capabilities.
        </p>

        <div className="grid gap-3">
          {loading ? (
            <div className="text-gray-500">Detecting AI tools...</div>
          ) : (
            aiTools.map(tool => (
              <IntegrationCard
                key={tool.id}
                tool={tool}
                onInstall={() => installTo(tool.id)}
                onUninstall={() => uninstallFrom(tool.id)}
              />
            ))
          )}
        </div>

        <div className="mt-4 p-3 bg-gray-800 rounded text-sm text-gray-400">
          <strong>What gets installed:</strong>
          <ul className="mt-2 list-disc list-inside space-y-1">
            <li>MCP Server configuration pointing to VibeShell</li>
            <li>SSH management tools (server_list, exec, sftp_*, etc.)</li>
            <li>Session sharing capabilities</li>
          </ul>
        </div>
      </section>
    </div>
  );
}
```

**Step 4: Create index export**

Create `src/components/Settings/index.ts`:
```typescript
export { Settings } from './Settings';
```

**Step 5: Add Tauri commands for settings**

Create `src-tauri/src/commands/settings.rs`:
```rust
use tauri::State;
use std::sync::Arc;

use crate::install::{detect_ai_tools, install_to_tool, uninstall_from_tool, AiTool};

#[tauri::command]
pub fn detect_ai_tools_cmd() -> Vec<AiTool> {
    detect_ai_tools()
}

#[tauri::command]
pub fn install_to_tool_cmd(tool_id: String) -> Result<(), String> {
    let tools = detect_ai_tools();
    if let Some(tool) = tools.iter().find(|t| t.id == tool_id) {
        install_to_tool(&tool.id, &tool.config_path)
            .map_err(|e| e.to_string())
    } else {
        Err(format!("Tool not found: {}", tool_id))
    }
}

#[tauri::command]
pub fn uninstall_from_tool_cmd(tool_id: String) -> Result<(), String> {
    let tools = detect_ai_tools();
    if let Some(tool) = tools.iter().find(|t| t.id == tool_id) {
        uninstall_from_tool(&tool.id, &tool.config_path)
            .map_err(|e| e.to_string())
    } else {
        Err(format!("Tool not found: {}", tool_id))
    }
}
```

**Step 6: Register commands in main.rs**

Update `src-tauri/src/main.rs`:
```rust
use vibeshell_core::commands::{
    session_list, session_create, session_kill, session_kill_all,
    detect_ai_tools_cmd, install_to_tool_cmd, uninstall_from_tool_cmd,
};

// In invoke_handler:
.invoke_handler(tauri::generate_handler![
    session_list,
    session_create,
    session_kill,
    session_kill_all,
    detect_ai_tools_cmd,
    install_to_tool_cmd,
    uninstall_from_tool_cmd,
])
```

**Step 7: Build and test**

Run:
```bash
pnpm tauri dev
```
Expected: Settings page shows AI tool integration cards

**Step 8: Commit**

```bash
git add .
git commit -m "feat: create settings page with AI tool integration UI"
```

---

## Summary

This implementation plan covers the complete VibeShell application:

1. **Phase 1** - Project foundation with Tauri, SQLite, encryption
2. **Phase 2** - SSH connection with russh, session management, xterm.js
3. **Phase 3** - CLI ↔ GUI session sharing via IPC
4. **Phase 4** - SFTP client and file operations
5. **Phase 5** - MCP server and one-click AI tool installation
6. **Phase 6** - Frontend UI components

Each task is broken into bite-sized steps following TDD principles with exact file paths, code, and commands.
