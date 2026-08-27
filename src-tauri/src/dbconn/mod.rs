//! Database connections: standalone connection profiles with native protocol
//! clients (PostgreSQL, MySQL), independent of SSH sessions.
//!
//! Passwords are encrypted at rest with a device-local key file using
//! AES-256-GCM. The key never leaves the device, so connection profiles are
//! intentionally not part of cloud sync.

use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::Value;
use tokio_postgres::config::Config as PgConfig;

const OP_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_MAX_ROWS: usize = 500;
const HARD_MAX_ROWS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DbEngine {
    Postgres,
    Mysql,
}

impl DbEngine {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "postgresql" | "postgres" => Ok(Self::Postgres),
            "mysql" | "mariadb" => Ok(Self::Mysql),
            other => Err(anyhow!("Unsupported database engine: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgresql",
            Self::Mysql => "mysql",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Postgres => 5432,
            Self::Mysql => 3306,
        }
    }
}

/// Connection coordinates. `password` is the decrypted plaintext, held only
/// in memory while an operation runs.
#[derive(Debug, Clone)]
pub struct DbEndpoint {
    pub engine: DbEngine,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub server_version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbColumnMeta {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbQueryResult {
    pub columns: Vec<String>,
    /// Cell values serialized as JSON scalars (null / number / string).
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: Option<u64>,
    pub duration_ms: u64,
    pub truncated: bool,
}

pub fn clamp_max_rows(requested: Option<usize>) -> usize {
    requested.unwrap_or(DEFAULT_MAX_ROWS).min(HARD_MAX_ROWS)
}

pub async fn test_connection(endpoint: &DbEndpoint) -> DbTestResult {
    let started = Instant::now();
    let probe = async {
        match endpoint.engine {
            DbEngine::Postgres => pg_scalar(endpoint, "SELECT version();").await,
            DbEngine::Mysql => mysql_scalar(endpoint, "SELECT VERSION();").await,
        }
    };
    match tokio::time::timeout(OP_TIMEOUT, probe).await {
        Ok(Ok(version)) => DbTestResult {
            ok: true,
            latency_ms: started.elapsed().as_millis() as u64,
            server_version: Some(version),
            error: None,
        },
        Ok(Err(error)) => DbTestResult {
            ok: false,
            latency_ms: started.elapsed().as_millis() as u64,
            server_version: None,
            error: Some(error.to_string()),
        },
        Err(_) => DbTestResult {
            ok: false,
            latency_ms: started.elapsed().as_millis() as u64,
            server_version: None,
            error: Some("Timed out after 15s".to_string()),
        },
    }
}

pub async fn list_databases(endpoint: &DbEndpoint) -> Result<Vec<String>> {
    let sql = match endpoint.engine {
        DbEngine::Postgres => {
            "SELECT datname FROM pg_database WHERE NOT datistemplate ORDER BY datname"
        }
        DbEngine::Mysql => "SHOW DATABASES",
    };
    let (_, rows, _) = run_query(endpoint, sql, 1000).await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| row.first().and_then(Value::as_str).map(str::to_string))
        .collect())
}

pub async fn list_tables(endpoint: &DbEndpoint, database: &str) -> Result<Vec<String>> {
    let mut ep = endpoint.clone();
    ep.database = Some(database.to_string());
    let sql = match endpoint.engine {
        DbEngine::Postgres => {
            "SELECT schemaname || '.' || tablename FROM pg_tables \
             WHERE schemaname NOT IN ('pg_catalog', 'information_schema') \
             ORDER BY schemaname, tablename"
        }
        DbEngine::Mysql => "SHOW FULL TABLES WHERE Table_type = 'BASE TABLE'",
    };
    let (_, rows, _) = run_query(&ep, sql, 5000).await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| row.first().and_then(Value::as_str).map(str::to_string))
        .collect())
}

pub async fn list_columns(
    endpoint: &DbEndpoint,
    database: &str,
    table: &str,
) -> Result<Vec<DbColumnMeta>> {
    let mut ep = endpoint.clone();
    ep.database = Some(database.to_string());
    // Table names come from our own metadata queries; still quote them so a
    // hostile identifier cannot rewrite the statement.
    let safe_table = table.replace('\'', "''");
    let safe_database = database.replace('\'', "''");
    let (_, rows, _) = match endpoint.engine {
        DbEngine::Postgres => {
            let sql = format!(
                "SELECT column_name, data_type, is_nullable, column_default \
                 FROM information_schema.columns \
                 WHERE (table_schema || '.' || table_name = '{safe_table}' \
                    OR table_name = '{safe_table}') \
                 ORDER BY table_schema, ordinal_position"
            );
            run_query(&ep, &sql, 2000).await?
        }
        DbEngine::Mysql => {
            let sql = format!(
                "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT \
                 FROM information_schema.columns \
                 WHERE TABLE_SCHEMA = '{safe_database}' AND TABLE_NAME = '{safe_table}' \
                 ORDER BY ORDINAL_POSITION"
            );
            run_query(&ep, &sql, 2000).await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| DbColumnMeta {
            name: value_str(&row, 0),
            data_type: value_str(&row, 1),
            nullable: value_str(&row, 2).eq_ignore_ascii_case("yes"),
            default_value: row.get(3).and_then(value_opt_str),
        })
        .collect())
}

pub async fn run_query(
    endpoint: &DbEndpoint,
    sql: &str,
    max_rows: usize,
) -> Result<(Vec<String>, Vec<Vec<Value>>, Option<u64>)> {
    let max_rows = clamp_max_rows(Some(max_rows));
    match endpoint.engine {
        DbEngine::Postgres => pg_query(endpoint, sql, max_rows).await,
        DbEngine::Mysql => mysql_query(endpoint, sql, max_rows).await,
    }
}

async fn pg_connect(endpoint: &DbEndpoint) -> Result<tokio_postgres::Client> {
    let mut config = PgConfig::new();
    config
        .host(&endpoint.host)
        .port(endpoint.port)
        .user(&endpoint.username);
    if !endpoint.password.is_empty() {
        config.password(&endpoint.password);
    }
    match &endpoint.database {
        Some(database) => {
            config.dbname(database);
        }
        None => {
            config.dbname("postgres");
        }
    }
    let (client, connection) = config
        .connect(tokio_postgres::NoTls)
        .await
        .map_err(|error| anyhow!("PostgreSQL connection failed: {error}"))?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            log::debug!("[DbConn] PostgreSQL connection task ended: {error}");
        }
    });
    Ok(client)
}

/// Simple-query protocol returns every value as text, which is exactly what a
/// data grid wants; command-complete messages expose rows_affected.
async fn pg_query(
    endpoint: &DbEndpoint,
    sql: &str,
    max_rows: usize,
) -> Result<(Vec<String>, Vec<Vec<Value>>, Option<u64>)> {
    let rows_future = async {
        let client = pg_connect(endpoint).await?;
        let messages = client.simple_query(sql).await?;
        let mut columns: Vec<String> = Vec::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut affected: Option<u64> = None;
        for message in messages {
            match message {
                tokio_postgres::SimpleQueryMessage::Row(row) => {
                    if columns.is_empty() {
                        columns = row
                            .columns()
                            .iter()
                            .map(|column| column.name().to_string())
                            .collect();
                    }
                    rows.push(
                        columns
                            .iter()
                            .enumerate()
                            .map(|(index, _)| match row.get(index) {
                                Some(text) => Value::String(text.to_string()),
                                None => Value::Null,
                            })
                            .collect(),
                    );
                    if rows.len() >= max_rows {
                        break;
                    }
                }
                tokio_postgres::SimpleQueryMessage::CommandComplete(count) => {
                    affected = Some(count);
                }
                _ => {}
            }
        }
        Ok::<_, anyhow::Error>((columns, rows, affected))
    };
    tokio::time::timeout(OP_TIMEOUT, rows_future)
        .await
        .map_err(|_| anyhow!("PostgreSQL query timed out after 15s"))?
}

async fn mysql_connect(
    endpoint: &DbEndpoint,
) -> Result<mysql_async::Conn> {
    let builder = mysql_async::OptsBuilder::default()
        .ip_or_hostname(endpoint.host.as_str())
        .tcp_port(endpoint.port)
        .user(Some(endpoint.username.as_str()))
        .pass(if endpoint.password.is_empty() {
            None
        } else {
            Some(endpoint.password.as_str())
        })
        .db_name(endpoint.database.as_deref());
    mysql_async::Conn::new(builder)
        .await
        .map_err(|error| anyhow!("MySQL connection failed: {error}"))
}

fn mysql_value_to_json(value: mysql_async::Value) -> Value {
    use mysql_async::Value as V;
    match value {
        V::NULL => Value::Null,
        V::Int(number) => Value::from(number),
        V::UInt(number) => Value::from(number),
        V::Float(number) => Value::from(number),
        V::Double(number) => Value::from(number),
        V::Bytes(bytes) => Value::String(String::from_utf8_lossy(&bytes).into_owned()),
        V::Date(year, month, day, hour, minute, second, micro) => {
            Value::String(format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{micro:06}"))
        }
        V::Time(negative, days, hours, minutes, seconds, micros) => {
            let sign = if negative { "-" } else { "" };
            Value::String(format!("{sign}{days}d {hours:02}:{minutes:02}:{seconds:02}.{micros:06}"))
        }
    }
}

/// The raw query path uses MySQL's text protocol, where every value arrives
/// as bytes. Column metadata tells us the real type so numeric cells surface
/// as JSON numbers in the data grid.
fn mysql_typed_value(value: mysql_async::Value, column_type: Option<mysql_async::consts::ColumnType>) -> Value {
    use mysql_async::Value as V;
    if let (V::Bytes(bytes), Some(kind)) = (&value, column_type) {
        let text = String::from_utf8_lossy(bytes);
        return match kind {
            mysql_async::consts::ColumnType::MYSQL_TYPE_TINY
            | mysql_async::consts::ColumnType::MYSQL_TYPE_SHORT
            | mysql_async::consts::ColumnType::MYSQL_TYPE_LONG
            | mysql_async::consts::ColumnType::MYSQL_TYPE_LONGLONG
            | mysql_async::consts::ColumnType::MYSQL_TYPE_INT24
            | mysql_async::consts::ColumnType::MYSQL_TYPE_YEAR => {
                match text.parse::<i64>() {
                    Ok(number) => Value::from(number),
                    Err(_) => Value::String(text.into_owned()),
                }
            }
            mysql_async::consts::ColumnType::MYSQL_TYPE_FLOAT
            | mysql_async::consts::ColumnType::MYSQL_TYPE_DOUBLE => {
                match text.parse::<f64>() {
                    Ok(number) => serde_json::Number::from_f64(number)
                        .map(Value::Number)
                        .unwrap_or_else(|| Value::String(text.into_owned())),
                    Err(_) => Value::String(text.into_owned()),
                }
            }
            _ => Value::String(text.into_owned()),
        };
    }
    mysql_value_to_json(value)
}

async fn mysql_query(
    endpoint: &DbEndpoint,
    sql: &str,
    max_rows: usize,
) -> Result<(Vec<String>, Vec<Vec<Value>>, Option<u64>)> {
    use mysql_async::prelude::Queryable;

    let run = async {
        let mut conn = mysql_connect(endpoint).await?;
        let mut result = conn
            .query_iter(sql)
            .await
            .map_err(|error| anyhow!("MySQL query failed: {error}"))?;
        let columns: Vec<String> = result
            .columns()
            .as_ref()
            .map(|cols| cols.iter().map(|column| column.name_str().to_string()).collect())
            .unwrap_or_default();
        let column_types: Vec<mysql_async::consts::ColumnType> = result
            .columns()
            .as_ref()
            .map(|cols| cols.iter().map(|column| column.column_type()).collect())
            .unwrap_or_default();
        let affected = result.affected_rows();
        let raw_rows: Vec<mysql_async::Row> = result.collect().await?;
        let rows = raw_rows
            .into_iter()
            .take(max_rows)
            .map(|row| {
                row.unwrap()
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        mysql_typed_value(value, column_types.get(index).copied())
                    })
                    .collect()
            })
            .collect();
        conn.disconnect().await.ok();
        let affected_note = if columns.is_empty() { Some(affected) } else { None };
        Ok::<_, anyhow::Error>((columns, rows, affected_note))
    };
    tokio::time::timeout(OP_TIMEOUT, run)
        .await
        .map_err(|_| anyhow!("MySQL query timed out after 15s"))?
}

async fn pg_scalar(endpoint: &DbEndpoint, sql: &str) -> Result<String> {
    let (columns, rows, _) = pg_query(endpoint, sql, 1).await?;
    let _ = columns;
    rows.into_iter()
        .next()
        .and_then(|row| row.into_iter().next())
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| anyhow!("Empty result"))
}

async fn mysql_scalar(endpoint: &DbEndpoint, sql: &str) -> Result<String> {
    let (columns, rows, _) = mysql_query(endpoint, sql, 1).await?;
    let _ = columns;
    rows.into_iter()
        .next()
        .and_then(|row| row.into_iter().next())
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| anyhow!("Empty result"))
}

fn value_str(row: &[Value], index: usize) -> String {
    row.get(index).and_then(Value::as_str).unwrap_or("").to_string()
}

fn value_opt_str(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

/// Encrypt a connection password for storage. Uses a device-local key file;
/// see module docs for why these profiles are not cloud-synced.
pub fn encrypt_password(password: &str) -> Result<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let crypto = device_crypto()?;
    let ciphertext = crypto.encrypt(password.as_bytes())?;
    Ok(STANDARD.encode(ciphertext))
}

pub fn decrypt_password(encrypted: &str) -> Result<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    if encrypted.is_empty() {
        return Ok(String::new());
    }
    let ciphertext = STANDARD
        .decode(encrypted)
        .context("Stored database password is not valid base64")?;
    let crypto = device_crypto()?;
    let plaintext = crypto
        .decrypt(&ciphertext)
        .context("Stored database password could not be decrypted on this device")?;
    Ok(String::from_utf8_lossy(&plaintext).into_owned())
}

fn device_crypto() -> Result<&'static crate::storage::crypto::Crypto> {
    use std::sync::OnceLock;
    static CRYPTO: OnceLock<crate::storage::crypto::Crypto> = OnceLock::new();
    if let Some(crypto) = CRYPTO.get() {
        return Ok(crypto);
    }
    let init = (|| {
        let app_dir = directories::ProjectDirs::from("com", "vibeshell", "VibeShell")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        std::fs::create_dir_all(&app_dir)?;
        let key_path = app_dir.join("dbconn.key");
        let salt_path = app_dir.join("dbconn.salt");
        let key_hex = match std::fs::read_to_string(&key_path) {
            Ok(existing) if existing.trim().len() == 64 => existing.trim().to_string(),
            _ => {
                use ring::rand::{SecureRandom, SystemRandom};
                let mut bytes = [0u8; 32];
                SystemRandom::new()
                    .fill(&mut bytes)
                    .map_err(|_| anyhow!("Failed to generate device key"))?;
                let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
                std::fs::write(&key_path, &hex)?;
                set_private(&key_path);
                hex
            }
        };
        let salt = match std::fs::read(&salt_path) {
            Ok(existing) if !existing.is_empty() => existing,
            _ => {
                let salt = crate::storage::crypto::Crypto::generate_salt();
                std::fs::write(&salt_path, &salt)?;
                set_private(&salt_path);
                salt
            }
        };
        crate::storage::crypto::Crypto::from_password(&key_hex, &salt)
    })();
    let crypto = init?;
    let _ = CRYPTO.set(crypto);
    CRYPTO.get().ok_or_else(|| anyhow!("Device crypto failed to initialize"))
}

#[cfg(unix)]
fn set_private(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        let _ = std::fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn set_private(_path: &std::path::Path) {}

#[cfg(test)]
mod engine_tests {
    use super::*;

    fn pg_endpoint() -> Option<DbEndpoint> {
        let host = std::env::var("VIBESHELL_TEST_PG_HOST").ok()?;
        Some(DbEndpoint {
            engine: DbEngine::Postgres,
            host,
            port: std::env::var("VIBESHELL_TEST_PG_PORT")
                .ok()
                .and_then(|port| port.parse().ok())
                .unwrap_or(5432),
            username: std::env::var("VIBESHELL_TEST_PG_USER").unwrap_or_else(|_| "postgres".into()),
            password: std::env::var("VIBESHELL_TEST_PG_PASSWORD").unwrap_or_default(),
            database: None,
        })
    }

    fn mysql_endpoint() -> Option<DbEndpoint> {
        let host = std::env::var("VIBESHELL_TEST_MYSQL_HOST").ok()?;
        Some(DbEndpoint {
            engine: DbEngine::Mysql,
            host,
            port: std::env::var("VIBESHELL_TEST_MYSQL_PORT")
                .ok()
                .and_then(|port| port.parse().ok())
                .unwrap_or(3306),
            username: std::env::var("VIBESHELL_TEST_MYSQL_USER").unwrap_or_else(|_| "root".into()),
            password: std::env::var("VIBESHELL_TEST_MYSQL_PASSWORD").unwrap_or_default(),
            database: None,
        })
    }

    #[tokio::test]
    #[ignore = "requires a reachable MySQL server (VIBESHELL_TEST_MYSQL_HOST)"]
    async fn mysql_round_trip() {
        let endpoint = mysql_endpoint().expect("VIBESHELL_TEST_MYSQL_HOST not set");
        let test = test_connection(&endpoint).await;
        assert!(test.ok, "connection failed: {:?}", test.error);

        run_query(&endpoint, "DROP DATABASE IF EXISTS vibeshell_engine_e2e", 10)
            .await
            .ok();
        run_query(
            &endpoint,
            "CREATE DATABASE vibeshell_engine_e2e",
            10,
        )
        .await
        .unwrap();
        run_query(
            &endpoint,
            "CREATE TABLE vibeshell_engine_e2e.items (id INT PRIMARY KEY, label TEXT)",
            10,
        )
        .await
        .unwrap();
        run_query(
            &endpoint,
            "INSERT INTO vibeshell_engine_e2e.items VALUES (1, 'hello'), (2, NULL)",
            10,
        )
        .await
        .unwrap();

        let tables = list_tables(&endpoint, "vibeshell_engine_e2e").await.unwrap();
        assert!(tables.iter().any(|table| table.contains("items")));

        let columns = list_columns(&endpoint, "vibeshell_engine_e2e", "items")
            .await
            .unwrap();
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].name, "id");

        let mut ep = endpoint.clone();
        ep.database = Some("vibeshell_engine_e2e".into());
        let (cols, rows, _) = run_query(
            &ep,
            "SELECT id, label FROM items ORDER BY id",
            10,
        )
        .await
        .unwrap();
        assert_eq!(cols, vec!["id", "label"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], serde_json::Value::from(1));
        assert_eq!(rows[1][1], serde_json::Value::Null);

        run_query(&endpoint, "DROP DATABASE vibeshell_engine_e2e", 10)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires a reachable PostgreSQL server (VIBESHELL_TEST_PG_HOST)"]
    async fn postgres_round_trip() {
        let endpoint = pg_endpoint().expect("VIBESHELL_TEST_PG_HOST not set");
        let test = test_connection(&endpoint).await;
        assert!(test.ok, "connection failed: {:?}", test.error);
        assert!(test.server_version.as_deref().unwrap_or("").contains("PostgreSQL"));

        let databases = list_databases(&endpoint).await.unwrap();
        assert!(databases.iter().any(|name| name == "postgres"));

        run_query(
            &endpoint,
            "CREATE TABLE vibeshell_engine_e2e (id integer, label text)",
            10,
        )
        .await
        .unwrap();
        let tables = list_tables(&endpoint, "postgres").await.unwrap();
        assert!(tables.iter().any(|table| table.contains("vibeshell_engine_e2e")));
        let columns = list_columns(&endpoint, "postgres", "vibeshell_engine_e2e")
            .await
            .unwrap();
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].name, "id");
        assert_eq!(columns[1].data_type, "text");
        run_query(&endpoint, "DROP TABLE vibeshell_engine_e2e", 10)
            .await
            .unwrap();

        let (columns, rows, _) = run_query(
            &endpoint,
            "SELECT 1 AS one, 'vibeshell' AS name, NULL AS nothing",
            10,
        )
        .await
        .unwrap();
        assert_eq!(columns, vec!["one", "name", "nothing"]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][1], Value::String("vibeshell".into()));
        assert_eq!(rows[0][2], Value::Null);
    }
}
