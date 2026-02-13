//! MCP Tool Definitions
//!
//! Defines all tools exposed by the VibeShell MCP server for AI assistants.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Represents a tool definition in the MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique name of the tool
    pub name: String,
    /// Human-readable description of what the tool does
    pub description: String,
    /// JSON Schema describing the tool's input parameters
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(name: impl Into<String>, description: impl Into<String>, input_schema: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}

/// Returns all tool definitions for the MCP server
pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // === Server Management Tools ===
        server_list_tool(),
        server_add_tool(),
        server_get_tool(),
        server_update_tool(),
        server_delete_tool(),
        // === Session Management Tools ===
        session_list_tool(),
        session_create_tool(),
        session_attach_tool(),
        session_detach_tool(),
        session_kill_tool(),
        // === Command Execution Tools ===
        exec_tool(),
        // === SFTP Operations Tools ===
        sftp_ls_tool(),
        sftp_upload_tool(),
        sftp_download_tool(),
        sftp_mkdir_tool(),
        sftp_rm_tool(),
        sftp_mv_tool(),
    ]
}

// === Server Management Tools ===

fn server_list_tool() -> ToolDefinition {
    ToolDefinition::new(
        "server_list",
        "List all configured SSH servers. Optionally filter by group or tags.",
        json!({
            "type": "object",
            "properties": {
                "group_id": {
                    "type": "string",
                    "description": "Filter servers by group ID"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Filter servers that have any of these tags"
                }
            },
            "additionalProperties": false
        }),
    )
}

fn server_add_tool() -> ToolDefinition {
    ToolDefinition::new(
        "server_add",
        "Add a new SSH server configuration.",
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Unique name for the server"
                },
                "host": {
                    "type": "string",
                    "description": "Hostname or IP address"
                },
                "port": {
                    "type": "integer",
                    "description": "SSH port (default: 22)",
                    "default": 22
                },
                "username": {
                    "type": "string",
                    "description": "SSH username"
                },
                "auth_type": {
                    "type": "string",
                    "enum": ["password", "key", "key_with_passphrase"],
                    "description": "Authentication type"
                },
                "group_id": {
                    "type": "string",
                    "description": "Optional group ID to organize servers"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags for organizing and filtering servers"
                }
            },
            "required": ["name", "host", "username", "auth_type"],
            "additionalProperties": false
        }),
    )
}

fn server_get_tool() -> ToolDefinition {
    ToolDefinition::new(
        "server_get",
        "Get details of a specific SSH server by ID or name.",
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Server ID"
                },
                "name": {
                    "type": "string",
                    "description": "Server name (used if id not provided)"
                }
            },
            "additionalProperties": false
        }),
    )
}

fn server_update_tool() -> ToolDefinition {
    ToolDefinition::new(
        "server_update",
        "Update an existing SSH server configuration.",
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Server ID to update"
                },
                "name": {
                    "type": "string",
                    "description": "New name for the server"
                },
                "host": {
                    "type": "string",
                    "description": "New hostname or IP address"
                },
                "port": {
                    "type": "integer",
                    "description": "New SSH port"
                },
                "username": {
                    "type": "string",
                    "description": "New SSH username"
                },
                "auth_type": {
                    "type": "string",
                    "enum": ["password", "key", "key_with_passphrase"],
                    "description": "New authentication type"
                },
                "group_id": {
                    "type": "string",
                    "description": "New group ID"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "New tags"
                }
            },
            "required": ["id"],
            "additionalProperties": false
        }),
    )
}

fn server_delete_tool() -> ToolDefinition {
    ToolDefinition::new(
        "server_delete",
        "Delete an SSH server configuration.",
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Server ID to delete"
                }
            },
            "required": ["id"],
            "additionalProperties": false
        }),
    )
}

// === Session Management Tools ===

fn session_list_tool() -> ToolDefinition {
    ToolDefinition::new(
        "session_list",
        "List all active SSH sessions.",
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    )
}

fn session_create_tool() -> ToolDefinition {
    ToolDefinition::new(
        "session_create",
        "Create a new SSH session to a configured server.",
        json!({
            "type": "object",
            "properties": {
                "server_id": {
                    "type": "string",
                    "description": "Server ID to connect to"
                },
                "server_name": {
                    "type": "string",
                    "description": "Server name to connect to (used if server_id not provided)"
                }
            },
            "additionalProperties": false
        }),
    )
}

fn session_attach_tool() -> ToolDefinition {
    ToolDefinition::new(
        "session_attach",
        "Attach to an existing SSH session to receive output.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID to attach to"
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    )
}

fn session_detach_tool() -> ToolDefinition {
    ToolDefinition::new(
        "session_detach",
        "Detach from an SSH session.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID to detach from"
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    )
}

fn session_kill_tool() -> ToolDefinition {
    ToolDefinition::new(
        "session_kill",
        "Terminate an SSH session.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID to terminate"
                },
                "all": {
                    "type": "boolean",
                    "description": "Terminate all sessions",
                    "default": false
                }
            },
            "additionalProperties": false
        }),
    )
}

// === Command Execution Tools ===

fn exec_tool() -> ToolDefinition {
    ToolDefinition::new(
        "exec",
        "Execute a command in an active SSH session.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID to execute command in"
                },
                "command": {
                    "type": "string",
                    "description": "Command to execute"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Command timeout in milliseconds (default: 30000)",
                    "default": 30000
                }
            },
            "required": ["session_id", "command"],
            "additionalProperties": false
        }),
    )
}

// === SFTP Operations Tools ===

fn sftp_ls_tool() -> ToolDefinition {
    ToolDefinition::new(
        "sftp_ls",
        "List files and directories at a remote path.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID with active SFTP connection"
                },
                "path": {
                    "type": "string",
                    "description": "Remote path to list",
                    "default": "."
                },
                "show_hidden": {
                    "type": "boolean",
                    "description": "Include hidden files (starting with .)",
                    "default": false
                }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
    )
}

fn sftp_upload_tool() -> ToolDefinition {
    ToolDefinition::new(
        "sftp_upload",
        "Upload a file to the remote server.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID with active SFTP connection"
                },
                "local_path": {
                    "type": "string",
                    "description": "Local file path to upload"
                },
                "remote_path": {
                    "type": "string",
                    "description": "Remote destination path"
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "Overwrite existing file if present",
                    "default": false
                }
            },
            "required": ["session_id", "local_path", "remote_path"],
            "additionalProperties": false
        }),
    )
}

fn sftp_download_tool() -> ToolDefinition {
    ToolDefinition::new(
        "sftp_download",
        "Download a file from the remote server.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID with active SFTP connection"
                },
                "remote_path": {
                    "type": "string",
                    "description": "Remote file path to download"
                },
                "local_path": {
                    "type": "string",
                    "description": "Local destination path"
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "Overwrite existing file if present",
                    "default": false
                }
            },
            "required": ["session_id", "remote_path", "local_path"],
            "additionalProperties": false
        }),
    )
}

fn sftp_mkdir_tool() -> ToolDefinition {
    ToolDefinition::new(
        "sftp_mkdir",
        "Create a directory on the remote server.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID with active SFTP connection"
                },
                "path": {
                    "type": "string",
                    "description": "Remote directory path to create"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Create parent directories as needed",
                    "default": false
                }
            },
            "required": ["session_id", "path"],
            "additionalProperties": false
        }),
    )
}

fn sftp_rm_tool() -> ToolDefinition {
    ToolDefinition::new(
        "sftp_rm",
        "Remove a file or directory on the remote server.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID with active SFTP connection"
                },
                "path": {
                    "type": "string",
                    "description": "Remote path to remove"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Remove directories recursively",
                    "default": false
                }
            },
            "required": ["session_id", "path"],
            "additionalProperties": false
        }),
    )
}

fn sftp_mv_tool() -> ToolDefinition {
    ToolDefinition::new(
        "sftp_mv",
        "Move or rename a file or directory on the remote server.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID with active SFTP connection"
                },
                "source": {
                    "type": "string",
                    "description": "Source path"
                },
                "destination": {
                    "type": "string",
                    "description": "Destination path"
                }
            },
            "required": ["session_id", "source", "destination"],
            "additionalProperties": false
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_tool_definitions() {
        let tools = get_tool_definitions();

        // Verify we have all expected tools
        assert_eq!(tools.len(), 17);

        // Check that all tools have non-empty names and descriptions
        for tool in &tools {
            assert!(!tool.name.is_empty(), "Tool name should not be empty");
            assert!(!tool.description.is_empty(), "Tool description should not be empty");
        }

        // Verify specific tools exist
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"server_list"));
        assert!(tool_names.contains(&"session_create"));
        assert!(tool_names.contains(&"exec"));
        assert!(tool_names.contains(&"sftp_ls"));
    }

    #[test]
    fn test_tool_definition_serialization() {
        let tool = server_list_tool();
        let json = serde_json::to_string(&tool).expect("Should serialize");
        assert!(json.contains("server_list"));
        assert!(json.contains("inputSchema"));
    }
}
