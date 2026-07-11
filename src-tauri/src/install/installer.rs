//! Install and remove the VibeShell Agent Gateway skill for AI coding tools.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::PathBuf;

use super::detector::{
    find_tool, skill_dir_candidates_for_tool, skill_install_dirs_for_tool, AiTool,
};

const SKILL_DIR_NAME: &str = "vibeshell";
const LEGACY_SKILL_DIR_NAME: &str = "vshell";
const GATEWAY_SKILL_MD_TEMPLATE: &str = include_str!("gateway-skill.md");

fn render_skill_md() -> Result<String> {
    let manifest_path = crate::mcp::gateway_manifest_path()?
        .to_string_lossy()
        .into_owned();
    Ok(GATEWAY_SKILL_MD_TEMPLATE.replace("{{MANIFEST_PATH}}", &manifest_path))
}

fn install_skill_file(tool_id: &str) -> Result<Vec<PathBuf>> {
    let skills_dirs = skill_install_dirs_for_tool(tool_id)
        .ok_or_else(|| anyhow!("No skills directory known for tool: {}", tool_id))?;
    install_skill_file_to_dirs(skills_dirs)
}

fn install_skill_file_to_dirs(skills_dirs: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let skill_content = render_skill_md()?;
    let mut installed_paths = Vec::new();

    for skills_dir in skills_dirs {
        let skill_dir = skills_dir.join(SKILL_DIR_NAME);
        fs::create_dir_all(&skill_dir)
            .with_context(|| format!("Failed to create skill directory {:?}", skill_dir))?;

        let skill_path = skill_dir.join("SKILL.md");
        fs::write(&skill_path, &skill_content)
            .with_context(|| format!("Failed to write skill file {:?}", skill_path))?;

        let legacy_skill_dir = skills_dir.join(LEGACY_SKILL_DIR_NAME);
        if legacy_skill_dir.exists() && legacy_skill_dir != skill_dir {
            fs::remove_dir_all(&legacy_skill_dir).with_context(|| {
                format!(
                    "Failed to remove legacy skill directory {:?}",
                    legacy_skill_dir
                )
            })?;
        }

        log::info!("[Install] Gateway skill installed to {:?}", skill_path);
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
                    log::info!("[Install] Gateway skill removed from {:?}", skill_dir);
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
    use super::detector::get_installed_tools;

    get_installed_tools()
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
    fn skill_content_uses_the_gateway_contract() {
        let content = render_skill_md().unwrap();
        for required in [
            "name: vibeshell",
            "Agent Gateway",
            "Authorization: Bearer",
            "server_list",
            "session_create",
            "session_send_input",
            "session_read",
            "sftp_upload_directory",
            "sftp_sync_directory",
            "get_content",
            "edit_file",
            "add_file",
            "macOS",
            "Linux",
            "Windows",
        ] {
            assert!(content.contains(required), "missing {required}");
        }
        assert!(!content.contains("vshell daemon"));
        assert!(!content.contains("vshell ssh"));
        assert!(!content.contains("{{MANIFEST_PATH}}"));
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
        assert!(content.contains("Agent Gateway"));
    }
}
