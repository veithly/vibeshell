//! Tauri commands for VibeShell skill installation management.
//!
//! This module exposes skill installation functionality to the frontend via Tauri commands.
//! Skills enable AI coding tools to use VibeShell for SSH/SFTP operations.

use crate::install;
use crate::install::installer::{is_usable_vshell_binary, resolve_vshell_binary};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

/// Serializable status for the bundled vshell CLI and system PATH lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VshellStatus {
    /// Resolved path to the bundled/preferred vshell binary.
    pub binary_path: String,
    /// Whether the resolved binary exists and is executable.
    pub binary_exists: bool,
    /// First vshell found through PATH, if any.
    pub path_entry: Option<String>,
    /// Whether `vshell` can be resolved from PATH.
    pub path_installed: bool,
    /// Whether the PATH entry resolves to the bundled/preferred binary.
    pub path_matches_binary: bool,
    /// Manual command users can run if automatic installation is not possible.
    pub install_command: String,
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

/// Get CLI installation status for the settings UI.
#[tauri::command]
pub fn get_vshell_status() -> VshellStatus {
    let binary_path = resolve_vshell_binary();
    let binary = PathBuf::from(&binary_path);
    let binary_exists = is_usable_vshell_binary(&binary);
    let path_entry = find_vshell_in_path();
    let path_matches_binary = path_entry
        .as_ref()
        .map(|path| paths_refer_to_same_file(Path::new(path), &binary))
        .unwrap_or(false);

    VshellStatus {
        install_command: install_command_for(&binary),
        binary_path,
        binary_exists,
        path_installed: path_entry.is_some(),
        path_entry,
        path_matches_binary,
    }
}

/// Add vshell to the system PATH.
///
/// On Windows: adds the directory containing vshell.exe to the user's PATH registry.
/// On macOS/Linux: creates a symlink in /usr/local/bin.
#[tauri::command]
pub fn add_vshell_to_path() -> Result<String, String> {
    let vshell_path = resolve_vshell_binary();
    let vshell = std::path::Path::new(&vshell_path);

    if !is_usable_vshell_binary(vshell) {
        return Err(format!(
            "vshell binary not found or not executable at '{}'. Build the CLI first with: cargo build --release -p vshell",
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

    match create_or_update_symlink(vshell, link_path) {
        Ok(SymlinkUpdate::AlreadyInstalled) => {
            Ok("vshell is already available at /usr/local/bin/vshell".to_string())
        }
        Ok(SymlinkUpdate::Created) => Ok(format!(
            "Installed vshell: /usr/local/bin/vshell -> {}",
            vshell.display()
        )),
        Err(e) => {
            #[cfg(target_os = "macos")]
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                return add_to_macos_path_with_admin_prompt(vshell, link_path)
                    .map_err(|admin_error| admin_error.to_string());
            }

            if e.kind() == std::io::ErrorKind::PermissionDenied {
                Err(format!(
                    "Permission denied. Run: sudo ln -sf {} /usr/local/bin/vshell",
                    shell_quote(vshell)
                ))
            } else {
                Err(format!("Failed to create symlink: {}", e))
            }
        }
    }
}

#[cfg(windows)]
fn find_vshell_in_path() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = std::process::Command::new("where")
        .arg("vshell")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| is_usable_vshell_binary(Path::new(line)))
        .map(ToOwned::to_owned)
}

#[cfg(not(windows))]
fn find_vshell_in_path() -> Option<String> {
    let output = std::process::Command::new("sh")
        .args(["-lc", "command -v vshell"])
        .output()
        .ok()?;

    if output.status.success() {
        if let Some(path) = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .find(|line| is_usable_vshell_binary(Path::new(line)))
            .map(ToOwned::to_owned)
        {
            return Some(path);
        }
    }

    [
        "/usr/local/bin/vshell",
        "/opt/homebrew/bin/vshell",
        "/usr/bin/vshell",
    ]
    .into_iter()
    .find(|path| is_usable_vshell_binary(Path::new(path)))
    .map(ToOwned::to_owned)
}

fn paths_refer_to_same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(windows)]
fn install_command_for(binary: &Path) -> String {
    binary
        .parent()
        .map(|dir| {
            format!(
                "setx PATH \"%PATH%;{}\"",
                dir.to_string_lossy().replace('"', "\\\"")
            )
        })
        .unwrap_or_else(|| "Open VibeShell Settings and click Install to PATH".to_string())
}

#[cfg(not(windows))]
fn install_command_for(binary: &Path) -> String {
    format!("sudo ln -sf {} /usr/local/bin/vshell", shell_quote(binary))
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymlinkUpdate {
    AlreadyInstalled,
    Created,
}

#[cfg(not(windows))]
fn create_or_update_symlink(target: &Path, link: &Path) -> std::io::Result<SymlinkUpdate> {
    match std::fs::symlink_metadata(link) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                if symlink_points_to(link, target) {
                    return Ok(SymlinkUpdate::AlreadyInstalled);
                }
                std::fs::remove_file(link)?;
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("{} already exists and is not a symlink", link.display()),
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::os::unix::fs::symlink(target, link)?;
    Ok(SymlinkUpdate::Created)
}

#[cfg(not(windows))]
fn symlink_points_to(link: &Path, target: &Path) -> bool {
    let Ok(existing_target) = std::fs::read_link(link) else {
        return false;
    };

    let existing_target = if existing_target.is_absolute() {
        existing_target
    } else {
        link.parent()
            .unwrap_or_else(|| Path::new("/"))
            .join(existing_target)
    };

    paths_refer_to_same_file(&existing_target, target)
}

#[cfg(not(windows))]
fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn add_to_macos_path_with_admin_prompt(target: &Path, link: &Path) -> std::io::Result<String> {
    let parent = link.parent().unwrap_or_else(|| Path::new("/usr/local/bin"));
    let command = format!(
        "set -e\n\
         if [ -e {link} ] && [ ! -L {link} ]; then\n\
           echo 'Destination already exists and is not a symlink' >&2\n\
           exit 17\n\
         fi\n\
         mkdir -p {parent}\n\
         rm -f {link}\n\
         ln -s {target} {link}",
        parent = shell_quote(parent),
        target = shell_quote(target),
        link = shell_quote(link),
    );
    let script = format!(
        "do shell script {} with administrator privileges",
        applescript_string_literal(&command)
    );
    let output = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()?;

    if output.status.success() {
        Ok(format!(
            "Installed vshell: {} -> {}",
            link.display(),
            target.display()
        ))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            if stderr.is_empty() {
                "Administrator authorization was cancelled or failed".to_string()
            } else {
                stderr
            },
        ))
    }
}

#[cfg(target_os = "macos")]
fn applescript_string_literal(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
