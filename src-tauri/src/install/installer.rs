//! Install and remove the native VibeShell CLI skill for AI coding tools.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::PathBuf;

use super::detector::{
    find_tool, skill_dir_candidates_for_tool, skill_install_dirs_for_tool, AiTool,
};

const SKILL_DIR_NAME: &str = "vibeshell";
const LEGACY_SKILL_DIR_NAME: &str = "vshell";
const NATIVE_SKILL_MD_TEMPLATE: &str = include_str!("../../../skills/vibeshell/SKILL.md");

fn render_skill_md() -> Result<String> {
    Ok(NATIVE_SKILL_MD_TEMPLATE.to_string())
}

fn install_skill_file(tool_id: &str) -> Result<Vec<PathBuf>> {
    let skills_dirs = skill_install_dirs_for_tool(tool_id)
        .ok_or_else(|| anyhow!("No skills directory known for tool: {}", tool_id))?;
    install_skill_file_to_dirs(skills_dirs)
}

fn write_if_changed(path: &PathBuf, content: &str) -> Result<()> {
    if fs::read_to_string(path)
        .map(|existing| existing == content)
        .unwrap_or(false)
    {
        return Ok(());
    }
    fs::write(path, content).with_context(|| format!("Failed to write skill file {:?}", path))
}

fn install_skill_file_to_dirs(skills_dirs: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let skill_content = render_skill_md()?;
    let mut installed_paths = Vec::new();

    for skills_dir in skills_dirs {
        let skill_dir = skills_dir.join(SKILL_DIR_NAME);
        fs::create_dir_all(&skill_dir)
            .with_context(|| format!("Failed to create skill directory {:?}", skill_dir))?;

        let skill_path = skill_dir.join("SKILL.md");
        write_if_changed(&skill_path, &skill_content)?;

        // Remove the obsolete Node gateway helper from prior installations. The
        // native skill talks directly to the `vibeshell` executable.
        let legacy_gateway = skill_dir.join("gateway.mjs");
        if legacy_gateway.exists() {
            fs::remove_file(&legacy_gateway).with_context(|| {
                format!(
                    "Failed to remove legacy gateway client {:?}",
                    legacy_gateway
                )
            })?;
        }

        let legacy_skill_dir = skills_dir.join(LEGACY_SKILL_DIR_NAME);
        if legacy_skill_dir.exists() && legacy_skill_dir != skill_dir {
            fs::remove_dir_all(&legacy_skill_dir).with_context(|| {
                format!(
                    "Failed to remove legacy skill directory {:?}",
                    legacy_skill_dir
                )
            })?;
        }

        log::info!(
            "[Install] Native VibeShell skill installed to {:?}",
            skill_path
        );
        installed_paths.push(skill_path);
    }

    Ok(installed_paths)
}

fn uninstall_skill_file(tool_id: &str) -> Result<()> {
    if let Some(skills_dirs) = skill_dir_candidates_for_tool(tool_id) {
        for skills_dir in skills_dirs {
            for dir_name in [SKILL_DIR_NAME, LEGACY_SKILL_DIR_NAME] {
                let skill_dir = skills_dir.join(dir_name);
                if skill_dir.exists() {
                    fs::remove_dir_all(&skill_dir).with_context(|| {
                        format!("Failed to remove skill directory {:?}", skill_dir)
                    })?;
                    log::info!("[Install] VibeShell skill removed from {:?}", skill_dir);
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct InstallResult {
    pub tool: AiTool,
    pub success: bool,
    pub backup_path: Option<PathBuf>,
    pub error: Option<String>,
}

pub fn install_to_tool(tool_id: &str, _config_path: &PathBuf) -> Result<Option<PathBuf>> {
    let skill_paths = install_skill_file(tool_id)?;
    Ok(skill_paths.into_iter().next())
}

pub fn uninstall_from_tool(tool_id: &str, _config_path: &PathBuf) -> Result<Option<PathBuf>> {
    uninstall_skill_file(tool_id)?;
    Ok(None)
}

pub fn install_by_id(tool_id: &str) -> Result<InstallResult> {
    let tool = find_tool(tool_id).ok_or_else(|| anyhow!("Unknown tool: {}", tool_id))?;
    match install_to_tool(&tool.id, &tool.config_path) {
        Ok(path) => Ok(InstallResult {
            tool,
            success: true,
            backup_path: path,
            error: None,
        }),
        Err(error) => Ok(InstallResult {
            tool,
            success: false,
            backup_path: None,
            error: Some(error.to_string()),
        }),
    }
}

pub fn uninstall_by_id(tool_id: &str) -> Result<InstallResult> {
    let tool = find_tool(tool_id).ok_or_else(|| anyhow!("Unknown tool: {}", tool_id))?;
    match uninstall_from_tool(&tool.id, &tool.config_path) {
        Ok(_) => Ok(InstallResult {
            tool,
            success: true,
            backup_path: None,
            error: None,
        }),
        Err(error) => Ok(InstallResult {
            tool,
            success: false,
            backup_path: None,
            error: Some(error.to_string()),
        }),
    }
}

pub fn install_to_all() -> Vec<InstallResult> {
    use super::detector::get_default_install_tools;

    get_default_install_tools()
        .into_iter()
        .map(|tool| match install_to_tool(&tool.id, &tool.config_path) {
            Ok(path) => InstallResult {
                tool,
                success: true,
                backup_path: path,
                error: None,
            },
            Err(error) => InstallResult {
                tool,
                success: false,
                backup_path: None,
                error: Some(error.to_string()),
            },
        })
        .collect()
}

pub fn uninstall_from_all() -> Vec<InstallResult> {
    use super::detector::get_configured_tools;

    get_configured_tools()
        .into_iter()
        .map(
            |tool| match uninstall_from_tool(&tool.id, &tool.config_path) {
                Ok(_) => InstallResult {
                    tool,
                    success: true,
                    backup_path: None,
                    error: None,
                },
                Err(error) => InstallResult {
                    tool,
                    success: false,
                    backup_path: None,
                    error: Some(error.to_string()),
                },
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn skill_content_uses_the_native_cli_contract() {
        let content = render_skill_md().unwrap();
        for required in [
            "name: vibeshell",
            "native `vibeshell` executable",
            "vibeshell servers",
            "vibeshell ssh <server>",
            "vibeshell sessions",
            "vibeshell sftp",
            "vibeshell import auto",
            "headless daemon",
        ] {
            assert!(content.contains(required), "missing {required}");
        }
        assert!(!content.contains("node gateway.mjs"));
        assert!(!content.contains("Agent Gateway"));
        assert!(!content.contains("agent-gateway.json"));
    }

    #[test]
    fn repository_skill_mirrors_stay_in_sync() {
        let canonical = render_skill_md().unwrap();
        assert_eq!(
            canonical,
            include_str!("../../../.claude/skills/vibeshell/SKILL.md")
        );
        assert_eq!(
            canonical,
            include_str!("../../../.codex/skills/vibeshell/SKILL.md")
        );
    }

    #[test]
    fn skill_install_is_confined_to_injected_directories() {
        let dir = TempDir::new().unwrap();
        let skills_dir = dir.path().join("skills");
        let installed = install_skill_file_to_dirs(vec![skills_dir.clone()]).unwrap();
        let skill_path = skills_dir.join(SKILL_DIR_NAME).join("SKILL.md");

        assert_eq!(installed, vec![skill_path.clone()]);
        let content = fs::read_to_string(skill_path).unwrap();
        assert!(content.contains("name: vibeshell"));
        assert!(content.contains("native `vibeshell` executable"));
        assert!(!skills_dir.join(SKILL_DIR_NAME).join("gateway.mjs").exists());
    }
}
