//! MCP Server Implementation
//!
//! Implements a JSON-RPC 2.0 server using axum that exposes VibeShell's
//! SSH/SFTP functionality following the Model Context Protocol (MCP).

use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::remote_tools::{build_remote_rg_command, RemoteSearchOptions};
use crate::session::SessionManager;
use crate::sftp::helpers::{
    resolve_remote_path, resolve_remote_upload_path, sftp_delete_path, sftp_mkdir_recursive,
    write_remote_file, write_remote_file_with_options, WriteRemoteFileOptions,
};
use crate::sftp::{
    effective_directory_transfer_options, transfer_directory_to_sftp, DirectoryTransferMode,
};
use crate::storage::models::{AuthType, Server};
use crate::storage::Database;

use super::approval::{AgentApprovalManager, ApprovalOutcome, ApprovalRequest};
use super::guard::{self, GuardConfig, GUARD_CONFIG_KEY};
use super::tools::get_tool_definitions;

/// MCP protocol version
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

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
    activity_emitter: Option<Arc<dyn Fn(AgentActivityEvent) + Send + Sync>>,
    terminal_input_emitter: Option<Arc<dyn Fn(TerminalInputEvent) + Send + Sync>>,
    approvals: Option<Arc<AgentApprovalManager>>,
    agent_input_tracker: Arc<guard::SharedAgentInputTracker>,
}

impl McpState {
    pub fn new(database: Arc<Database>, session_manager: Arc<SessionManager>) -> Self {
        Self {
            database,
            session_manager,
            activity_emitter: None,
            terminal_input_emitter: None,
            approvals: None,
            agent_input_tracker: Arc::new(guard::SharedAgentInputTracker::default()),
        }
    }

    pub fn with_activity_emitter(
        mut self,
        emitter: Arc<dyn Fn(AgentActivityEvent) + Send + Sync>,
    ) -> Self {
        self.activity_emitter = Some(emitter);
        self
    }

    pub fn with_terminal_input_emitter(
        mut self,
        emitter: Arc<dyn Fn(TerminalInputEvent) + Send + Sync>,
    ) -> Self {
        self.terminal_input_emitter = Some(emitter);
        self
    }

    pub fn with_approvals(mut self, approvals: Arc<AgentApprovalManager>) -> Self {
        self.approvals = Some(approvals);
        self
    }

    pub fn with_agent_input_tracker(
        mut self,
        tracker: Arc<guard::SharedAgentInputTracker>,
    ) -> Self {
        self.agent_input_tracker = tracker;
        self
    }

    fn emit_activity(&self, event: AgentActivityEvent) {
        if let Some(emitter) = &self.activity_emitter {
            emitter(event);
        }
    }

    fn emit_terminal_input(&self, session_id: &str, text: String, kind: TerminalInputKind) {
        if let Some(emitter) = &self.terminal_input_emitter {
            emitter(TerminalInputEvent {
                id: Uuid::new_v4().to_string(),
                session_id: session_id.to_string(),
                text,
                kind,
                timestamp: chrono::Utc::now().timestamp_millis(),
            });
        }
    }

    /// Load the persisted command-guard configuration (falling back to safe
    /// defaults when unset or unparseable).
    fn load_guard_config(&self) -> GuardConfig {
        let json = self.database.get_setting(GUARD_CONFIG_KEY).ok().flatten();
        GuardConfig::from_stored_json(json.as_deref())
    }

    /// Block until the user approves a risky command, or return an error string
    /// (surfaced to the agent) when denied. A transport without a GUI approval
    /// manager fails closed instead of silently executing a risky command.
    async fn require_approval(
        &self,
        tool: &str,
        session_id: Option<&str>,
        command: &str,
        reasons: Vec<String>,
    ) -> Result<(), String> {
        let Some(approvals) = &self.approvals else {
            return Err(
                "Command requires user approval, but no approval UI is available.".to_string(),
            );
        };

        match approvals
            .gate(ApprovalRequest {
                tool: tool.to_string(),
                command: command.to_string(),
                reasons,
                session_id: session_id.map(ToOwned::to_owned),
            })
            .await
        {
            ApprovalOutcome::Approved => Ok(()),
            ApprovalOutcome::Denied(reason) => Err(format!(
                "Command requires user approval and was not granted ({}).",
                reason
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivityEvent {
    pub id: String,
    pub tool: String,
    pub summary: String,
    pub status: AgentActivityStatus,
    pub session_id: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentActivityStatus {
    Started,
    Succeeded,
    Failed,
}

/// The presentation role of an agent terminal event.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TerminalInputKind {
    /// A complete command submitted to the shared PTY (shown in the activity notice).
    Input,
    /// Printable text about to be typed into the PTY (decorated inline).
    Typing,
    /// A command executed through a separate SSH exec channel.
    Exec,
}

/// Emitted when the agent drives the terminal so the GUI can distinguish its
/// input from human keystrokes without writing synthetic bytes into the PTY.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInputEvent {
    pub id: String,
    pub session_id: String,
    pub text: String,
    pub kind: TerminalInputKind,
    pub timestamp: i64,
}

/// MCP Server
pub struct McpServer {
    state: McpState,
}

impl McpServer {
    /// Create a new MCP server with the given database and session manager
    pub fn new(database: Arc<Database>, session_manager: Arc<SessionManager>) -> Self {
        Self {
            state: McpState::new(database, session_manager),
        }
    }

    pub fn with_activity_emitter(
        mut self,
        emitter: Arc<dyn Fn(AgentActivityEvent) + Send + Sync>,
    ) -> Self {
        self.state = self.state.with_activity_emitter(emitter);
        self
    }

    pub fn with_terminal_input_emitter(
        mut self,
        emitter: Arc<dyn Fn(TerminalInputEvent) + Send + Sync>,
    ) -> Self {
        self.state = self.state.with_terminal_input_emitter(emitter);
        self
    }

    pub fn with_approvals(mut self, approvals: Arc<AgentApprovalManager>) -> Self {
        self.state = self.state.with_approvals(approvals);
        self
    }

    pub fn with_agent_input_tracker(
        mut self,
        tracker: Arc<guard::SharedAgentInputTracker>,
    ) -> Self {
        self.state = self.state.with_agent_input_tracker(tracker);
        self
    }

    pub fn router(&self, bearer_token: String) -> Router {
        Router::new()
            .route("/health", get(handle_health))
            .route("/", post(handle_jsonrpc))
            .route("/mcp", post(handle_jsonrpc))
            .with_state(self.state.clone())
            .layer(middleware::from_fn_with_state(
                Arc::new(bearer_token),
                require_bearer_token,
            ))
    }
}

async fn require_bearer_token(
    State(expected): State<Arc<String>>,
    request: Request,
    next: Next,
) -> Response {
    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    let authorized = provided
        .map(|token| bool::from(token.as_bytes().ct_eq(expected.as_bytes())))
        .unwrap_or(false);

    if !authorized {
        let mut response = (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return response;
    }

    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn handle_health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "server": SERVER_NAME,
        "version": SERVER_VERSION,
        "protocolVersion": MCP_PROTOCOL_VERSION
    }))
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
        "notifications/initialized" => Ok(json!({})),
        "ping" => Ok(json!({})),
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
    let activity_id = Uuid::new_v4().to_string();
    let summary = activity_summary(name, &arguments);
    let session_id = arguments
        .get("session_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    state.emit_activity(AgentActivityEvent {
        id: activity_id.clone(),
        tool: name.to_string(),
        summary: summary.clone(),
        status: AgentActivityStatus::Started,
        session_id: session_id.clone(),
        timestamp: chrono::Utc::now().timestamp_millis(),
    });

    // Execute the tool
    let result = execute_tool(state, name, &arguments).await;

    match result {
        Ok(content) => {
            state.emit_activity(AgentActivityEvent {
                id: activity_id,
                tool: name.to_string(),
                summary,
                status: AgentActivityStatus::Succeeded,
                session_id,
                timestamp: chrono::Utc::now().timestamp_millis(),
            });
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": content
                }]
            }))
        }
        Err(err) => {
            state.emit_activity(AgentActivityEvent {
                id: activity_id,
                tool: name.to_string(),
                summary,
                status: AgentActivityStatus::Failed,
                session_id,
                timestamp: chrono::Utc::now().timestamp_millis(),
            });
            Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Error: {}", err)
                }],
                "isError": true
            }))
        }
    }
}

fn activity_summary(name: &str, args: &Value) -> String {
    let detail = match name {
        "exec" => args.get("command").and_then(Value::as_str),
        "session_create" => args
            .get("server_name")
            .or_else(|| args.get("server_id"))
            .and_then(Value::as_str),
        "session_send_input" => args.get("data").and_then(Value::as_str),
        "sftp_ls" | "sftp_mkdir" | "sftp_rm" | "sftp_read" | "sftp_write" | "get_content"
        | "edit_file" | "add_file" => args.get("path").and_then(Value::as_str),
        "sftp_upload" | "sftp_upload_directory" | "sftp_sync_directory" => args
            .get("remote_path")
            .or_else(|| args.get("local_path"))
            .and_then(Value::as_str),
        "sftp_download" => args
            .get("remote_path")
            .or_else(|| args.get("local_path"))
            .and_then(Value::as_str),
        "sftp_mv" => args.get("source").and_then(Value::as_str),
        "rg" => args.get("pattern").and_then(Value::as_str),
        _ => args.get("session_id").and_then(Value::as_str),
    };

    let Some(detail) = detail else {
        return name.replace('_', " ");
    };
    let detail = detail.replace(['\r', '\n'], " ");
    let detail = if detail.chars().count() > 160 {
        format!("{}...", detail.chars().take(157).collect::<String>())
    } else {
        detail
    };
    format!("{}: {}", name.replace('_', " "), detail)
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
        "session_send_input" => tool_session_send_input(state, args).await,
        "session_read" => tool_session_read(state, args).await,
        "session_resize" => tool_session_resize(state, args).await,

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

    let force_new = args
        .get("force_new")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if !force_new {
        if let Some(session) = state
            .session_manager
            .find_reusable_by_server_name(&server_name)
            .await
        {
            return serde_json::to_string_pretty(&session.get_info().await)
                .map_err(|error| error.to_string());
        }
    }

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

    // Create a real terminal session so the GUI and Agent share the same PTY.
    let session = state
        .session_manager
        .create_with_credentials(
            &server_name,
            ssh_cred,
            Some(crate::ssh::PtyConfig::default()),
        )
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

async fn tool_session_send_input(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or("Missing required field: session_id")?;
    let data_arg = args.get("data").and_then(Value::as_str).unwrap_or_default();
    let mut keys_arg = Vec::new();
    if let Some(keys) = args.get("keys").and_then(Value::as_array) {
        for key in keys {
            keys_arg.push(key.as_str().ok_or("keys must contain strings")?.to_string());
        }
    }
    let append_enter = args
        .get("append_enter")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut data = data_arg.as_bytes().to_vec();
    for key in &keys_arg {
        data.extend_from_slice(named_key_bytes(key)?);
    }
    if append_enter {
        data.push(b'\r');
    }

    if data.is_empty() {
        return Err("Provide data, keys, or append_enter=true".to_string());
    }

    let session = state
        .session_manager
        .get(session_id)
        .await
        .ok_or("Session not found")?;

    // Keep the tracker transaction open until approval and the PTY write both
    // succeed. If either fails, rolling back prevents a denied split command
    // from being submitted later with a bare Enter that appears harmless.
    let _session_input_guard = state.agent_input_tracker.lock_session(session_id).await;
    let (tracker_checkpoint, executed_commands) = state
        .agent_input_tracker
        .checkpoint_and_observe(session_id, data_arg, &keys_arg, append_enter)
        .await;

    // Gate and surface commands that will actually execute in the shared terminal.
    let cfg = state.load_guard_config();
    for command in executed_commands {
        if cfg.enabled {
            let mut decision = guard::classify_command(&command.command, &cfg);
            if !command.is_verifiable {
                decision.requires_approval = true;
                decision.reasons.push(
                    "Shell history, completion, or cursor editing hides the final command text"
                        .to_string(),
                );
            }
            if decision.requires_approval {
                if let Err(error) = state
                    .require_approval(
                        "session_send_input",
                        Some(session_id),
                        &command.command,
                        decision.reasons,
                    )
                    .await
                {
                    state.agent_input_tracker.restore(tracker_checkpoint).await;
                    return Err(error);
                }
            }
        }

        state.emit_terminal_input(session_id, command.command, TerminalInputKind::Input);
    }

    // Emit printable typing immediately before the PTY write. This lets the UI
    // decorate split input calls at the actual cursor position; command notices
    // above remain whole and are emitted only when Enter submits the line.
    let typing_text = data_arg.trim_end_matches(['\r', '\n']);
    if !typing_text.is_empty()
        && typing_text
            .chars()
            .all(|ch| !ch.is_control() || matches!(ch, '\r' | '\n'))
    {
        state.emit_terminal_input(
            session_id,
            typing_text.to_string(),
            TerminalInputKind::Typing,
        );
    }

    if let Err(error) = session.send_input(data.clone()).await {
        state.agent_input_tracker.restore(tracker_checkpoint).await;
        return Err(format!("Failed to send terminal input: {}", error));
    }

    Ok(format!(
        "Sent {} byte(s) to session '{}'",
        data.len(),
        session_id
    ))
}

fn named_key_bytes(key: &str) -> Result<&'static [u8], String> {
    match key.to_ascii_lowercase().as_str() {
        "enter" => Ok(b"\r"),
        "tab" => Ok(b"\t"),
        "escape" | "esc" => Ok(b"\x1b"),
        "backspace" => Ok(b"\x7f"),
        "ctrl-c" => Ok(b"\x03"),
        "ctrl-d" => Ok(b"\x04"),
        "ctrl-z" => Ok(b"\x1a"),
        "up" => Ok(b"\x1b[A"),
        "down" => Ok(b"\x1b[B"),
        "right" => Ok(b"\x1b[C"),
        "left" => Ok(b"\x1b[D"),
        _ => Err(format!("Unsupported named key: {}", key)),
    }
}

async fn tool_session_read(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or("Missing required field: session_id")?;
    let max_bytes = args
        .get("max_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(65_536)
        .clamp(1, 65_536) as usize;
    let session = state
        .session_manager
        .get(session_id)
        .await
        .ok_or("Session not found")?;

    let output = session.replay_output().await.concat();
    let start = output.len().saturating_sub(max_bytes);
    Ok(String::from_utf8_lossy(&output[start..]).into_owned())
}

async fn tool_session_resize(state: &McpState, args: &Value) -> Result<String, String> {
    let session_id = args
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or("Missing required field: session_id")?;
    let cols = args
        .get("cols")
        .and_then(Value::as_u64)
        .ok_or("Missing required field: cols")? as u32;
    let rows = args
        .get("rows")
        .and_then(Value::as_u64)
        .ok_or("Missing required field: rows")? as u32;
    if cols == 0 || rows == 0 {
        return Err("cols and rows must be greater than zero".to_string());
    }

    let session = state
        .session_manager
        .get(session_id)
        .await
        .ok_or("Session not found")?;
    session
        .resize_pty(cols, rows)
        .await
        .map_err(|error| format!("Failed to resize terminal: {}", error))?;
    Ok(format!(
        "Resized session '{}' to {}x{}",
        session_id, cols, rows
    ))
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

    // Gate risky exec commands, then surface them in the shared terminal so the
    // human collaborator can see out-of-band work too.
    let cfg = state.load_guard_config();
    if cfg.enabled && cfg.require_for_exec {
        let decision = guard::classify_command(command, &cfg);
        if decision.requires_approval {
            state
                .require_approval("exec", Some(session_id), command, decision.reasons)
                .await?;
        }
    }
    state.emit_terminal_input(session_id, command.to_string(), TerminalInputKind::Exec);

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

    sftp_delete_path(&sftp, &resolved_path, recursive).await?;
    Ok(format!("Removed '{}'", resolved_path))
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

    write_remote_file_with_options(
        &sftp,
        &resolved_path,
        content.as_bytes(),
        WriteRemoteFileOptions {
            create_parent_dirs: parents,
            overwrite,
        },
    )
    .await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use std::time::Duration;
    use tower::ServiceExt;

    fn protected_health_router(token: &str) -> Router {
        Router::new()
            .route("/health", get(handle_health))
            .layer(middleware::from_fn_with_state(
                Arc::new(token.to_string()),
                require_bearer_token,
            ))
    }

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

    #[tokio::test]
    async fn gateway_rejects_missing_bearer_token() {
        let response = protected_health_router("secret")
            .oneshot(HttpRequest::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn gateway_rejects_wrong_bearer_token() {
        let response = protected_health_router("secret")
            .oneshot(
                HttpRequest::get("/health")
                    .header(header::AUTHORIZATION, "Bearer incorrect")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn gateway_accepts_matching_bearer_token_without_caching() {
        let response = protected_health_router("secret")
            .oneshot(
                HttpRequest::get("/health")
                    .header(header::AUTHORIZATION, "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }

    #[tokio::test]
    async fn gateway_tool_call_emits_started_and_succeeded_activity() {
        let temp = tempfile::tempdir().unwrap();
        let database = Arc::new(Database::new_at(temp.path().join("gateway.db")).unwrap());
        let session_manager = Arc::new(SessionManager::new(database.clone()));
        let (activity_tx, activity_rx) = std::sync::mpsc::channel();
        let emitter = Arc::new(move |event| {
            activity_tx.send(event).unwrap();
        });
        let app = McpServer::new(database, session_manager)
            .with_activity_emitter(emitter)
            .router("secret".to_string());
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "session_list",
                "arguments": {}
            }
        });

        let response = app
            .oneshot(
                HttpRequest::post("/mcp")
                    .header(header::AUTHORIZATION, "Bearer secret")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let started = activity_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let completed = activity_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(started.id, completed.id);
        assert_eq!(started.tool, "session_list");
        assert_eq!(started.status, AgentActivityStatus::Started);
        assert_eq!(completed.status, AgentActivityStatus::Succeeded);
    }
}
