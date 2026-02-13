//! AI tool detection for VibeShell skill configuration.
//!
//! This module provides functionality to detect installed AI coding tools
//! and check whether VibeShell skill is configured in each tool.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Represents an AI coding tool that can have the VibeShell skill installed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTool {
    /// Unique identifier for the tool (e.g., "claude-code", "cursor")
    pub id: String,
    /// Human-readable name of the tool
    pub name: String,
    /// Path to the tool's configuration file
    pub config_path: PathBuf,
    /// Whether the AI tool is detected/installed on the system
    pub installed: bool,
    /// Whether the VibeShell skill is installed in the tool
    pub vibeshell_installed: bool,
}

impl AiTool {
    /// Create a new AiTool instance from multiple candidate config paths.
    ///
    /// Selection rule:
    /// 1. Prefer the first existing config file path
    /// 2. Otherwise fall back to the default path (first candidate)
    fn from_candidates(id: &str, name: &str, candidates: Vec<PathBuf>) -> Self {
        let config_path = select_preferred_config_path(&candidates)
            .unwrap_or_else(|| PathBuf::from("mcp.json"));

        let installed = candidates
            .iter()
            .any(|path| path.parent().map(|p| p.exists()).unwrap_or(false));

        let vibeshell_installed = check_vibeshell_installed(&config_path).unwrap_or(false);

        Self {
            id: id.to_string(),
            name: name.to_string(),
            config_path,
            installed,
            vibeshell_installed,
        }
    }
}

/// Get the user's home directory.
fn get_home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

/// Select preferred config path from candidates.
///
/// Rule: existing file path first, otherwise default path (first candidate).
fn select_preferred_config_path(candidates: &[PathBuf]) -> Option<PathBuf> {
    if let Some(existing) = candidates.iter().find(|path| path.exists()) {
        return Some(existing.clone());
    }

    candidates.first().cloned()
}

/// Check if the VibeShell skill is installed in the given config file.
///
/// Checks both the MCP config (mcp.json) and the skills directory (skills/vibeshell/SKILL.md).
fn check_vibeshell_installed(config_path: &PathBuf) -> Result<bool> {
    // Check MCP config
    if config_path.exists() {
        let content = std::fs::read_to_string(config_path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;

        // Nested MCP format: { "mcpServers": { "vibeshell": { ... } } }
        if let Some(mcp_servers) = json.get("mcpServers").and_then(|v| v.as_object()) {
            if mcp_servers.get("vibeshell").is_some() {
                return Ok(true);
            }
        }

        // Root format: { "vibeshell": { ... } }
        if json.get("vibeshell").is_some() {
            return Ok(true);
        }
    }

    // Also check for skill file in the skills directory
    if let Some(parent) = config_path.parent() {
        let skill_path = parent.join("skills").join("vibeshell").join("SKILL.md");
        if skill_path.exists() {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Detect all supported AI tools and their skill installation status.
///
/// Returns a list of all known AI tools with their current status,
/// including whether they are installed and whether the VibeShell skill is installed.
pub fn detect_ai_tools() -> Vec<AiTool> {
    let mut tools = Vec::new();

    if let Some(home) = get_home_dir() {
        // Claude Code: multi-candidate paths (existing path first, fallback to first)
        tools.push(AiTool::from_candidates(
            "claude-code",
            "Claude Code",
            vec![
                home.join(".claude").join("mcp.json"),
                home.join(".claude.json"),
                home.join(".config").join("claude").join("mcp.json"),
                home.join(".config").join("claude-code").join("mcp.json"),
            ],
        ));

        // Cursor: support both mcp.json and mcpServers.json variants
        tools.push(AiTool::from_candidates(
            "cursor",
            "Cursor",
            vec![
                home.join(".cursor").join("mcp.json"),
                home.join(".cursor").join("mcpServers.json"),
                home.join(".config").join("Cursor").join("User").join("mcp.json"),
                home.join(".config").join("Cursor").join("User").join("mcpServers.json"),
            ],
        ));

        // Codex: support both config.json and mcp.json variants
        tools.push(AiTool::from_candidates(
            "codex",
            "Codex",
            vec![
                home.join(".codex").join("config.json"),
                home.join(".codex").join("mcp.json"),
                home.join(".config").join("codex").join("config.json"),
                home.join(".config").join("codex").join("mcp.json"),
            ],
        ));

        // Open Code: legacy path
        tools.push(AiTool::from_candidates(
            "opencode",
            "Open Code",
            vec![home.join(".opencode").join("mcp.json")],
        ));

        // Gemini CLI: Google's Gemini CLI uses settings.json with mcpServers
        tools.push(AiTool::from_candidates(
            "gemini-cli",
            "Gemini CLI",
            vec![
                home.join(".gemini").join("settings.json"),
                home.join(".config").join("gemini-cli").join("settings.json"),
            ],
        ));

        // OpenClaw: AI agent gateway with MCP support via openclaw.json
        tools.push(AiTool::from_candidates(
            "openclaw",
            "OpenClaw",
            vec![
                home.join(".openclaw").join("openclaw.json"),
                home.join(".config").join("openclaw").join("openclaw.json"),
            ],
        ));
    }

    tools
}

/// Find a specific AI tool by its ID.
pub fn find_tool(tool_id: &str) -> Option<AiTool> {
    detect_ai_tools().into_iter().find(|t| t.id == tool_id)
}

/// Get all installed AI tools (tools whose config directory exists).
pub fn get_installed_tools() -> Vec<AiTool> {
    detect_ai_tools()
        .into_iter()
        .filter(|t| t.installed)
        .collect()
}

/// Get all tools that have the VibeShell skill installed.
pub fn get_configured_tools() -> Vec<AiTool> {
    detect_ai_tools()
        .into_iter()
        .filter(|t| t.vibeshell_installed)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_detect_ai_tools() {
        let tools = detect_ai_tools();
        assert_eq!(tools.len(), 6);

        let tool_ids: Vec<&str> = tools.iter().map(|t| t.id.as_str()).collect();
        assert!(tool_ids.contains(&"claude-code"));
        assert!(tool_ids.contains(&"cursor"));
        assert!(tool_ids.contains(&"codex"));
        assert!(tool_ids.contains(&"opencode"));
        assert!(tool_ids.contains(&"gemini-cli"));
        assert!(tool_ids.contains(&"openclaw"));
    }

    #[test]
    fn test_find_tool() {
        let tool = find_tool("claude-code");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name, "Claude Code");

        let unknown = find_tool("unknown-tool");
        assert!(unknown.is_none());
    }

    #[test]
    fn test_select_preferred_config_path_prefers_existing_file() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("missing.json");
        let existing = dir.path().join("existing.json");
        fs::write(&existing, "{}").unwrap();

        let selected = select_preferred_config_path(&[missing, existing.clone()]);
        assert_eq!(selected, Some(existing));
    }

    #[test]
    fn test_select_preferred_config_path_falls_back_to_default() {
        let dir = TempDir::new().unwrap();
        let default_path = dir.path().join("default.json");
        let second = dir.path().join("second.json");

        let selected = select_preferred_config_path(&[default_path.clone(), second]);
        assert_eq!(selected, Some(default_path));
    }

    #[test]
    fn test_check_vibeshell_installed_supports_both_json_shapes() {
        let dir = TempDir::new().unwrap();

        let nested_path = dir.path().join("nested.json");
        let mut nested_file = fs::File::create(&nested_path).unwrap();
        nested_file
            .write_all(br#"{"mcpServers":{"vibeshell":{"command":"vshell"}}}"#)
            .unwrap();

        let root_path = dir.path().join("root.json");
        let mut root_file = fs::File::create(&root_path).unwrap();
        root_file
            .write_all(br#"{"vibeshell":{"command":"vshell"}}"#)
            .unwrap();

        assert!(check_vibeshell_installed(&nested_path).unwrap());
        assert!(check_vibeshell_installed(&root_path).unwrap());
    }
}
