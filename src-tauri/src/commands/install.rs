//! Tauri commands for installing VibeShell Gateway skills into AI coding tools.

use crate::install;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolInfo {
    pub id: String,
    pub name: String,
    pub config_path: String,
    pub installed: bool,
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

#[tauri::command]
pub fn detect_ai_tools() -> Vec<AiToolInfo> {
    install::detect_ai_tools()
        .into_iter()
        .map(AiToolInfo::from)
        .collect()
}

#[tauri::command]
pub fn install_to_tool(tool_id: String) -> Result<(), String> {
    let result = install::install_by_id(&tool_id).map_err(|error| error.to_string())?;
    if result.success {
        Ok(())
    } else {
        Err(result.error.unwrap_or_else(|| "Unknown error".to_string()))
    }
}

#[tauri::command]
pub fn uninstall_from_tool(tool_id: String) -> Result<(), String> {
    let result = install::uninstall_by_id(&tool_id).map_err(|error| error.to_string())?;
    if result.success {
        Ok(())
    } else {
        Err(result.error.unwrap_or_else(|| "Unknown error".to_string()))
    }
}
