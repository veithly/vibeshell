//! MCP Server Implementation
//!
//! Implements a JSON-RPC 2.0 server using axum that exposes VibeShell's
//! SSH/SFTP functionality following the Model Context Protocol (MCP).

use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::session::SessionManager;
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
        _ => Err((METHOD_NOT_FOUND, format!("Unknown method: {}", request.method))),
    };

    match result {
        Ok(value) => (StatusCode::OK, Json(JsonRpcResponse::success(request.id, value))),
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

        // SFTP Operations
        "sftp_ls" => tool_sftp_ls(state, args).await,
        "sftp_upload" => tool_sftp_upload(state, args).await,
        "sftp_download" => tool_sftp_download(state, args).await,
        "sftp_mkdir" => tool_sftp_mkdir(state, args).await,
        "sftp_rm" => tool_sftp_rm(state, args).await,
        "sftp_mv" => tool_sftp_mv(state, args).await,

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

    let group_id = args.get("group_id").and_then(|v| v.as_str()).map(String::from);
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
    let session = if let Some(server_id) = args.get("server_id").and_then(|v| v.as_str()) {
        state
            .session_manager
            .create(server_id)
            .await
            .map_err(|e| e.to_string())?
    } else if let Some(server_name) = args.get("server_name").and_then(|v| v.as_str()) {
        state
            .session_manager
            .create_by_name(server_name)
            .await
            .map_err(|e| e.to_string())?
    } else {
        return Err("Either 'server_id' or 'server_name' must be provided".to_string());
    };

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

    let _timeout_ms = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);

    let session = state
        .session_manager
        .get(session_id)
        .await
        .ok_or("Session not found")?;

    // Send the command to the session
    session
        .send_input(format!("{}\n", command).into_bytes())
        .await
        .map_err(|e| e.to_string())?;

    // TODO: Implement proper command execution with output capture
    // For now, we just send the input and acknowledge
    Ok(format!("Command '{}' sent to session '{}'", command, session_id))
}

// === SFTP Tool Implementations ===
// Note: These are placeholder implementations. Full SFTP integration
// requires the SftpClient to be associated with sessions.

async fn tool_sftp_ls(_state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let _show_hidden = args.get("show_hidden").and_then(|v| v.as_bool()).unwrap_or(false);

    // TODO: Implement actual SFTP ls operation
    Ok(format!(
        "SFTP ls for session '{}' at path '{}' (not yet implemented)",
        session_id, path
    ))
}

async fn tool_sftp_upload(_state: &McpState, args: &Value) -> Result<String, String> {
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

    // TODO: Implement actual SFTP upload operation
    Ok(format!(
        "SFTP upload for session '{}': '{}' -> '{}' (not yet implemented)",
        session_id, local_path, remote_path
    ))
}

async fn tool_sftp_download(_state: &McpState, args: &Value) -> Result<String, String> {
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

    // TODO: Implement actual SFTP download operation
    Ok(format!(
        "SFTP download for session '{}': '{}' -> '{}' (not yet implemented)",
        session_id, remote_path, local_path
    ))
}

async fn tool_sftp_mkdir(_state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: path")?;

    let _recursive = args.get("recursive").and_then(|v| v.as_bool()).unwrap_or(false);

    // TODO: Implement actual SFTP mkdir operation
    Ok(format!(
        "SFTP mkdir for session '{}': '{}' (not yet implemented)",
        session_id, path
    ))
}

async fn tool_sftp_rm(_state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: session_id")?;

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required field: path")?;

    let _recursive = args.get("recursive").and_then(|v| v.as_bool()).unwrap_or(false);

    // TODO: Implement actual SFTP rm operation
    Ok(format!(
        "SFTP rm for session '{}': '{}' (not yet implemented)",
        session_id, path
    ))
}

async fn tool_sftp_mv(_state: &McpState, args: &Value) -> Result<String, String> {
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

    // TODO: Implement actual SFTP mv operation
    Ok(format!(
        "SFTP mv for session '{}': '{}' -> '{}' (not yet implemented)",
        session_id, source, destination
    ))
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
