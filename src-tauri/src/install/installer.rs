//! VibeShell skill installer for AI coding tools.
//!
//! This module provides functionality to install and uninstall VibeShell skill
//! configuration to/from various AI coding tools.

use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};

use super::detector::{find_tool, AiTool};

/// The VibeShell skill configuration key name
const VIBESHELL_KEY: &str = "vibeshell";

/// Create the VibeShell skill server configuration.
fn vibeshell_skill_config() -> Value {
    json!({
        "command": "vshell",
        "args": ["skill-server"],
        "description": "VibeShell - SSH/SFTP management skill for AI tools"
    })
}

/// The SKILL.md content that teaches AI how to use VibeShell for SSH connections.
const SKILL_MD_CONTENT: &str = r#"---
name: vibeshell
description: Connect to remote SSH servers, execute commands, and transfer files via SFTP using VibeShell. Use this when the user needs to manage remote servers, deploy code, run remote commands, or transfer files over SSH.
---

You have access to VibeShell, a high-performance SSH/SFTP terminal. VibeShell provides MCP tools prefixed with `mcp__vibeshell__` that let you manage servers, sessions, and file transfers.

## Quick Start

To connect to a server and run commands:

1. Check existing servers: `mcp__vibeshell__server_list`
2. Connect: `mcp__vibeshell__session_create` with `server_name`
3. Run commands: `mcp__vibeshell__exec` with `session_id` and `command`
4. Clean up when done: `mcp__vibeshell__session_kill` with `session_id`

## When to Use This Skill

- User asks to "SSH into", "connect to", or "log into" a remote server
- User wants to run commands on a remote machine
- User needs to deploy files or code to a server
- User wants to check server status, logs, or resource usage
- User needs to transfer files between local and remote machines

## Tools Reference

### Server Management
- **server_list** — List configured servers (optional: `group_id`, `tags`)
- **server_add** — Add server (`name`, `host`, `username`, `auth_type` required; `port` defaults to 22)
- **server_get** — Get server details (by `id` or `name`)
- **server_update** — Update server config (`id` required, other fields optional)
- **server_delete** — Remove server (`id` required)

### Sessions (Active SSH Connections)
- **session_list** — List active sessions
- **session_create** — Open SSH connection (`server_id` or `server_name`)
- **session_attach** — Reattach to a session (`session_id`)
- **session_detach** — Detach without closing (`session_id`)
- **session_kill** — Terminate session (`session_id`, or `all: true`)

### Remote Execution
- **exec** — Run a command (`session_id`, `command` required; `timeout_ms` defaults to 30s)

### File Transfer (SFTP)
- **sftp_ls** — List remote directory (`session_id`, `path`)
- **sftp_upload** — Upload file (`session_id`, `local_path`, `remote_path`)
- **sftp_download** — Download file (`session_id`, `remote_path`, `local_path`)
- **sftp_mkdir** — Create remote directory (`session_id`, `path`, `recursive`)
- **sftp_rm** — Delete remote file/dir (`session_id`, `path`, `recursive`)
- **sftp_mv** — Move/rename remote path (`session_id`, `source`, `destination`)

## Auth Types

| Type | Use When |
|------|----------|
| `password` | Username + password |
| `key` | SSH private key (PEM, no passphrase) |
| `key_with_passphrase` | Encrypted SSH private key |

## Guidelines

- Always call `session_list` before creating a new session to avoid duplicates
- Prefer `server_name` over `server_id` for readability
- Sessions persist when detached — reattach later instead of recreating
- Always `session_kill` when done to free server resources
- Use tags to organize servers (e.g., `["prod", "web"]`, `["staging", "db"]`)
"#;

/// Get the skills directory path for a given AI tool.
///
/// Returns the directory where skill folders should be placed for each tool.
/// Returns None if the tool doesn't support skills or the home directory is unknown.
fn get_skills_dir(tool_id: &str) -> Option<PathBuf> {
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())?;

    match tool_id {
        "claude-code" => Some(home.join(".claude").join("skills")),
        "cursor" => Some(home.join(".cursor").join("skills")),
        "codex" => Some(home.join(".codex").join("skills")),
        "opencode" => Some(home.join(".opencode").join("skills")),
        "gemini-cli" => Some(home.join(".gemini").join("skills")),
        "openclaw" => Some(home.join(".openclaw").join("skills")),
        _ => None,
    }
}

/// Install the SKILL.md file into the tool's skills directory.
fn install_skill_file(tool_id: &str) -> Result<()> {
    if let Some(skills_dir) = get_skills_dir(tool_id) {
        let skill_dir = skills_dir.join("vibeshell");
        fs::create_dir_all(&skill_dir)
            .with_context(|| format!("Failed to create skill directory {:?}", skill_dir))?;

        let skill_path = skill_dir.join("SKILL.md");
        fs::write(&skill_path, SKILL_MD_CONTENT)
            .with_context(|| format!("Failed to write skill file {:?}", skill_path))?;

        log::info!("[Install] Skill file installed to {:?}", skill_path);
    }
    Ok(())
}

/// Remove the SKILL.md file from the tool's skills directory.
fn uninstall_skill_file(tool_id: &str) -> Result<()> {
    if let Some(skills_dir) = get_skills_dir(tool_id) {
        let skill_dir = skills_dir.join("vibeshell");
        if skill_dir.exists() {
            fs::remove_dir_all(&skill_dir)
                .with_context(|| format!("Failed to remove skill directory {:?}", skill_dir))?;
            log::info!("[Install] Skill file removed from {:?}", skill_dir);
        }
    }
    Ok(())
}

/// Create a backup of the config file before modification.
fn backup_config(config_path: &PathBuf) -> Result<Option<PathBuf>> {
    if !config_path.exists() {
        return Ok(None);
    }

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!(
        "{}.backup.{}",
        config_path.file_name().unwrap_or_default().to_string_lossy(),
        timestamp
    );
    let backup_path = config_path.parent().unwrap().join(backup_name);

    fs::copy(config_path, &backup_path)
        .with_context(|| format!("Failed to backup config to {:?}", backup_path))?;

    Ok(Some(backup_path))
}

/// Ensure the parent directory exists for the config file.
fn ensure_parent_dir(config_path: &Path) -> Result<()> {
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }
    }
    Ok(())
}

/// Read and parse the existing config file, or return an empty object.
fn read_config(config_path: &Path) -> Result<Value> {
    if !config_path.exists() {
        return Ok(json!({}));
    }

    let content = fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config file {:?}", config_path))?;

    if content.trim().is_empty() {
        return Ok(json!({}));
    }

    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse config file {:?}", config_path))
}

/// Write the config to file with pretty formatting.
fn write_config(config_path: &Path, config: &Value) -> Result<()> {
    let content = serde_json::to_string_pretty(config)?;
    fs::write(config_path, content)
        .with_context(|| format!("Failed to write config file {:?}", config_path))?;
    Ok(())
}

/// Whether the tool config should store vibeshell at root level.
///
/// Rules:
/// - Codex always uses root-level format
/// - Cursor uses root-level format when config file is mcpServers.json
fn should_use_root_level_format(tool_id: &str, config_path: &Path) -> bool {
    if tool_id == "codex" {
        return true;
    }

    if tool_id == "cursor" {
        if let Some(file_name) = config_path.file_name().and_then(|f| f.to_str()) {
            return file_name.eq_ignore_ascii_case("mcpServers.json");
        }
    }

    false
}

/// Install VibeShell skill to a specific AI tool.
///
/// # Arguments
/// * `tool_id` - The ID of the tool to install to (e.g., "claude-code", "cursor")
/// * `config_path` - The path to the tool's config file
///
/// # Returns
/// * `Ok(PathBuf)` - The path to the backup file if one was created
/// * `Err` - If installation failed
pub fn install_to_tool(tool_id: &str, config_path: &PathBuf) -> Result<Option<PathBuf>> {
    // Ensure parent directory exists
    ensure_parent_dir(config_path)?;

    // Backup existing config
    let backup_path = backup_config(config_path)?;

    // Read existing config
    let mut config = read_config(config_path)?;

    // Handle different config formats
    if should_use_root_level_format(tool_id, config_path) {
        if let Value::Object(ref mut map) = config {
            map.insert(VIBESHELL_KEY.to_string(), vibeshell_skill_config());
        }
    } else {
        // Standard MCP format: under mcpServers
        if let Value::Object(ref mut map) = config {
            // Get or create mcpServers object
            let mcp_servers = map
                .entry("mcpServers")
                .or_insert_with(|| json!({}));

            if let Value::Object(ref mut servers) = mcp_servers {
                servers.insert(VIBESHELL_KEY.to_string(), vibeshell_skill_config());
            }
        }
    }

    // Write updated config
    write_config(config_path, &config)?;

    // Install the SKILL.md file to the tool's skills directory
    install_skill_file(tool_id)?;

    Ok(backup_path)
}

/// Uninstall VibeShell skill from a specific AI tool.
///
/// # Arguments
/// * `tool_id` - The ID of the tool to uninstall from
/// * `config_path` - The path to the tool's config file
///
/// # Returns
/// * `Ok(PathBuf)` - The path to the backup file
/// * `Err` - If uninstallation failed
pub fn uninstall_from_tool(tool_id: &str, config_path: &PathBuf) -> Result<Option<PathBuf>> {
    if !config_path.exists() {
        return Ok(None);
    }

    // Backup existing config
    let backup_path = backup_config(config_path)?;

    // Read existing config
    let mut config = read_config(config_path)?;

    // Handle different config formats
    if should_use_root_level_format(tool_id, config_path) {
        // Root format: remove vibeshell directly
        if let Value::Object(ref mut map) = config {
            map.remove(VIBESHELL_KEY);
        }
    } else {
        // Standard MCP format: remove under mcpServers
        if let Value::Object(ref mut map) = config {
            if let Some(Value::Object(ref mut servers)) = map.get_mut("mcpServers") {
                servers.remove(VIBESHELL_KEY);

                // Remove mcpServers if empty
                if servers.is_empty() {
                    map.remove("mcpServers");
                }
            }
        }
    }

    // Write updated config
    write_config(config_path, &config)?;

    // Remove the SKILL.md file from the tool's skills directory
    uninstall_skill_file(tool_id)?;

    Ok(backup_path)
}

/// Result of an installation operation.
#[derive(Debug)]
pub struct InstallResult {
    /// The tool that was installed to
    pub tool: AiTool,
    /// Whether the installation was successful
    pub success: bool,
    /// Path to the backup file if created
    pub backup_path: Option<PathBuf>,
    /// Error message if installation failed
    pub error: Option<String>,
}

/// Install VibeShell skill to a tool by ID.
///
/// This is a convenience function that looks up the tool and calls install_to_tool.
pub fn install_by_id(tool_id: &str) -> Result<InstallResult> {
    let tool = find_tool(tool_id)
        .ok_or_else(|| anyhow!("Unknown tool: {}", tool_id))?;

    match install_to_tool(&tool.id, &tool.config_path) {
        Ok(backup_path) => Ok(InstallResult {
            tool,
            success: true,
            backup_path,
            error: None,
        }),
        Err(e) => Ok(InstallResult {
            tool,
            success: false,
            backup_path: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Uninstall VibeShell skill from a tool by ID.
///
/// This is a convenience function that looks up the tool and calls uninstall_from_tool.
pub fn uninstall_by_id(tool_id: &str) -> Result<InstallResult> {
    let tool = find_tool(tool_id)
        .ok_or_else(|| anyhow!("Unknown tool: {}", tool_id))?;

    match uninstall_from_tool(&tool.id, &tool.config_path) {
        Ok(backup_path) => Ok(InstallResult {
            tool,
            success: true,
            backup_path,
            error: None,
        }),
        Err(e) => Ok(InstallResult {
            tool,
            success: false,
            backup_path: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Install VibeShell skill to all detected installed tools.
pub fn install_to_all() -> Vec<InstallResult> {
    use super::detector::get_installed_tools;

    get_installed_tools()
        .into_iter()
        .map(|tool| {
            match install_to_tool(&tool.id, &tool.config_path) {
                Ok(backup_path) => InstallResult {
                    tool,
                    success: true,
                    backup_path,
                    error: None,
                },
                Err(e) => InstallResult {
                    tool,
                    success: false,
                    backup_path: None,
                    error: Some(e.to_string()),
                },
            }
        })
        .collect()
}

/// Uninstall VibeShell skill from all configured tools.
pub fn uninstall_from_all() -> Vec<InstallResult> {
    use super::detector::get_configured_tools;

    get_configured_tools()
        .into_iter()
        .map(|tool| {
            match uninstall_from_tool(&tool.id, &tool.config_path) {
                Ok(backup_path) => InstallResult {
                    tool,
                    success: true,
                    backup_path,
                    error: None,
                },
                Err(e) => InstallResult {
                    tool,
                    success: false,
                    backup_path: None,
                    error: Some(e.to_string()),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_temp_config(dir: &TempDir, content: &str) -> PathBuf {
        let config_path = dir.path().join("mcp.json");
        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        config_path
    }

    fn create_temp_config_with_name(dir: &TempDir, file_name: &str, content: &str) -> PathBuf {
        let config_path = dir.path().join(file_name);
        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        config_path
    }

    #[test]
    fn test_install_to_empty_config() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("mcp.json");

        let result = install_to_tool("claude-code", &config_path);
        assert!(result.is_ok());

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();

        assert!(json["mcpServers"]["vibeshell"].is_object());
        assert_eq!(json["mcpServers"]["vibeshell"]["command"], "vshell");
    }

    #[test]
    fn test_install_merges_with_existing() {
        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config(&dir, r#"{
            "mcpServers": {
                "other-server": {
                    "command": "other"
                }
            }
        }"#);

        install_to_tool("claude-code", &config_path).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();

        // Original server should still exist
        assert!(json["mcpServers"]["other-server"].is_object());
        // VibeShell should be added
        assert!(json["mcpServers"]["vibeshell"].is_object());
    }

    #[test]
    fn test_uninstall() {
        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config(&dir, r#"{
            "mcpServers": {
                "vibeshell": {
                    "command": "vshell"
                },
                "other": {
                    "command": "other"
                }
            }
        }"#);

        uninstall_from_tool("claude-code", &config_path).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();

        assert!(json["mcpServers"]["vibeshell"].is_null());
        assert!(json["mcpServers"]["other"].is_object());
    }

    #[test]
    fn test_backup_created() {
        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config(&dir, r#"{"existing": true}"#);

        let result = install_to_tool("claude-code", &config_path).unwrap();
        assert!(result.is_some());

        let backup_path = result.unwrap();
        assert!(backup_path.exists());
    }

    #[test]
    fn test_codex_format() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        install_to_tool("codex", &config_path).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();

        // Codex format: vibeshell at root level, not under mcpServers
        assert!(json["vibeshell"].is_object());
        assert_eq!(json["vibeshell"]["command"], "vshell");
    }

    #[test]
    fn test_cursor_mcpservers_json_uses_root_level_format() {
        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config_with_name(
            &dir,
            "mcpServers.json",
            r#"{
                "other": {
                    "command": "other"
                }
            }"#,
        );

        install_to_tool("cursor", &config_path).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();

        assert!(json["vibeshell"].is_object());
        assert!(json["mcpServers"].is_null());
        assert!(json["other"].is_object());
    }

    #[test]
    fn test_cursor_mcpservers_json_uninstall_removes_root_level_vibeshell() {
        let dir = TempDir::new().unwrap();
        let config_path = create_temp_config_with_name(
            &dir,
            "mcpServers.json",
            r#"{
                "vibeshell": {
                    "command": "vshell"
                },
                "other": {
                    "command": "other"
                }
            }"#,
        );

        uninstall_from_tool("cursor", &config_path).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();

        assert!(json["vibeshell"].is_null());
        assert!(json["other"].is_object());
    }
}
