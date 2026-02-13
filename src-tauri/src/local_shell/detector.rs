//! Shell detection module for discovering available shells on the system.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Information about an available shell
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellInfo {
    /// Unique identifier for the shell (e.g., "powershell", "cmd", "bash")
    pub id: String,
    /// Display name for the shell
    pub name: String,
    /// Full path to the shell executable
    pub path: String,
    /// Shell type category
    pub shell_type: ShellType,
    /// Whether this is the system default shell
    pub is_default: bool,
}

/// Shell type categories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ShellType {
    PowerShell,
    Cmd,
    Bash,
    Zsh,
    Fish,
    Sh,
    Other,
}

/// Detect all available shells on the system
pub fn detect_available_shells() -> Vec<ShellInfo> {
    let mut shells = Vec::new();

    #[cfg(target_os = "windows")]
    {
        shells.extend(detect_windows_shells());
    }

    #[cfg(not(target_os = "windows"))]
    {
        shells.extend(detect_unix_shells());
    }

    // Mark the default shell
    if let Some(default_id) = get_default_shell_id() {
        for shell in &mut shells {
            if shell.id == default_id {
                shell.is_default = true;
            }
        }
    } else if !shells.is_empty() {
        // If no default identified, mark the first one as default
        shells[0].is_default = true;
    }

    shells
}

/// Get the system's default shell
pub fn get_default_shell() -> Option<ShellInfo> {
    let shells = detect_available_shells();
    shells.into_iter().find(|s| s.is_default)
}

#[cfg(target_os = "windows")]
fn detect_windows_shells() -> Vec<ShellInfo> {
    

    let mut shells = Vec::new();

    // PowerShell 7+ (pwsh)
    if let Ok(path) = which::which("pwsh") {
        shells.push(ShellInfo {
            id: "pwsh".to_string(),
            name: "PowerShell 7+".to_string(),
            path: path.to_string_lossy().to_string(),
            shell_type: ShellType::PowerShell,
            is_default: false,
        });
    }

    // Windows PowerShell (powershell.exe)
    if let Ok(path) = which::which("powershell") {
        shells.push(ShellInfo {
            id: "powershell".to_string(),
            name: "Windows PowerShell".to_string(),
            path: path.to_string_lossy().to_string(),
            shell_type: ShellType::PowerShell,
            is_default: false,
        });
    }

    // Command Prompt (cmd.exe)
    if let Ok(path) = which::which("cmd") {
        shells.push(ShellInfo {
            id: "cmd".to_string(),
            name: "Command Prompt".to_string(),
            path: path.to_string_lossy().to_string(),
            shell_type: ShellType::Cmd,
            is_default: false,
        });
    }

    // Git Bash - check common installation paths
    let git_bash_paths = [
        "C:\\Program Files\\Git\\bin\\bash.exe",
        "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
    ];

    for path_str in &git_bash_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            shells.push(ShellInfo {
                id: "git-bash".to_string(),
                name: "Git Bash".to_string(),
                path: path_str.to_string(),
                shell_type: ShellType::Bash,
                is_default: false,
            });
            break;
        }
    }

    // WSL Bash (if WSL is installed)
    if let Ok(path) = which::which("wsl") {
        shells.push(ShellInfo {
            id: "wsl".to_string(),
            name: "WSL (Windows Subsystem for Linux)".to_string(),
            path: path.to_string_lossy().to_string(),
            shell_type: ShellType::Bash,
            is_default: false,
        });
    }

    // MSYS2 Bash
    let msys2_paths = [
        "C:\\msys64\\usr\\bin\\bash.exe",
        "C:\\msys32\\usr\\bin\\bash.exe",
    ];

    for path_str in &msys2_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            shells.push(ShellInfo {
                id: "msys2-bash".to_string(),
                name: "MSYS2 Bash".to_string(),
                path: path_str.to_string(),
                shell_type: ShellType::Bash,
                is_default: false,
            });
            break;
        }
    }

    // Cygwin Bash
    let cygwin_path = PathBuf::from("C:\\cygwin64\\bin\\bash.exe");
    if cygwin_path.exists() {
        shells.push(ShellInfo {
            id: "cygwin-bash".to_string(),
            name: "Cygwin Bash".to_string(),
            path: cygwin_path.to_string_lossy().to_string(),
            shell_type: ShellType::Bash,
            is_default: false,
        });
    }

    shells
}

#[cfg(not(target_os = "windows"))]
fn detect_unix_shells() -> Vec<ShellInfo> {
    let mut shells = Vec::new();

    // Bash
    if let Ok(path) = which::which("bash") {
        shells.push(ShellInfo {
            id: "bash".to_string(),
            name: "Bash".to_string(),
            path: path.to_string_lossy().to_string(),
            shell_type: ShellType::Bash,
            is_default: false,
        });
    }

    // Zsh
    if let Ok(path) = which::which("zsh") {
        shells.push(ShellInfo {
            id: "zsh".to_string(),
            name: "Zsh".to_string(),
            path: path.to_string_lossy().to_string(),
            shell_type: ShellType::Zsh,
            is_default: false,
        });
    }

    // Fish
    if let Ok(path) = which::which("fish") {
        shells.push(ShellInfo {
            id: "fish".to_string(),
            name: "Fish".to_string(),
            path: path.to_string_lossy().to_string(),
            shell_type: ShellType::Fish,
            is_default: false,
        });
    }

    // Sh (POSIX shell)
    if let Ok(path) = which::which("sh") {
        shells.push(ShellInfo {
            id: "sh".to_string(),
            name: "POSIX Shell".to_string(),
            path: path.to_string_lossy().to_string(),
            shell_type: ShellType::Sh,
            is_default: false,
        });
    }

    shells
}

#[cfg(target_os = "windows")]
fn get_default_shell_id() -> Option<String> {
    // On Windows, prefer PowerShell 7+ if available, then Windows PowerShell
    if which::which("pwsh").is_ok() {
        Some("pwsh".to_string())
    } else if which::which("powershell").is_ok() {
        Some("powershell".to_string())
    } else {
        Some("cmd".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn get_default_shell_id() -> Option<String> {
    use std::env;

    // Check SHELL environment variable
    if let Ok(shell) = env::var("SHELL") {
        let shell_name = PathBuf::from(&shell)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        match shell_name.as_str() {
            "bash" => return Some("bash".to_string()),
            "zsh" => return Some("zsh".to_string()),
            "fish" => return Some("fish".to_string()),
            "sh" => return Some("sh".to_string()),
            _ => {}
        }
    }

    // Default to bash if available
    if which::which("bash").is_ok() {
        Some("bash".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_shells() {
        let shells = detect_available_shells();
        // Should find at least one shell
        assert!(!shells.is_empty(), "Should detect at least one shell");

        // Should have at least one default
        assert!(shells.iter().any(|s| s.is_default), "Should have a default shell");
    }

    #[test]
    fn test_get_default_shell() {
        let default = get_default_shell();
        assert!(default.is_some(), "Should have a default shell");
    }
}
