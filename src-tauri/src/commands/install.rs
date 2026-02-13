//! Tauri commands for VibeShell skill installation management.
//!
//! This module exposes skill installation functionality to the frontend via Tauri commands.
//! Skills enable AI coding tools to use VibeShell for SSH/SFTP operations.

use crate::install;
use serde::{Deserialize, Serialize};

/// Serializable AI tool info for frontend consumption.
/// Maps the AiTool struct from install::detector to a frontend-friendly format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolInfo {
    /// Unique identifier for the tool
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Path to the config file
    pub config_path: String,
    /// Whether the AI tool is detected/installed on the system
    pub installed: bool,
    /// Whether the VibeShell skill is installed in this tool
    pub vibeshell_installed: bool,
}

impl From<install::AiTool> for AiToolInfo {
    fn from(tool: install::AiTool) -> Self {
        Self {
            id: tool.id,
            name: tool.name,
            config_path: tool.config_path.to_string_lossy().to_string(),
            installed: tool.installed,
            vibeshell_installed: tool.vibeshell_installed,
        }
    }
}

/// Detect all AI tools and their skill installation status.
///
/// Returns a list of all supported AI tools with their current status,
/// including whether the VibeShell skill is installed.
#[tauri::command]
pub fn detect_ai_tools() -> Vec<AiToolInfo> {
    install::detect_ai_tools()
        .into_iter()
        .map(AiToolInfo::from)
        .collect()
}

/// Install VibeShell skill to a specific AI tool.
///
/// This configures the AI tool to use VibeShell for SSH/SFTP operations.
///
/// # Arguments
/// * `tool_id` - The ID of the tool to install to (e.g., "claude-code", "cursor")
///
/// # Returns
/// * `Ok(())` - Skill installation was successful
/// * `Err(String)` - Error message if installation failed
#[tauri::command]
pub fn install_to_tool(tool_id: String) -> Result<(), String> {
    let result = install::install_by_id(&tool_id).map_err(|e| e.to_string())?;

    if result.success {
        Ok(())
    } else {
        Err(result.error.unwrap_or_else(|| "Unknown error".to_string()))
    }
}

/// Uninstall VibeShell skill from a specific AI tool.
///
/// This removes VibeShell configuration from the AI tool.
///
/// # Arguments
/// * `tool_id` - The ID of the tool to uninstall from
///
/// # Returns
/// * `Ok(())` - Skill uninstallation was successful
/// * `Err(String)` - Error message if uninstallation failed
#[tauri::command]
pub fn uninstall_from_tool(tool_id: String) -> Result<(), String> {
    let result = install::uninstall_by_id(&tool_id).map_err(|e| e.to_string())?;

    if result.success {
        Ok(())
    } else {
        Err(result.error.unwrap_or_else(|| "Unknown error".to_string()))
    }
}
