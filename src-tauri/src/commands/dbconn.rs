use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::dbconn::{
    self, clamp_max_rows, run_query, DbEndpoint, DbEngine, DbQueryResult, DbTestResult,
};
use crate::session::SessionManager;
use crate::storage::{Database, DatabaseConnection};

/// Connection profile as seen by the frontend. The password never crosses the
/// IPC boundary; `hasPassword` drives the edit dialog instead.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseConnectionView {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub has_password: bool,
    pub default_database: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_connected_at: Option<i64>,
}

impl From<&DatabaseConnection> for DatabaseConnectionView {
    fn from(connection: &DatabaseConnection) -> Self {
        Self {
            id: connection.id.clone(),
            name: connection.name.clone(),
            engine: connection.engine.clone(),
            host: connection.host.clone(),
            port: connection.port,
            username: connection.username.clone(),
            has_password: !connection.password_encrypted.is_empty(),
            default_database: connection.default_database.clone(),
            created_at: connection.created_at,
            updated_at: connection.updated_at,
            last_connected_at: connection.last_connected_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseConnectionInput {
    pub id: Option<String>,
    pub name: String,
    pub engine: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Empty string keeps the stored password on update; passwords are never
    /// returned to the frontend, so an untouched dialog submits "".
    pub password: Option<String>,
    pub default_database: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseConnectionIdRequest {
    pub connection_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseTablesRequest {
    pub connection_id: String,
    pub database: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseColumnsRequest {
    pub connection_id: String,
    pub database: String,
    pub table: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseQueryRequest {
    pub connection_id: String,
    pub database: Option<String>,
    pub sql: String,
    pub max_rows: Option<usize>,
}

/// A database found on an SSH server by the session probe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSuggestion {
    pub engine: String,
    pub port: u16,
    pub source: String,
    pub detail: String,
}

fn endpoint_for(connection: &DatabaseConnection, database: Option<&str>) -> Result<DbEndpoint, String> {
    Ok(DbEndpoint {
        engine: DbEngine::parse(&connection.engine).map_err(|error| error.to_string())?,
        host: connection.host.clone(),
        port: connection.port,
        username: connection.username.clone(),
        password: dbconn::decrypt_password(&connection.password_encrypted).map_err(|error| error.to_string())?,
        database: database
            .map(str::to_string)
            .or_else(|| connection.default_database.clone()),
    })
}

fn view_list(db: &Database) -> Result<Vec<DatabaseConnectionView>, String> {
    Ok(db
        .database_connection_list()
        .map_err(|error| error.to_string())?
        .iter()
        .map(DatabaseConnectionView::from)
        .collect())
}

#[tauri::command]
pub fn db_connection_list(db: State<'_, Arc<Database>>) -> Result<Vec<DatabaseConnectionView>, String> {
    view_list(&db)
}

#[tauri::command]
pub fn db_connection_save(
    db: State<'_, Arc<Database>>,
    input: DatabaseConnectionInput,
) -> Result<DatabaseConnectionView, String> {
    let engine = DbEngine::parse(&input.engine).map_err(|error| error.to_string())?;
    let now = chrono::Utc::now().timestamp();

    let existing = match &input.id {
        Some(id) => db
            .database_connection_get(id)
            .map_err(|error| error.to_string())?,
        None => None,
    };

    let submitted_password = input.password.as_deref().unwrap_or("");
    let password_encrypted = if !submitted_password.is_empty() {
        dbconn::encrypt_password(submitted_password).map_err(|error| error.to_string())?
    } else {
        existing
            .as_ref()
            .map(|stored| stored.password_encrypted.clone())
            .unwrap_or_default()
    };

    let connection = DatabaseConnection {
        id: existing.as_ref().map(|c| c.id.clone()).unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: input.name.trim().to_string(),
        engine: engine.as_str().to_string(),
        host: input.host.trim().to_string(),
        port: if input.port == 0 { engine.default_port() } else { input.port },
        username: input.username.trim().to_string(),
        password_encrypted,
        default_database: input
            .default_database
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        created_at: existing.as_ref().map(|c| c.created_at).unwrap_or(now),
        updated_at: now,
        last_connected_at: existing.as_ref().and_then(|c| c.last_connected_at),
    };

    db.database_connection_upsert(&connection)
        .map_err(|error| error.to_string())?;
    Ok(DatabaseConnectionView::from(&connection))
}

#[tauri::command]
pub fn db_connection_delete(
    db: State<'_, Arc<Database>>,
    request: DatabaseConnectionIdRequest,
) -> Result<(), String> {
    db.database_connection_delete(&request.connection_id)
        .map_err(|error| error.to_string())
}

/// Test a saved connection (or, via `db_connection_probe`, unsaved edits).
#[tauri::command]
pub async fn db_connection_test(
    db: State<'_, Arc<Database>>,
    request: DatabaseConnectionIdRequest,
) -> Result<DbTestResult, String> {
    let connection = db
        .database_connection_get(&request.connection_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Database connection not found".to_string())?;
    let endpoint = endpoint_for(&connection, None)?;
    let result = dbconn::test_connection(&endpoint).await;
    if result.ok {
        let _ = db.database_connection_touch(&connection.id, chrono::Utc::now().timestamp());
    }
    Ok(result)
}

#[tauri::command]
pub async fn db_connection_probe(input: DatabaseConnectionInput) -> Result<DbTestResult, String> {
    let engine = DbEngine::parse(&input.engine).map_err(|error| error.to_string())?;
    let endpoint = DbEndpoint {
        engine,
        host: input.host.trim().to_string(),
        port: if input.port == 0 { engine.default_port() } else { input.port },
        username: input.username.trim().to_string(),
        password: input.password.unwrap_or_default(),
        database: input.default_database.clone().filter(|value| !value.trim().is_empty()),
    };
    Ok(dbconn::test_connection(&endpoint).await)
}

#[tauri::command]
pub async fn db_connection_databases(
    db: State<'_, Arc<Database>>,
    request: DatabaseConnectionIdRequest,
) -> Result<Vec<String>, String> {
    let connection = db
        .database_connection_get(&request.connection_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Database connection not found".to_string())?;
    let endpoint = endpoint_for(&connection, None)?;
    dbconn::list_databases(&endpoint)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn db_connection_tables(
    db: State<'_, Arc<Database>>,
    request: DatabaseTablesRequest,
) -> Result<Vec<String>, String> {
    let connection = db
        .database_connection_get(&request.connection_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Database connection not found".to_string())?;
    let endpoint = endpoint_for(&connection, None)?;
    dbconn::list_tables(&endpoint, &request.database)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn db_connection_columns(
    db: State<'_, Arc<Database>>,
    request: DatabaseColumnsRequest,
) -> Result<Vec<crate::dbconn::DbColumnMeta>, String> {
    let connection = db
        .database_connection_get(&request.connection_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Database connection not found".to_string())?;
    let endpoint = endpoint_for(&connection, None)?;
    dbconn::list_columns(&endpoint, &request.database, &request.table)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn db_connection_query(
    db: State<'_, Arc<Database>>,
    request: DatabaseQueryRequest,
) -> Result<DbQueryResult, String> {
    let connection = db
        .database_connection_get(&request.connection_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Database connection not found".to_string())?;
    let endpoint = endpoint_for(&connection, request.database.as_deref())?;
    let max_rows = clamp_max_rows(request.max_rows);
    let started = std::time::Instant::now();
    let (columns, rows, rows_affected) = run_query(&endpoint, request.sql.trim(), max_rows)
        .await
        .map_err(|error| error.to_string())?;
    let truncated = rows.len() >= max_rows;
    let _ = db.database_connection_touch(&connection.id, chrono::Utc::now().timestamp());
    Ok(DbQueryResult {
        columns,
        rows,
        rows_affected,
        duration_ms: started.elapsed().as_millis() as u64,
        truncated,
    })
}

/// Scan an SSH session's server for listening database ports and database
/// containers, so connections can be added with one click.
#[tauri::command]
pub async fn db_session_detect(
    manager: State<'_, Arc<SessionManager>>,
    request: crate::commands::SessionIdRequest,
) -> Result<Vec<DatabaseSuggestion>, String> {
    let session = manager
        .get(&request.session_id)
        .await
        .ok_or_else(|| "Session not found".to_string())?;

    let script = r#"(ss -ltn 2>/dev/null || netstat -ltn 2>/dev/null) | grep -E ':(5432|3306|6379) ' ; \
echo === ; docker ps --format '{{.Names}}\t{{.Image}}\t{{.Ports}}' 2>/dev/null"#;
    let output = session
        .exec_command_with_stdin(script, None)
        .await
        .map_err(|error| format!("Probe failed: {error}"))?;

    let mut suggestions: Vec<DatabaseSuggestion> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in output.lines() {
        let line = line.trim();
        if line == "===" || line.is_empty() {
            continue;
        }
        if line.contains('\t') {
            let mut parts = line.split('\t');
            let name = parts.next().unwrap_or("");
            let image = parts.next().unwrap_or("");
            let ports = parts.next().unwrap_or("");
            let engine = if image.to_lowercase().contains("postgres") {
                "postgresql"
            } else if image.to_lowercase().contains("mysql") || image.to_lowercase().contains("mariadb") {
                "mysql"
            } else if image.to_lowercase().contains("redis") {
                "redis"
            } else {
                continue;
            };
            let port = ports
                .split(',')
                .find_map(|mapping| {
                    let published = mapping.split("->").next()?.trim();
                    let host_part = published.rsplit(':').next()?.trim();
                    let candidate = if host_part.contains(':') {
                        host_part.rsplit(':').next()?.trim()
                    } else {
                        host_part
                    };
                    candidate.parse::<u16>().ok()
                })
                .unwrap_or(match engine {
                    "postgresql" => 5432,
                    "mysql" => 3306,
                    _ => 6379,
                });
            if seen.insert(format!("docker:{engine}:{port}")) {
                suggestions.push(DatabaseSuggestion {
                    engine: engine.to_string(),
                    port,
                    source: "docker".to_string(),
                    detail: format!("{name} ({image})"),
                });
            }
            continue;
        }
        for (port_text, engine) in [("5432", "postgresql"), ("3306", "mysql"), ("6379", "redis")] {
            if line.ends_with(&format!(":{port_text}")) || line.contains(&format!(":{port_text} ")) {
                if seen.insert(format!("tcp:{engine}")) {
                    suggestions.push(DatabaseSuggestion {
                        engine: engine.to_string(),
                        port: port_text.parse().unwrap_or_default(),
                        source: "tcp".to_string(),
                        detail: format!("listening :{port_text}"),
                    });
                }
            }
        }
    }
    Ok(suggestions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_parse_listener_and_docker_output() {
        let db = crate::storage::Database::new_at(std::env::temp_dir().join(format!(
            "dbconn-test-{}.db",
            uuid::Uuid::new_v4()
        )))
        .unwrap();
        let connection = DatabaseConnection {
            id: "c1".to_string(),
            name: "Prod".to_string(),
            engine: "postgresql".to_string(),
            host: "127.0.0.1".to_string(),
            port: 0,
            username: "postgres".to_string(),
            password_encrypted: String::new(),
            default_database: None,
            created_at: 1,
            updated_at: 1,
            last_connected_at: None,
        };
        db.database_connection_upsert(&connection).unwrap();
        let stored = db.database_connection_get("c1").unwrap().unwrap();
        assert_eq!(stored.engine, "postgresql");

        let view = DatabaseConnectionView::from(&stored);
        assert!(!view.has_password);
        assert_eq!(view.name, "Prod");

        db.database_connection_delete("c1").unwrap();
        assert!(db.database_connection_get("c1").unwrap().is_none());
    }
}
