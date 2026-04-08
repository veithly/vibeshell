//! AI tool detection for VibeShell skill installation.
//!
//! This module detects installed AI coding tools and checks whether
//! the VibeShell SKILL.md is installed in each tool's skills directory.
//!
//! Detection is based on SKILL.md presence only — we do NOT check
//! MCP config files (mcp.json, etc.).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const SKILL_DIR_NAMES: [&str; 2] = ["vshell", "vibeshell"];

/// Represents an AI coding tool that can have the VibeShell skill installed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTool {
    /// Unique identifier for the tool (e.g., "claude-code", "cursor")
    pub id: String,
    /// Human-readable name of the tool
    pub name: String,
    /// Path to the tool's configuration file (kept for API compat)
    pub config_path: PathBuf,
    /// Whether the AI tool is detected/installed on the system
    pub installed: bool,
    /// Whether the VibeShell SKILL.md is installed in the tool
    pub vibeshell_installed: bool,
}

impl AiTool {
    /// Create a new AiTool instance.
    ///
    /// Detection:
    /// - `installed`: true if any of the candidate directories exist
    /// - `vibeshell_installed`: true if SKILL.md exists in the skills dir
    fn from_candidates(id: &str, name: &str, candidates: Vec<PathBuf>) -> Self {
        let config_path =
            select_preferred_config_path(&candidates).unwrap_or_else(|| PathBuf::from("mcp.json"));

        let installed = candidates
            .iter()
            .any(|path| path.parent().map(|p| p.exists()).unwrap_or(false));

        let vibeshell_installed = check_skill_installed(id);

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
fn select_preferred_config_path(candidates: &[PathBuf]) -> Option<PathBuf> {
    if let Some(existing) = candidates.iter().find(|path| path.exists()) {
        return Some(existing.clone());
    }

    candidates.first().cloned()
}

/// Check if the VibeShell SKILL.md is installed for the given tool.
///
/// Only checks the skills directory — does NOT inspect MCP configs.
fn check_skill_installed(tool_id: &str) -> bool {
    let home = match get_home_dir() {
        Some(h) => h,
        None => return false,
    };

    let skills_dir = match tool_id {
        "claude-code" => home.join(".claude").join("skills"),
        "cursor" => home.join(".cursor").join("skills"),
        "codex" => home.join(".codex").join("skills"),
        "opencode" => home.join(".opencode").join("skills"),
        "gemini-cli" => home.join(".gemini").join("skills"),
        "openclaw" => home.join(".openclaw").join("skills"),
        "windsurf" => home.join(".codeium").join("windsurf").join("skills"),
        "roo-code" => home.join(".roo").join("skills"),
        "augment" => home.join(".augment").join("skills"),
        "continue" => home.join(".continue").join("skills"),
        "kiro" => home.join(".kiro").join("skills"),
        "trae" => home.join(".trae").join("skills"),
        "openhands" => home.join(".openhands").join("skills"),
        "agents" => home.join(".agents").join("skills"),
        "stepfun" => home.join(".stepfun").join("skills"),
        _ => return false,
    };

    SKILL_DIR_NAMES
        .iter()
        .any(|dir_name| skills_dir.join(dir_name).join("SKILL.md").exists())
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
                home.join(".config")
                    .join("Cursor")
                    .join("User")
                    .join("mcp.json"),
                home.join(".config")
                    .join("Cursor")
                    .join("User")
                    .join("mcpServers.json"),
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
                home.join(".config")
                    .join("gemini-cli")
                    .join("settings.json"),
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

        // Windsurf (Codeium): uses ~/.codeium/windsurf/ for config and skills
        tools.push(AiTool::from_candidates(
            "windsurf",
            "Windsurf",
            vec![
                home.join(".codeium")
                    .join("windsurf")
                    .join("mcp_config.json"),
                home.join(".codeium").join("windsurf").join("mcp.json"),
            ],
        ));

        // Roo Code: global skills at ~/.roo/skills/
        tools.push(AiTool::from_candidates(
            "roo-code",
            "Roo Code",
            vec![
                home.join(".roo").join("mcp.json"),
                home.join(".config").join("roo-code").join("mcp.json"),
            ],
        ));

        // Augment Code: AI coding assistant
        tools.push(AiTool::from_candidates(
            "augment",
            "Augment Code",
            vec![
                home.join(".augment").join("config.json"),
                home.join(".augment").join("mcp.json"),
            ],
        ));

        // Continue: open-source AI code assistant
        tools.push(AiTool::from_candidates(
            "continue",
            "Continue",
            vec![
                home.join(".continue").join("config.json"),
                home.join(".continue").join("mcp.json"),
            ],
        ));

        // Kiro (AWS): AI coding IDE
        tools.push(AiTool::from_candidates(
            "kiro",
            "Kiro",
            vec![
                home.join(".kiro").join("settings.json"),
                home.join(".kiro").join("mcp.json"),
            ],
        ));

        // Trae (ByteDance): AI IDE
        tools.push(AiTool::from_candidates(
            "trae",
            "Trae",
            vec![
                home.join(".trae").join("settings.json"),
                home.join(".trae").join("mcp.json"),
            ],
        ));

        // OpenHands: open-source AI agent platform
        tools.push(AiTool::from_candidates(
            "openhands",
            "OpenHands",
            vec![
                home.join(".openhands").join("config.json"),
                home.join(".openhands").join("mcp.json"),
            ],
        ));

        // Universal .agents directory (shared by Amp, Gemini CLI, GitHub Copilot, etc.)
        tools.push(AiTool::from_candidates(
            "agents",
            "Agents (Universal)",
            vec![home.join(".agents").join("config.json")],
        ));

        // StepFun: AI coding assistant
        tools.push(AiTool::from_candidates(
            "stepfun",
            "StepFun",
            vec![
                home.join(".stepfun").join("config.json"),
                home.join(".stepfun").join("mcp.json"),
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
    use tempfile::TempDir;

    #[test]
    fn test_detect_ai_tools() {
        let tools = detect_ai_tools();
        assert_eq!(tools.len(), 15);

        let tool_ids: Vec<&str> = tools.iter().map(|t| t.id.as_str()).collect();
        assert!(tool_ids.contains(&"claude-code"));
        assert!(tool_ids.contains(&"cursor"));
        assert!(tool_ids.contains(&"codex"));
        assert!(tool_ids.contains(&"opencode"));
        assert!(tool_ids.contains(&"gemini-cli"));
        assert!(tool_ids.contains(&"openclaw"));
        assert!(tool_ids.contains(&"windsurf"));
        assert!(tool_ids.contains(&"roo-code"));
        assert!(tool_ids.contains(&"augment"));
        assert!(tool_ids.contains(&"continue"));
        assert!(tool_ids.contains(&"kiro"));
        assert!(tool_ids.contains(&"trae"));
        assert!(tool_ids.contains(&"openhands"));
        assert!(tool_ids.contains(&"agents"));
        assert!(tool_ids.contains(&"stepfun"));
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
    fn test_skill_dir_names_include_current_and_legacy_names() {
        assert!(SKILL_DIR_NAMES.contains(&"vshell"));
        assert!(SKILL_DIR_NAMES.contains(&"vibeshell"));
    }
}
