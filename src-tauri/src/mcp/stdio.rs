//! MCP Stdio Transport
//!
//! Implements the MCP protocol over stdin/stdout, which is the standard
//! transport expected by Claude Code, Codex CLI, Cursor, and other AI tools.
//!
//! Protocol: newline-delimited JSON-RPC 2.0 over stdin/stdout.
//! Each message is a single JSON object terminated by a newline.

use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::session::SessionManager;
use crate::storage::Database;

use super::server::McpState;
use super::tools::get_compact_tool_definitions;

/// MCP protocol version
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Server name for MCP identification
const SERVER_NAME: &str = "vibeshell";

/// Server version for MCP identification
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

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

    fn error(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const PARSE_ERROR: i32 = -32700;

/// Run the MCP server over stdio (stdin/stdout).
///
/// This is the entry point for AI tool integration. The server reads
/// JSON-RPC requests from stdin and writes responses to stdout.
pub async fn run_stdio(
    database: Arc<Database>,
    session_manager: Arc<SessionManager>,
) -> Result<()> {
    let state = McpState::new(database, session_manager);

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    // Log to stderr (stdout is reserved for JSON-RPC protocol)
    eprintln!("[VibeShell MCP] Stdio server started, waiting for requests...");

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        // Parse the JSON-RPC request
        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => {
                let is_notification = request.id.is_none();
                let resp = handle_request(&state, request).await;
                // Per JSON-RPC 2.0 spec: notifications (no id) don't get responses
                if is_notification {
                    continue;
                }
                resp
            }
            Err(e) => {
                eprintln!("[VibeShell MCP] Parse error: {}", e);
                JsonRpcResponse::error(None, PARSE_ERROR, format!("Parse error: {}", e))
            }
        };

        // Serialize and write response to stdout
        let response_json = serde_json::to_string(&response)?;
        stdout.write_all(response_json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    eprintln!("[VibeShell MCP] Stdin closed, shutting down.");
    Ok(())
}

/// Handle a single JSON-RPC request and return a response.
async fn handle_request(state: &McpState, request: JsonRpcRequest) -> JsonRpcResponse {
    if request.jsonrpc != "2.0" {
        return JsonRpcResponse::error(request.id, -32600, "Invalid JSON-RPC version".to_string());
    }

    match request.method.as_str() {
        "initialize" => JsonRpcResponse::success(
            request.id,
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                }
            }),
        ),
        "notifications/initialized" => {
            // Client acknowledgment — no response needed for notifications
            // But we still return success if it has an id
            if request.id.is_some() {
                JsonRpcResponse::success(request.id, json!({}))
            } else {
                // Notifications don't get responses, but we need to return something
                // for our loop. We'll handle this in the caller.
                JsonRpcResponse::success(None, json!({}))
            }
        }
        "tools/list" => {
            let tools = get_compact_tool_definitions();
            JsonRpcResponse::success(request.id, json!({ "tools": tools }))
        }
        "tools/call" => match handle_tool_call(state, request.params).await {
            Ok(result) => JsonRpcResponse::success(request.id, result),
            Err(msg) => JsonRpcResponse::error(request.id, INVALID_PARAMS, msg),
        },
        "ping" => JsonRpcResponse::success(request.id, json!({})),
        _ => {
            eprintln!("[VibeShell MCP] Unknown method: {}", request.method);
            JsonRpcResponse::error(
                request.id,
                METHOD_NOT_FOUND,
                format!("Unknown method: {}", request.method),
            )
        }
    }
}

/// Handle a tools/call request.
async fn handle_tool_call(state: &McpState, params: Option<Value>) -> Result<Value, String> {
    let params = params.ok_or("Missing params".to_string())?;

    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing tool name".to_string())?;

    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    // Reuse the existing tool execution from the HTTP server
    let result = super::server::execute_tool_public(state, name, &arguments).await;

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
