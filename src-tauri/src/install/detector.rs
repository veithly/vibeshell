//! AI tool detection for VibeShell skill installation.
//!
//! This module detects installed AI coding tools and checks whether
//! the VibeShell SKILL.md is installed in each tool's skills directory.
//!
//! Installed-tool detection checks known config and skill roots. VibeShell
//! installation status is based on SKILL.md presence only — we do NOT modify
//! or require MCP config files (mcp.json, etc.).

use std::path::{Path, PathBuf};

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
    fn from_definition(definition: &ToolDefinition) -> Self {
        let config_path = select_preferred_config_path(&definition.config_candidates)
            .unwrap_or_else(|| PathBuf::from("mcp.json"));

        let installed = definition
            .config_candidates
            .iter()
            .any(|path| config_candidate_exists(path))
            || definition
                .skill_dir_candidates
                .iter()
                .any(|path| path.exists() || path.parent().map(|p| p.exists()).unwrap_or(false));

        let vibeshell_installed = check_skill_installed_in_dirs(&definition.skill_dir_candidates);

        Self {
            id: definition.id.to_string(),
            name: definition.name.to_string(),
            config_path,
            installed,
            vibeshell_installed,
        }
    }
}

#[derive(Debug, Clone)]
struct ToolDefinition {
    id: &'static str,
    name: &'static str,
    config_candidates: Vec<PathBuf>,
    skill_dir_candidates: Vec<PathBuf>,
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

fn config_candidate_exists(path: &Path) -> bool {
    if path.exists() {
        return true;
    }

    if path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
    {
        return false;
    }

    path.parent().map(|parent| parent.exists()).unwrap_or(false)
}

fn select_install_skill_dirs(candidates: &[PathBuf]) -> Vec<PathBuf> {
    let mut selected = unique_paths(
        candidates
            .iter()
            .filter(|path| path.exists() || path.parent().map(|p| p.exists()).unwrap_or(false))
            .cloned()
            .collect(),
    );

    if selected.is_empty() {
        if let Some(default) = candidates.first() {
            selected.push(default.clone());
        }
    }

    selected
}

fn unique_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if !unique.iter().any(|existing| existing == &path) {
            unique.push(path);
        }
    }
    unique
}

fn check_skill_installed_in_dirs(skill_dirs: &[PathBuf]) -> bool {
    skill_dirs.iter().any(|skills_dir| {
        SKILL_DIR_NAMES
            .iter()
            .any(|dir_name| skills_dir.join(dir_name).join("SKILL.md").exists())
    })
}

pub fn skill_dir_candidates_for_tool(tool_id: &str) -> Option<Vec<PathBuf>> {
    let home = get_home_dir()?;
    tool_definitions(&home)
        .into_iter()
        .find(|definition| definition.id == tool_id)
        .map(|definition| definition.skill_dir_candidates)
}

pub fn skill_install_dirs_for_tool(tool_id: &str) -> Option<Vec<PathBuf>> {
    skill_dir_candidates_for_tool(tool_id).map(|candidates| select_install_skill_dirs(&candidates))
}

fn tool_definitions(home: &Path) -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            id: "claude-code",
            name: "Claude Code",
            config_candidates: vec![
                home.join(".claude").join("mcp.json"),
                home.join(".claude.json"),
                home.join(".config").join("claude").join("mcp.json"),
                home.join(".config").join("claude-code").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".claude").join("skills"),
                home.join(".config").join("claude").join("skills"),
                home.join(".config").join("claude-code").join("skills"),
            ],
        },
        ToolDefinition {
            id: "cursor",
            name: "Cursor",
            config_candidates: vec![
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
            skill_dir_candidates: vec![
                home.join(".cursor").join("skills"),
                home.join(".config")
                    .join("Cursor")
                    .join("User")
                    .join("skills"),
                home.join(".config").join("cursor").join("skills"),
            ],
        },
        ToolDefinition {
            id: "codex",
            name: "Codex",
            config_candidates: vec![
                home.join(".codex").join("config.toml"),
                home.join(".codex").join("config.json"),
                home.join(".codex").join("mcp.json"),
                home.join(".config").join("codex").join("config.toml"),
                home.join(".config").join("codex").join("config.json"),
                home.join(".config").join("codex").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".agents").join("skills"),
                home.join(".codex").join("skills"),
                home.join(".config").join("codex").join("skills"),
            ],
        },
        ToolDefinition {
            id: "opencode",
            name: "Open Code",
            config_candidates: vec![
                home.join(".opencode").join("mcp.json"),
                home.join(".config").join("opencode").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".opencode").join("skills"),
                home.join(".config").join("opencode").join("skills"),
            ],
        },
        ToolDefinition {
            id: "gemini-cli",
            name: "Gemini CLI",
            config_candidates: vec![
                home.join(".gemini").join("settings.json"),
                home.join(".config")
                    .join("gemini-cli")
                    .join("settings.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".gemini").join("skills"),
                home.join(".config").join("gemini-cli").join("skills"),
                home.join(".agents").join("skills"),
            ],
        },
        ToolDefinition {
            id: "openclaw",
            name: "OpenClaw",
            config_candidates: vec![
                home.join(".openclaw").join("openclaw.json"),
                home.join(".config").join("openclaw").join("openclaw.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".openclaw").join("skills"),
                home.join(".config").join("openclaw").join("skills"),
            ],
        },
        ToolDefinition {
            id: "windsurf",
            name: "Windsurf",
            config_candidates: vec![
                home.join(".codeium")
                    .join("windsurf")
                    .join("mcp_config.json"),
                home.join(".codeium").join("windsurf").join("mcp.json"),
                home.join(".windsurf").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".codeium").join("windsurf").join("skills"),
                home.join(".windsurf").join("skills"),
            ],
        },
        ToolDefinition {
            id: "roo-code",
            name: "Roo Code",
            config_candidates: vec![
                home.join(".roo").join("mcp.json"),
                home.join(".config").join("roo-code").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".roo").join("skills"),
                home.join(".config").join("roo-code").join("skills"),
            ],
        },
        ToolDefinition {
            id: "augment",
            name: "Augment Code",
            config_candidates: vec![
                home.join(".augment").join("config.json"),
                home.join(".augment").join("mcp.json"),
                home.join(".config").join("augment").join("config.json"),
                home.join(".config").join("augment").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".augment").join("skills"),
                home.join(".config").join("augment").join("skills"),
            ],
        },
        ToolDefinition {
            id: "continue",
            name: "Continue",
            config_candidates: vec![
                home.join(".continue").join("config.json"),
                home.join(".continue").join("mcp.json"),
                home.join(".config").join("continue").join("config.json"),
                home.join(".config").join("continue").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".continue").join("skills"),
                home.join(".config").join("continue").join("skills"),
            ],
        },
        ToolDefinition {
            id: "kiro",
            name: "Kiro",
            config_candidates: vec![
                home.join(".kiro").join("settings.json"),
                home.join(".kiro").join("mcp.json"),
                home.join(".config").join("kiro").join("settings.json"),
                home.join(".config").join("kiro").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".kiro").join("skills"),
                home.join(".config").join("kiro").join("skills"),
            ],
        },
        ToolDefinition {
            id: "trae",
            name: "Trae",
            config_candidates: vec![
                home.join(".trae").join("settings.json"),
                home.join(".trae").join("mcp.json"),
                home.join(".config").join("trae").join("settings.json"),
                home.join(".config").join("trae").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".trae").join("skills"),
                home.join(".config").join("trae").join("skills"),
            ],
        },
        ToolDefinition {
            id: "openhands",
            name: "OpenHands",
            config_candidates: vec![
                home.join(".openhands").join("config.json"),
                home.join(".openhands").join("mcp.json"),
                home.join(".config").join("openhands").join("config.json"),
                home.join(".config").join("openhands").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".openhands").join("skills"),
                home.join(".config").join("openhands").join("skills"),
            ],
        },
        ToolDefinition {
            id: "agents",
            name: "Agents (Universal)",
            config_candidates: vec![home.join(".agents").join("config.json")],
            skill_dir_candidates: vec![home.join(".agents").join("skills")],
        },
        ToolDefinition {
            id: "stepfun",
            name: "StepFun",
            config_candidates: vec![
                home.join(".stepfun").join("config.json"),
                home.join(".stepfun").join("mcp.json"),
                home.join(".config").join("stepfun").join("config.json"),
                home.join(".config").join("stepfun").join("mcp.json"),
            ],
            skill_dir_candidates: vec![
                home.join(".stepfun").join("skills"),
                home.join(".config").join("stepfun").join("skills"),
            ],
        },
    ]
}

/// Detect all supported AI tools and their skill installation status.
///
/// Returns a list of all known AI tools with their current status,
/// including whether they are installed and whether the VibeShell skill is installed.
pub fn detect_ai_tools() -> Vec<AiTool> {
    get_home_dir()
        .map(|home| {
            tool_definitions(&home)
                .iter()
                .map(AiTool::from_definition)
                .collect()
        })
        .unwrap_or_default()
}

/// Find a specific AI tool by its ID.
pub fn find_tool(tool_id: &str) -> Option<AiTool> {
    detect_ai_tools().into_iter().find(|t| t.id == tool_id)
}

/// Get all installed AI tools (tools whose known config or skill roots exist).
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
    fn test_config_candidate_exists_does_not_count_missing_home_dotfile_parent() {
        let dir = TempDir::new().unwrap();
        let dotfile = dir.path().join(".claude.json");

        assert!(!config_candidate_exists(&dotfile));
    }

    #[test]
    fn test_config_candidate_exists_counts_existing_tool_directory() {
        let dir = TempDir::new().unwrap();
        let config = dir.path().join(".cursor").join("mcp.json");
        fs::create_dir_all(config.parent().unwrap()).unwrap();

        assert!(config_candidate_exists(&config));
    }

    #[test]
    fn test_select_install_skill_dirs_prefers_existing_tool_roots() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("missing").join("skills");
        let existing = dir.path().join("existing").join("skills");
        fs::create_dir_all(existing.parent().unwrap()).unwrap();

        let selected = select_install_skill_dirs(&[missing, existing.clone()]);
        assert_eq!(selected, vec![existing]);
    }

    #[test]
    fn test_select_install_skill_dirs_falls_back_to_default() {
        let dir = TempDir::new().unwrap();
        let default = dir.path().join("default").join("skills");
        let alternate = dir.path().join("alternate").join("skills");

        let selected = select_install_skill_dirs(&[default.clone(), alternate]);
        assert_eq!(selected, vec![default]);
    }

    #[test]
    fn test_codex_skill_candidates_include_agents_and_legacy_codex_dirs() {
        let dir = TempDir::new().unwrap();
        let definitions = tool_definitions(dir.path());
        let codex = definitions
            .iter()
            .find(|definition| definition.id == "codex")
            .expect("codex definition should exist");

        assert!(codex
            .skill_dir_candidates
            .contains(&dir.path().join(".agents").join("skills")));
        assert!(codex
            .skill_dir_candidates
            .contains(&dir.path().join(".codex").join("skills")));
    }

    #[test]
    fn test_skill_dir_names_include_current_and_legacy_names() {
        assert!(SKILL_DIR_NAMES.contains(&"vshell"));
        assert!(SKILL_DIR_NAMES.contains(&"vibeshell"));
    }
}
