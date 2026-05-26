//! Tauri commands for VibeShell skill installation management.
//!
//! This module exposes skill installation functionality to the frontend via Tauri commands.
//! Skills enable AI coding tools to use VibeShell for SSH/SFTP operations.

use crate::install;
use crate::install::installer::resolve_vshell_binary;
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

/// Get the resolved path to the vshell binary.
///
/// Returns the absolute path if found, or the bare command name if not.
#[tauri::command]
pub fn get_vshell_path() -> String {
    resolve_vshell_binary()
}

/// Add vshell to the system PATH.
///
/// On Windows: adds the directory containing vshell.exe to the user's PATH registry.
/// On macOS/Linux: creates a symlink in /usr/local/bin.
#[tauri::command]
pub fn add_vshell_to_path() -> Result<String, String> {
    let vshell_path = resolve_vshell_binary();
    let vshell = std::path::Path::new(&vshell_path);

    if !vshell.exists() {
        return Err(format!(
            "vshell binary not found at '{}'. Build the CLI first with: cargo build --release -p vshell",
            vshell_path
        ));
    }

    #[cfg(windows)]
    {
        let vshell_dir = vshell.parent().ok_or("Cannot determine vshell directory")?;
        add_to_windows_path(vshell_dir).map_err(|e| e.to_string())
    }

    #[cfg(not(windows))]
    {
        add_to_unix_path(vshell).map_err(|e| e.to_string())
    }
}

/// Add a directory to the Windows user PATH via the registry.
#[cfg(windows)]
fn add_to_windows_path(dir: &std::path::Path) -> Result<String, String> {
    use std::process::Command;

    let dir_str = dir.to_string_lossy();

    // Read current user PATH
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Environment]::GetEnvironmentVariable('PATH', 'User')",
        ])
        .output()
        .map_err(|e| format!("Failed to read PATH: {}", e))?;

    let current_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Check if already in PATH
    for entry in current_path.split(';') {
        if entry.eq_ignore_ascii_case(&dir_str) {
            return Ok(format!("'{}' is already in PATH", dir_str));
        }
    }

    // Add to PATH
    let new_path = if current_path.is_empty() {
        dir_str.to_string()
    } else {
        format!("{};{}", current_path, dir_str)
    };

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "[Environment]::SetEnvironmentVariable('PATH', '{}', 'User')",
                new_path.replace('\'', "''")
            ),
        ])
        .status()
        .map_err(|e| format!("Failed to update PATH: {}", e))?;

    if status.success() {
        Ok(format!(
            "Added '{}' to user PATH. Restart your terminal for changes to take effect.",
            dir_str
        ))
    } else {
        Err("Failed to update PATH registry".to_string())
    }
}

/// Add vshell to PATH on Unix by creating a symlink.
#[cfg(not(windows))]
fn add_to_unix_path(vshell: &std::path::Path) -> Result<String, String> {
    let link_path = std::path::Path::new("/usr/local/bin/vshell");

    if link_path.exists() {
        return Ok("vshell is already available at /usr/local/bin/vshell".to_string());
    }

    // Try to create symlink (may need sudo)
    match std::os::unix::fs::symlink(vshell, link_path) {
        Ok(_) => Ok(format!(
            "Created symlink: /usr/local/bin/vshell -> {}",
            vshell.display()
        )),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                Err(format!(
                    "Permission denied. Run: sudo ln -sf {} /usr/local/bin/vshell",
                    vshell.display()
                ))
            } else {
                Err(format!("Failed to create symlink: {}", e))
            }
        }
    }
}
