//! MCP Server Implementation
//!
//! Implements a JSON-RPC 2.0 server using axum that exposes VibeShell's
//! SSH/SFTP functionality following the Model Context Protocol (MCP).

use std::sync::Arc;

use anyhow::Result;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::remote_tools::{build_remote_rg_command, RemoteSearchOptions};
use crate::session::SessionManager;
use crate::sftp::helpers::{
    resolve_remote_path, resolve_remote_upload_path, sftp_mkdir_recursive, sftp_remove_recursive,
    write_remote_file,
};
use crate::sftp::{
    effective_directory_transfer_options, transfer_directory_to_sftp, DirectoryTransferMode,
};
use crate::storage::models::{AuthType, Server};
use crate::storage::Database;

use super::tools::get_tool_definitions;

/// MCP protocol version
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Server name for MCP identification
const SERVER_NAME: &str = "vibeshell";

/// Server version for MCP identification
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared state for the MCP server
#[derive(Clone)]
pub struct McpState {
    /// Database for server configurations
    pub database: Arc<Database>,
    /// Session manager for SSH sessions
    pub session_manager: Arc<SessionManager>,
}

/// MCP Server
pub struct McpServer {
    state: McpState,
}

impl McpServer {
    /// Create a new MCP server with the given database and session manager
    pub fn new(database: Arc<Database>, session_manager: Arc<SessionManager>) -> Self {
        Self {
            state: McpState {
                database,
                session_manager,
            },
        }
    }

    /// Run the MCP server on the specified port
    pub async fn run(&self, port: u16) -> Result<()> {
        let app = Router::new()
            .route("/", post(handle_jsonrpc))
            .route("/mcp", post(handle_jsonrpc))
            .with_state(self.state.clone());

        let addr = format!("127.0.0.1:{}", port);
        println!("MCP server listening on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

// === JSON-RPC 2.0 Types ===

/// JSON-RPC 2.0 Request
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: String, data: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data,
            }),
        }
    }
}

// JSON-RPC error codes
#[allow(dead_code)]
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
#[allow(dead_code)]
const INTERNAL_ERROR: i32 = -32603;

// === Request Handler ===

async fn handle_jsonrpc(
    State(state): State<McpState>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            Json(JsonRpcResponse::error(
                request.id,
                INVALID_REQUEST,
                "Invalid JSON-RPC version".to_string(),
                None,
            )),
        );
    }

    // Route to appropriate handler
    let result = match request.method.as_str() {
        "initialize" => handle_initialize(request.params).await,
        "tools/list" => handle_tools_list().await,
        "tools/call" => handle_tools_call(&state, request.params).await,
        _ => Err((
            METHOD_NOT_FOUND,
            format!("Unknown method: {}", request.method),
        )),
    };

    match result {
        Ok(value) => (
            StatusCode::OK,
            Json(JsonRpcResponse::success(request.id, value)),
        ),
        Err((code, message)) => (
            StatusCode::OK,
            Json(JsonRpcResponse::error(request.id, code, message, None)),
        ),
    }
}

// === MCP Protocol Handlers ===

/// Handle the `initialize` method
async fn handle_initialize(_params: Option<Value>) -> Result<Value, (i32, String)> {
    Ok(json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        }
    }))
}

/// Handle the `tools/list` method
async fn handle_tools_list() -> Result<Value, (i32, String)> {
    let tools = get_tool_definitions();
    Ok(json!({
        "tools": tools
    }))
}

/// Handle the `tools/call` method
async fn handle_tools_call(
    state: &McpState,
    params: Option<Value>,
) -> Result<Value, (i32, String)> {
    let params = params.ok_or((INVALID_PARAMS, "Missing params".to_string()))?;

    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or((INVALID_PARAMS, "Missing tool name".to_string()))?;

    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    // Execute the tool
    let result = execute_tool(state, name, &arguments).await;

    match result {
        Ok(content) => Ok(json!({
            "content": [{
                "type": "text",
                "text": content
            }]
        })),
        Err(err) => Ok(json!({
            "content": [{
                "type": "text",
                "text": format!("Error: {}", err)
            }],
            "isError": true
        })),
    }
}

// === Tool Execution ===

/// Execute a tool by name — public wrapper for use by stdio transport.
pub async fn execute_tool_public(
    state: &McpState,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    execute_tool(state, name, args).await
}

async fn execute_tool(state: &McpState, name: &str, args: &Value) -> Result<String, String> {
    match name {
        // Server Management
        "server_list" => tool_server_list(state, args).await,
        "server_add" => tool_server_add(state, args).await,
        "server_get" => tool_server_get(state, args).await,
        "server_update" => tool_server_update(state, args).await,
        "server_delete" => tool_server_delete(state, args).await,

        // Session Management
        "session_list" => tool_session_list(state).await,
        "session_create" => tool_session_create(state, args).await,
        "session_attach" => tool_session_attach(state, args).await,
        "session_detach" => tool_session_detach(state, args).await,
        "session_kill" => tool_session_kill(state, args).await,

        // Command Execution
        "exec" => tool_exec(state, args).await,
        "rg" => tool_remote_rg(state, args).await,

        // SFTP Operations
        "sftp_ls" => tool_sftp_ls(state, args).await,
        "sftp_upload" => tool_sftp_upload(state, args).await,
        "sftp_upload_directory" => {
            tool_sftp_upload_directory(state, args, DirectoryTransferMode::Upload).await
        }
        "sftp_sync_directory" => {
            tool_sftp_upload_directory(state, args, DirectoryTransferMode::Sync).await
        }
        "sftp_download" => tool_sftp_download(state, args).await,
        "sftp_mkdir" => tool_sftp_mkdir(state, args).await,
        "sftp_rm" => tool_sftp_rm(state, args).await,
        "sftp_mv" => tool_sftp_mv(state, args).await,
        "sftp_read" => tool_sftp_read(state, args).await,
        "sftp_write" => tool_sftp_write(state, args).await,
        "get_content" => tool_sftp_read(state, args).await,
        "edit_file" => tool_edit_file(state, args).await,
        "add_file" => tool_add_file(state, args).await,

        _ => Err(format!("Unknown tool: {}", name)),
    }
}

// === Server Management Tool Implementations ===

async fn tool_server_list(state: &McpState, args: &Value) -> Result<String, String> {
    let group_id = args.get("group_id").and_then(|v| v.as_str());
    let tags: Option<Vec<String>> = args
        .get("tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let servers = state
        .database
        .server_list(group_id, tags.as_deref())
        .map_err(|e| e.to_string())?;

    serde_json::to_string_pretty(&servers).map_err(|e| e.to_string())
}

async fn tool_server_add(state: &McpState, args: &Value) -> Result<String, String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: name")?;
    let host = args
        .get("host")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: host")?;
    let port = args.get("port").and_then(|v| v.as_u64()).unwrap_or(22) as u16;
    let username = args
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: username")?;
    let auth_type_str = args
        .get("auth_type")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: auth_type")?;

    let auth_type = match auth_type_str {
        "password" => AuthType::Password,
        "key" => AuthType::Key,
        "key_with_passphrase" => AuthType::KeyWithPassphrase,
        _ => return Err(format!("Invalid auth_type: {}", auth_type_str)),
    };

    let group_id = args
        .get("group_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let tags: Vec<String> = args
        .get("tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let mut server = Server {
        id: String::new(),
        name: name.to_string(),
        host: host.to_string(),
        port,
        username: username.to_string(),
        auth_type,
        credential_id: None,
        group_id,
        tags,
        created_at: 0,
        updated_at: 0,
        jump_host_id: None,
        post_login_command: None,
        agent_forwarding: false,
    };

    state
        .database
        .server_add(&mut server)
        .map_err(|e| e.to_string())?;

    Ok(format!(
        "Server '{}' added successfully with ID: {}",
        server.name, server.id
    ))
}

async fn tool_server_get(state: &McpState, args: &Value) -> Result<String, String> {
    let server = if let Some(id) = args.get("id").and_then(|v| v.as_str()) {
        state.database.server_get(id).map_err(|e| e.to_string())?
    } else if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
        state
            .database
            .server_get_by_name(name)
            .map_err(|e| e.to_string())?
    } else {
        return Err("Either 'id' or 'name' must be provided".to_string());
    };

    match server {
        Some(s) => serde_json::to_string_pretty(&s).map_err(|e| e.to_string()),
        None => Err("Server not found".to_string()),
    }
}

async fn tool_server_update(state: &McpState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: id")?;

    let mut server = state
        .database
        .server_get(id)
        .map_err(|e| e.to_string())?
        .ok_or("Server not found")?;

    // Update fields if provided
    if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
        server.name = name.to_string();
    }
    if let Some(host) = args.get("host").and_then(|v| v.as_str()) {
        server.host = host.to_string();
    }
    if let Some(port) = args.get("port").and_then(|v| v.as_u64()) {
        server.port = port as u16;
    }
    if let Some(username) = args.get("username").and_then(|v| v.as_str()) {
        server.username = username.to_string();
    }
    if let Some(auth_type_str) = args.get("auth_type").and_then(|v| v.as_str()) {
        server.auth_type = match auth_type_str {
            "password" => AuthType::Password,
            "key" => AuthType::Key,
            "key_with_passphrase" => AuthType::KeyWithPassphrase,
            _ => return Err(format!("Invalid auth_type: {}", auth_type_str)),
        };
    }
    if let Some(group_id) = args.get("group_id") {
        server.group_id = group_id.as_str().map(String::from);
    }
    if let Some(tags) = args.get("tags") {
        server.tags = serde_json::from_value(tags.clone()).map_err(|e| e.to_string())?;
    }

    state
        .database
        .server_update(&server)
        .map_err(|e| e.to_string())?;

    Ok(format!("Server '{}' updated successfully", server.name))
}

async fn tool_server_delete(state: &McpState, args: &Value) -> Result<String, String> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: id")?;

    state
        .database
        .server_delete(id)
        .map_err(|e| e.to_string())?;

    Ok(format!("Server '{}' deleted successfully", id))
}

// === Session Management Tool Implementations ===

async fn tool_session_list(state: &McpState) -> Result<String, String> {
    let sessions = state.session_manager.list().await;
    serde_json::to_string_pretty(&sessions).map_err(|e| e.to_string())
}

async fn tool_session_create(state: &McpState, args: &Value) -> Result<String, String> {
    // Resolve the server name: either from server_name directly or by looking up server_id
    let server_name = if let Some(name) = args.get("server_name").and_then(|v| v.as_str()) {
        name.to_string()
    } else if let Some(server_id) = args.get("server_id").and_then(|v| v.as_str()) {
        let server = state
            .database
            .server_get(server_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Server not found: {}", server_id))?;
        server.name
    } else {
        return Err("Either 'server_id' or 'server_name' must be provided".to_string());
    };

    // Look up saved credentials for this server
    let cred = state
        .database
        .credential_get(&server_name)
        .map_err(|e| format!("Failed to look up credentials: {}", e))?
        .ok_or_else(|| {
            format!(
                "No saved credentials for server '{}'. Please save credentials in the VibeShell GUI first.",
                server_name
            )
        })?;

    // Convert stored credential to SshCredential
    let ssh_cred = match cred.auth_type.as_str() {
        "password" => crate::session::SshCredential::Password(cred.credential),
        "key" | "key_with_passphrase" => crate::session::SshCredential::PrivateKey {
            key: cred.credential,
            passphrase: cred.passphrase,
        },
        other => {
            return Err(format!(
                "Unknown auth type '{}' for server '{}'",
                other, server_name
            ))
        }
    };

    // Create session with actual SSH connection (no PTY needed for exec/SFTP)
    let session = state
        .session_manager
        .create_with_credentials(&server_name, ssh_cred, None)
        .await
        .map_err(|e| format!("Failed to connect to '{}': {}", server_name, e))?;

    let info = session.get_info().await;
    serde_json::to_string_pretty(&info).map_err(|e| e.to_string())
}

async fn tool_session_attach(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let session = state
        .session_manager
        .get(session_id)
        .await
        .ok_or("Session not found")?;

    session.attach().await;

    Ok(format!(
        "Attached to session '{}'. Client count: {}",
        session_id,
        session.client_count().await
    ))
}

async fn tool_session_detach(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let session = state
        .session_manager
        .get(session_id)
        .await
        .ok_or("Session not found")?;

    session.detach().await;

    Ok(format!(
        "Detached from session '{}'. Client count: {}",
        session_id,
        session.client_count().await
    ))
}

async fn tool_session_kill(state: &McpState, args: &Value) -> Result<String, String> {
    let kill_all = args.get("all").and_then(|v| v.as_bool()).unwrap_or(false);

    if kill_all {
        state
            .session_manager
            .kill_all()
            .await
            .map_err(|e| e.to_string())?;
        Ok("All sessions terminated".to_string())
    } else {
        let session_id = args
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'session_id' or 'all' parameter")?;

        state
            .session_manager
            .kill(session_id)
            .await
            .map_err(|e| e.to_string())?;

        Ok(format!("Session '{}' terminated", session_id))
    }
}

// === Command Execution Tool Implementation ===

async fn tool_exec(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: command")?;

    let timeout_ms = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);

    let session = state
        .session_manager
        .get(session_id)
        .await
        .ok_or("Session not found")?;

    // Execute command via a dedicated exec channel (separate from the shell).
    // This opens a new SSH channel, runs the command, and captures stdout+stderr.
    let timeout_duration = tokio::time::Duration::from_millis(timeout_ms);
    let result = tokio::time::timeout(timeout_duration, session.exec_command(command))
        .await
        .map_err(|_| format!("Command timed out after {}ms", timeout_ms))?
        .map_err(|e| format!("Command execution failed: {}", e))?;

    Ok(result)
}

async fn tool_remote_rg(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let pattern = args
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: pattern")?;

    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    let max_results = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(200) as usize;

    let globs = args
        .get("globs")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut options = RemoteSearchOptions::new(pattern, path);
    options.ignore_case = args
        .get("ignore_case")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    options.fixed_strings = args
        .get("fixed_strings")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    options.hidden = args
        .get("hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    options.globs = globs;
    options.max_results = max_results;

    let command = build_remote_rg_command(&options);
    let session = state
        .session_manager
        .get(session_id)
        .await
        .ok_or("Session not found")?;

    let timeout_duration = tokio::time::Duration::from_millis(timeout_ms);
    tokio::time::timeout(timeout_duration, session.exec_command(&command))
        .await
        .map_err(|_| format!("Command timed out after {}ms", timeout_ms))?
        .map_err(|e| format!("Command execution failed: {}", e))
}

// === SFTP Tool Implementations ===

/// Open a fresh SFTP session for the given session_id.
/// Returns (SftpSession, home_dir). The SFTP channel closes when dropped.
async fn open_sftp_for_session(
    state: &McpState,
    session_id: &str,
) -> Result<(russh_sftp::client::SftpSession, String), String> {
    let session = state
        .session_manager
        .get(session_id)
        .await
        .ok_or("Session not found")?;

    let sftp = session
        .open_sftp_session()
        .await
        .map_err(|e| format!("Failed to open SFTP session: {}", e))?;

    let home_dir = sftp
        .canonicalize(".")
        .await
        .map_err(|e| format!("Failed to resolve home directory: {}", e))?;

    Ok((sftp, home_dir))
}

/// Format file size in human-readable form.
fn format_file_size(size: u64) -> String {
    if size < 1024 {
        format!("{}B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1}K", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1}M", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

async fn tool_sftp_ls(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let show_hidden = args
        .get("show_hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_path = resolve_remote_path(path, &home_dir, &home_dir);

    let dir_entries = sftp
        .read_dir(&resolved_path)
        .await
        .map_err(|e| format!("Failed to list directory {}: {}", resolved_path, e))?;

    let mut text_lines: Vec<String> = Vec::new();
    text_lines.push(format!("Directory: {}", resolved_path));
    text_lines.push(String::new());

    // Collect entries for sorting
    struct EntryInfo {
        name: String,
        is_dir: bool,
        size: u64,
        permissions: String,
    }
    let mut entries: Vec<EntryInfo> = Vec::new();

    for entry in dir_entries {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        if !show_hidden && name.starts_with('.') {
            continue;
        }

        let file_type = entry.file_type();
        let is_directory = file_type.is_dir();
        let metadata = entry.metadata();
        let size = if is_directory { 0 } else { metadata.len() };
        let perms = metadata.permissions();
        let permissions = format!("{}{}", if is_directory { "d" } else { "-" }, perms);

        entries.push(EntryInfo {
            name,
            is_dir: is_directory,
            size,
            permissions,
        });
    }

    // Sort: directories first, then alphabetical
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    for entry in &entries {
        let size_str = if entry.is_dir {
            "-".to_string()
        } else {
            format_file_size(entry.size)
        };
        let type_indicator = if entry.is_dir { "/" } else { "" };
        text_lines.push(format!(
            "{} {:>8} {}{}",
            entry.permissions, size_str, entry.name, type_indicator
        ));
    }

    text_lines.push(String::new());
    text_lines.push(format!("Total: {} entries", entries.len()));

    Ok(text_lines.join("\n"))
}

async fn tool_sftp_upload(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let local_path = args
        .get("local_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: local_path")?;

    let remote_path = args
        .get("remote_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: remote_path")?;

    // Read local file
    let content = tokio::fs::read(local_path)
        .await
        .map_err(|e| format!("Failed to read local file '{}': {}", local_path, e))?;

    let file_size = content.len();

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_remote = resolve_remote_path(remote_path, &home_dir, &home_dir);
    let filename = std::path::Path::new(local_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown")
        .to_string();
    let resolved_remote = resolve_remote_upload_path(&sftp, &resolved_remote, &filename).await;

    write_remote_file(&sftp, &resolved_remote, &content).await?;

    Ok(format!(
        "Uploaded '{}' -> '{}' ({} bytes)",
        local_path, resolved_remote, file_size
    ))
}

async fn tool_sftp_upload_directory(
    state: &McpState,
    args: &Value,
    mode: DirectoryTransferMode,
) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let local_path = args
        .get("local_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: local_path")?;

    let remote_path = args
        .get("remote_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: remote_path")?;

    let delete_extra = args
        .get("delete_extra")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let respect_gitignore = args.get("respect_gitignore").and_then(|v| v.as_bool());
    let excluded_paths = args.get("excluded_paths").and_then(|value| {
        value.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|text| text.to_string()))
                .collect::<Vec<_>>()
        })
    });

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_remote = resolve_remote_path(remote_path, &home_dir, &home_dir);
    let options =
        effective_directory_transfer_options(excluded_paths, respect_gitignore, delete_extra);
    let summary = transfer_directory_to_sftp(
        &sftp,
        std::path::Path::new(local_path),
        &resolved_remote,
        mode,
        &options,
    )
    .await?;

    Ok(format!(
        "{} '{}' -> '{}' ({} uploaded, {} skipped, {} deleted, {} bytes)",
        if summary.mode == "sync" {
            "Synced"
        } else {
            "Uploaded"
        },
        summary.local_root,
        summary.remote_root,
        summary.uploaded_files,
        summary.skipped_files,
        summary.deleted_entries,
        summary.transferred_bytes
    ))
}

async fn tool_sftp_download(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let remote_path = args
        .get("remote_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: remote_path")?;

    let local_path = args
        .get("local_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: local_path")?;

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_remote = resolve_remote_path(remote_path, &home_dir, &home_dir);

    let content = sftp
        .read(&resolved_remote)
        .await
        .map_err(|e| format!("Failed to read remote file '{}': {}", resolved_remote, e))?;

    let file_size = content.len();

    // Ensure parent directory exists locally
    if let Some(parent) = std::path::Path::new(local_path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create local directory: {}", e))?;
        }
    }

    tokio::fs::write(local_path, &content)
        .await
        .map_err(|e| format!("Failed to write local file '{}': {}", local_path, e))?;

    Ok(format!(
        "Downloaded '{}' -> '{}' ({} bytes)",
        resolved_remote, local_path, file_size
    ))
}

async fn tool_sftp_mkdir(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: path")?;

    let recursive = args
        .get("recursive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_path = resolve_remote_path(path, &home_dir, &home_dir);

    if recursive {
        sftp_mkdir_recursive(&sftp, &resolved_path).await?;
    } else {
        sftp.create_dir(&resolved_path)
            .await
            .map_err(|e| format!("Failed to create directory '{}': {}", resolved_path, e))?;
    }

    Ok(format!("Created directory '{}'", resolved_path))
}

async fn tool_sftp_rm(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: path")?;

    let recursive = args
        .get("recursive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_path = resolve_remote_path(path, &home_dir, &home_dir);

    let meta = sftp
        .metadata(&resolved_path)
        .await
        .map_err(|e| format!("Failed to stat '{}': {}", resolved_path, e))?;

    if meta.is_dir() {
        if recursive {
            sftp_remove_recursive(&sftp, &resolved_path, 0).await?;
            Ok(format!("Removed directory '{}' recursively", resolved_path))
        } else {
            sftp.remove_dir(&resolved_path)
                .await
                .map_err(|e| {
                    format!(
                        "Failed to remove directory '{}': {}. Use recursive=true for non-empty directories.",
                        resolved_path, e
                    )
                })?;
            Ok(format!("Removed directory '{}'", resolved_path))
        }
    } else {
        sftp.remove_file(&resolved_path)
            .await
            .map_err(|e| format!("Failed to remove file '{}': {}", resolved_path, e))?;
        Ok(format!("Removed file '{}'", resolved_path))
    }
}

async fn tool_sftp_mv(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: source")?;

    let destination = args
        .get("destination")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: destination")?;

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_source = resolve_remote_path(source, &home_dir, &home_dir);
    let resolved_dest = resolve_remote_path(destination, &home_dir, &home_dir);

    sftp.rename(&resolved_source, &resolved_dest)
        .await
        .map_err(|e| {
            format!(
                "Failed to move '{}' to '{}': {}",
                resolved_source, resolved_dest, e
            )
        })?;

    Ok(format!(
        "Moved '{}' -> '{}'",
        resolved_source, resolved_dest
    ))
}

async fn tool_sftp_read(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: path")?;

    let max_bytes = args
        .get("max_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(1_048_576) as usize; // 1MB default

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_path = resolve_remote_path(path, &home_dir, &home_dir);

    let content = sftp
        .read(&resolved_path)
        .await
        .map_err(|e| format!("Failed to read '{}': {}", resolved_path, e))?;

    if content.len() > max_bytes {
        let truncated = &content[..max_bytes];
        let text = String::from_utf8_lossy(truncated);
        Ok(format!(
            "[Truncated: showing {}/{} bytes]\n{}",
            max_bytes,
            content.len(),
            text
        ))
    } else {
        String::from_utf8(content).map_err(|_| {
            format!(
                "File '{}' contains non-UTF-8 binary content. Use sftp_download instead.",
                resolved_path
            )
        })
    }
}

async fn tool_sftp_write(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: path")?;

    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: content")?;

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_path = resolve_remote_path(path, &home_dir, &home_dir);

    write_remote_file(&sftp, &resolved_path, content.as_bytes()).await?;

    Ok(format!(
        "Written {} bytes to '{}'",
        content.len(),
        resolved_path
    ))
}

async fn tool_add_file(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: path")?;

    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: content")?;

    let overwrite = args
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let parents = args
        .get("parents")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_path = resolve_remote_path(path, &home_dir, &home_dir);

    if !overwrite && sftp.metadata(&resolved_path).await.is_ok() {
        return Err(format!(
            "Remote path already exists: {}. Set overwrite=true to replace it.",
            resolved_path
        ));
    }

    if let Some(parent) = remote_parent_path(&resolved_path) {
        if parents {
            sftp_mkdir_recursive(&sftp, &parent).await?;
        } else if parent != "/" {
            match sftp.metadata(&parent).await {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => {
                    return Err(format!("Parent path is not a directory: {}", parent));
                }
                Err(e) => {
                    return Err(format!(
                        "Parent directory does not exist for '{}': {}. Set parents=true to create it.",
                        resolved_path, e
                    ));
                }
            }
        }
    }

    write_remote_file(&sftp, &resolved_path, content.as_bytes()).await?;

    Ok(format!(
        "Added {} bytes to '{}'",
        content.len(),
        resolved_path
    ))
}

async fn tool_edit_file(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: path")?;

    let content = args.get("content").and_then(|v| v.as_str());
    let old_text = args.get("old_text").and_then(|v| v.as_str());
    let new_text = args.get("new_text").and_then(|v| v.as_str());
    let replace_all = args
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let replace_mode = old_text.is_some() || new_text.is_some();
    if content.is_some() && replace_mode {
        return Err("Use either content or old_text/new_text, not both".to_string());
    }

    let (sftp, home_dir) = open_sftp_for_session(state, session_id).await?;
    let resolved_path = resolve_remote_path(path, &home_dir, &home_dir);

    let metadata = sftp
        .metadata(&resolved_path)
        .await
        .map_err(|e| format!("Remote file does not exist '{}': {}", resolved_path, e))?;
    if metadata.is_dir() {
        return Err(format!(
            "Remote path is a directory, not a file: {}",
            resolved_path
        ));
    }

    if let Some(content) = content {
        write_remote_file(&sftp, &resolved_path, content.as_bytes()).await?;
        return Ok(format!(
            "Edited '{}' ({} bytes)",
            resolved_path,
            content.len()
        ));
    }

    let old_text = old_text.ok_or("Missing content or old_text/new_text")?;
    let new_text = new_text.ok_or("Missing required field: new_text")?;

    const MAX_EDIT_BYTES: u64 = 10 * 1024 * 1024;
    if metadata.len() > MAX_EDIT_BYTES {
        return Err(format!(
            "Refusing to edit '{}' because it is {} bytes (max: {} bytes)",
            resolved_path,
            metadata.len(),
            MAX_EDIT_BYTES
        ));
    }

    let bytes = sftp
        .read(&resolved_path)
        .await
        .map_err(|e| format!("Failed to read '{}': {}", resolved_path, e))?;
    let current = String::from_utf8(bytes).map_err(|_| {
        format!(
            "File '{}' contains non-UTF-8 binary content. Use sftp_download/upload instead.",
            resolved_path
        )
    })?;

    let (updated, replacements) = replace_text(&current, old_text, new_text, replace_all)?;
    write_remote_file(&sftp, &resolved_path, updated.as_bytes()).await?;

    Ok(format!(
        "Edited '{}' ({} replacement(s))",
        resolved_path, replacements
    ))
}

fn replace_text(
    content: &str,
    old_text: &str,
    new_text: &str,
    replace_all: bool,
) -> Result<(String, usize), String> {
    if old_text.is_empty() {
        return Err("old_text cannot be empty".to_string());
    }

    if replace_all {
        let replacements = content.matches(old_text).count();
        if replacements == 0 {
            return Err("old_text was not found".to_string());
        }
        return Ok((content.replace(old_text, new_text), replacements));
    }

    let Some(index) = content.find(old_text) else {
        return Err("old_text was not found".to_string());
    };

    let mut updated = String::with_capacity(content.len() - old_text.len() + new_text.len());
    updated.push_str(&content[..index]);
    updated.push_str(new_text);
    updated.push_str(&content[index + old_text.len()..]);
    Ok((updated, 1))
}

fn remote_parent_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }

    let index = trimmed.rfind('/')?;
    if index == 0 {
        Some("/".to_string())
    } else {
        Some(trimmed[..index].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_response_success() {
        let response = JsonRpcResponse::success(Some(json!(1)), json!({"result": "ok"}));
        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let response = JsonRpcResponse::error(
            Some(json!(1)),
            METHOD_NOT_FOUND,
            "Method not found".to_string(),
            None,
        );
        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let result = handle_initialize(None).await.unwrap();
        assert_eq!(result["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert!(result["capabilities"]["tools"].is_object());
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let result = handle_tools_list().await.unwrap();
        assert!(result["tools"].is_array());
        let tools = result["tools"].as_array().unwrap();
        assert!(!tools.is_empty());
    }
}
