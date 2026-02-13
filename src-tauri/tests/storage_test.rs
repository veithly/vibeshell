/// Integration tests for the storage layer (database CRUD operations)
/// Run with: cargo test --test storage_test

use rusqlite::Connection;

/// Helper to create an in-memory database with the same schema as production.
fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();

    // Core tables
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS groups (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS servers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            host TEXT NOT NULL,
            port INTEGER NOT NULL DEFAULT 22,
            username TEXT NOT NULL,
            auth_type TEXT NOT NULL DEFAULT 'password',
            key_path TEXT,
            group_id TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            jump_host_id TEXT,
            post_login_command TEXT,
            agent_forwarding INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS tunnel_configs (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            name TEXT NOT NULL,
            tunnel_type TEXT NOT NULL,
            listen_host TEXT NOT NULL DEFAULT '127.0.0.1',
            listen_port INTEGER NOT NULL,
            dest_host TEXT NOT NULL DEFAULT '',
            dest_port INTEGER NOT NULL DEFAULT 0,
            auto_start INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS command_snippets (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            command TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS recordings (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            server_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            ended_at INTEGER,
            file_size INTEGER NOT NULL DEFAULT 0
        );
        "
    )
    .unwrap();

    conn
}

#[test]
fn test_create_and_read_server() {
    let conn = setup_test_db();
    let now = chrono::Utc::now().timestamp();
    let id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO servers (id, name, host, port, username, auth_type, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![id, "test-server", "192.168.1.100", 22, "root", "password", now, now],
    )
    .unwrap();

    let name: String = conn
        .query_row("SELECT name FROM servers WHERE id = ?1", [&id], |row| {
            row.get(0)
        })
        .unwrap();

    assert_eq!(name, "test-server");
}

#[test]
fn test_create_tunnel_config() {
    let conn = setup_test_db();
    let now = chrono::Utc::now().timestamp();
    let server_id = uuid::Uuid::new_v4().to_string();
    let tunnel_id = uuid::Uuid::new_v4().to_string();

    // Create a server first
    conn.execute(
        "INSERT INTO servers (id, name, host, port, username, auth_type, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![server_id, "tunnel-server", "10.0.0.1", 22, "admin", "password", now, now],
    )
    .unwrap();

    // Create tunnel config
    conn.execute(
        "INSERT INTO tunnel_configs (id, server_id, name, tunnel_type, listen_host, listen_port, dest_host, dest_port, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![tunnel_id, server_id, "DB Tunnel", "local", "127.0.0.1", 3307, "localhost", 3306, now],
    )
    .unwrap();

    let name: String = conn
        .query_row("SELECT name FROM tunnel_configs WHERE id = ?1", [&tunnel_id], |row| {
            row.get(0)
        })
        .unwrap();

    assert_eq!(name, "DB Tunnel");
}

#[test]
fn test_create_command_snippet() {
    let conn = setup_test_db();
    let now = chrono::Utc::now().timestamp();
    let id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO command_snippets (id, name, command, description, tags, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, "Check Disk", "df -h", "Show disk usage", "disk,monitor", now, now],
    )
    .unwrap();

    let (name, cmd): (String, String) = conn
        .query_row(
            "SELECT name, command FROM command_snippets WHERE id = ?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(name, "Check Disk");
    assert_eq!(cmd, "df -h");
}

#[test]
fn test_create_recording() {
    let conn = setup_test_db();
    let now = chrono::Utc::now().timestamp();
    let id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO recordings (id, session_id, server_id, file_path, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, "session-123", "server-456", "/tmp/recording.cast", now],
    )
    .unwrap();

    let path: String = conn
        .query_row("SELECT file_path FROM recordings WHERE id = ?1", [&id], |row| {
            row.get(0)
        })
        .unwrap();

    assert_eq!(path, "/tmp/recording.cast");
}

#[test]
fn test_snippet_search() {
    let conn = setup_test_db();
    let now = chrono::Utc::now().timestamp();

    // Insert multiple snippets
    for (name, cmd, tags) in [
        ("Check Disk", "df -h", "disk,storage"),
        ("Memory Usage", "free -m", "memory,monitor"),
        ("Network Info", "ip addr", "network,config"),
    ] {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO command_snippets (id, name, command, description, tags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, name, cmd, "", tags, now, now],
        )
        .unwrap();
    }

    // Search by name
    let mut stmt = conn
        .prepare("SELECT name FROM command_snippets WHERE name LIKE ?1 OR command LIKE ?1 OR tags LIKE ?1")
        .unwrap();
    let results: Vec<String> = stmt
        .query_map(["%disk%"], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0], "Check Disk");
}

#[test]
fn test_server_with_jump_host() {
    let conn = setup_test_db();
    let now = chrono::Utc::now().timestamp();

    let bastion_id = uuid::Uuid::new_v4().to_string();
    let target_id = uuid::Uuid::new_v4().to_string();

    // Create bastion host
    conn.execute(
        "INSERT INTO servers (id, name, host, port, username, auth_type, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![bastion_id, "bastion", "bastion.example.com", 22, "admin", "key", now, now],
    )
    .unwrap();

    // Create target with jump host
    conn.execute(
        "INSERT INTO servers (id, name, host, port, username, auth_type, jump_host_id, agent_forwarding, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![target_id, "internal-db", "10.0.0.50", 22, "deploy", "password", bastion_id, 1, now, now],
    )
    .unwrap();

    let (jump_host, agent_fwd): (Option<String>, bool) = conn
        .query_row(
            "SELECT jump_host_id, agent_forwarding FROM servers WHERE id = ?1",
            [&target_id],
            |row| Ok((row.get(0)?, row.get::<_, i32>(1)? != 0)),
        )
        .unwrap();

    assert_eq!(jump_host, Some(bastion_id));
    assert!(agent_fwd);
}

#[test]
fn test_cascade_delete_tunnel_configs() {
    let conn = setup_test_db();
    let now = chrono::Utc::now().timestamp();
    let server_id = uuid::Uuid::new_v4().to_string();

    // Enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

    // Create server
    conn.execute(
        "INSERT INTO servers (id, name, host, port, username, auth_type, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![server_id, "deleteme", "1.2.3.4", 22, "root", "password", now, now],
    )
    .unwrap();

    // Create tunnel configs for the server
    for i in 0..3 {
        let tunnel_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO tunnel_configs (id, server_id, name, tunnel_type, listen_port, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![tunnel_id, server_id, format!("Tunnel {}", i), "local", 8080 + i, now],
        )
        .unwrap();
    }

    // Verify tunnels exist
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tunnel_configs WHERE server_id = ?1",
            [&server_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 3);

    // Delete server
    conn.execute("DELETE FROM servers WHERE id = ?1", [&server_id])
        .unwrap();

    // Tunnels should be cascade-deleted
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tunnel_configs WHERE server_id = ?1",
            [&server_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
