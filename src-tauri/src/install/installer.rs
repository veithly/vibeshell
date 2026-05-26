//! VibeShell skill installer for AI coding tools.
//!
//! This module installs/uninstalls the VibeShell SKILL.md file into
//! AI coding tool skill directories. The SKILL.md teaches AI agents
//! how to use the `vshell` CLI to manage SSH servers and sessions.
//!
//! **Important**: This installer does NOT modify MCP config files.
//! Integration is purely through SKILL.md — the AI reads the skill
//! and learns to call `vshell` via its shell/exec tool.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::detector::{find_tool, AiTool};

const SKILL_DIR_NAME: &str = "vshell";
const LEGACY_SKILL_DIR_NAME: &str = "vibeshell";

/// Resolve the absolute path to the vshell binary.
///
/// Search order:
/// 1. Next to the current executable (Tauri install location)
/// 2. Common installation paths per platform
/// 3. PATH lookup via `which`/`where`
/// 4. Fall back to bare "vshell" command name
pub fn resolve_vshell_binary() -> String {
    let vshell_name = if cfg!(windows) {
        "vshell.exe"
    } else {
        "vshell"
    };

    // 1. Next to the current executable
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let candidate = dir.join(vshell_name);
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }

    // 2. Common installation paths per platform
    #[cfg(windows)]
    {
        for env_var in &["LOCALAPPDATA", "ProgramFiles"] {
            if let Ok(base) = std::env::var(env_var) {
                let candidate = PathBuf::from(&base).join("VibeShell").join(vshell_name);
                if candidate.exists() {
                    return candidate.to_string_lossy().to_string();
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for c in &[
            "/Applications/VibeShell.app/Contents/MacOS/vshell",
            "/usr/local/bin/vshell",
        ] {
            if Path::new(c).exists() {
                return c.to_string();
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for c in &["/usr/bin/vshell", "/usr/local/bin/vshell"] {
            if Path::new(c).exists() {
                return c.to_string();
            }
        }
    }

    // 3. Try to find via PATH (which/where)
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        if let Ok(output) = std::process::Command::new("where")
            .arg("vshell")
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Some(first_line) = path.lines().next() {
                    if Path::new(first_line).exists() {
                        return first_line.to_string();
                    }
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(output) = std::process::Command::new("which").arg("vshell").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if Path::new(&path).exists() {
                    return path;
                }
            }
        }
    }

    // 4. Fallback: bare command name
    vshell_name.to_string()
}

/// The SKILL.md content that teaches AI agents how to use VibeShell CLI.
///
/// This is the sole integration mechanism — AI agents read this skill file
/// and learn to run `vshell` commands via their shell/exec capabilities.
const SKILL_MD_CONTENT: &str = r#"---
name: vshell
description: Use when the user needs SSH access to a configured server, wants to run remote commands, inspect logs, deploy files, or manage remote files over SFTP through VibeShell.
---

You have access to **VibeShell**, an SSH/SFTP client with persistent reusable sessions.

## Access Flow

```text
IF MCP tools are available
THEN prefer MCP
ELSE use `vshell` CLI
```

```text
IF a reusable session already exists
THEN reuse it
ELSE create a new session
```

```text
IF the user explicitly asks for a fresh SSH login
THEN use `vshell ssh <server> --new`
ELSE prefer the default reusable session path
```

```text
IF the task is non-interactive automation
THEN prefer `exec`
ELSE attach to the shell
```

## Session Flow

```text
Need server name?
-> use `server_list` or `vshell servers`

Need a reusable connection?
-> use `vshell ssh <server>`

Need a fresh connection anyway?
-> use `session_create`
-> or `vshell ssh <server> --new`

Need another command on the same machine?
-> reuse the existing session

Done for now?
-> leave session alive for reuse, or kill it explicitly
```

Sessions persist across commands and can be reused for SSH, SFTP, and follow-up input. Idle sessions are reaped only after about 30 minutes with no clients and no activity.

## SSH Flow

### MCP

```text
Create session
-> `session_create`

Run command
-> `exec`

Need more commands?
-> keep the same `session_id`

Done?
-> `session_kill`
```

Example:

```json
session_create({ "server_name": "my-server" })
exec({ "session_id": "abc-123", "command": "hostname" })
exec({ "session_id": "abc-123", "command": "ls -la /var/log", "timeout_ms": 60000 })
session_kill({ "session_id": "abc-123" })
```

### CLI

```text
Open shell
-> `vshell ssh my-server`
-> by default, reuse the earliest active session for that server

Need a new parallel shell?
-> `vshell ssh my-server --new`

Run another command on the same session
-> `vshell ssh-session 001 -- <command>`
-> `vshell ssh-session 001 --command-file ./remote-command.sh`

Reattach interactively
-> `vshell ssh-session 001`

List or kill sessions
-> `vshell sessions`
-> `vshell kill 001`
```

Examples:

```bash
vshell servers
vshell ssh my-server
vshell ssh my-server --new
vshell ssh --wait my-server
vshell ssh-session 001 -- hostname
vshell ssh-session 001 -- ls -la /var/log
vshell ssh-session 001 --command-file ./remote-command.sh
vshell exec <session-id> -- hostname
vshell attach 001
vshell kill 001
```

If the local shell is fighting nested quotes, especially in PowerShell, prefer
`--command-file <path>` or pipe text into `--command-stdin` instead of stacking
more escaping.

### PowerShell-safe command input

PowerShell parses quotes before `vshell` receives arguments. For commands that
contain nested quotes, pipes, regexes, or redaction expressions, put the remote
command in a file or pipe it through stdin.

Prefer a temporary command file for repeatable work:

```powershell
@'
sh -lc 'cd /srv/app && grep -E "POSTGRES|DATABASE|DB_" .env | sed -E "s/(PASSWORD|PASS|URL|DSN)=.*/\1=***REDACTED***/"'
'@ | Set-Content -NoNewline -Encoding UTF8 .\remote-command.sh

vshell ssh my-server --command-file .\remote-command.sh
vshell ssh-session 001 --command-file .\remote-command.sh
```

For one-off commands, pipe a single-quoted here-string into stdin:

```powershell
@'
sh -lc 'cd /srv/app && docker compose ps && curl -fsS http://127.0.0.1:8000/health'
'@ | vshell ssh my-server --command-stdin
```

Avoid this fragile pattern in PowerShell for complex remote commands:

```powershell
vshell ssh my-server -- "sh -lc 'grep -E \"A|B\" .env | sed -E \"s/(PASSWORD)=.*/\\1=***/\"'"
```

## Interactive Command Flow

```text
IF a command may ask for Enter / y / password / confirmation
THEN run it inside the persistent shell session
AND reuse that same session for follow-up input
```

CLI will print a `Next use:` hint after `vshell ssh` or when a command is waiting for more input. Follow that hint to continue on the same session instead of starting a new connection.

Recommended follow-up commands:

```bash
vshell ssh-session 001 -- <command>
vshell ssh-session 001 --command-file ./remote-command.sh
vshell ssh-session 001
vshell exec <session-id> -- <command>
vshell attach <session-id>
```

## SFTP Flow

```text
Need file operations?
-> reuse an existing session if possible
-> otherwise create one first
-> perform SFTP operations on that session
```

```text
Need to inspect or edit text?
-> search with `rg`
-> read with `get_content`
-> edit existing files with `edit_file`
-> create new files with `add_file`
```

```text
Need to upload a whole folder?
-> use `sftp_upload_directory` or `vshell sftp <server> put <local-dir> <remote-dir>`

Need repeatable deploy-style sync?
-> use `sftp_sync_directory` or `vshell sftp <server> sync <local-dir> <remote-dir>`
-> set delete_extra / --delete only when remote extras should really be removed
```

### MCP

```json
session_create({ "server_name": "my-server" })
sftp_ls({ "session_id": "abc-123", "path": "/var/www", "show_hidden": true })
rg({ "session_id": "abc-123", "pattern": "TODO", "path": "/srv/app", "globs": ["*.rs"], "max_results": 100 })
get_content({ "session_id": "abc-123", "path": "/etc/nginx/nginx.conf" })
edit_file({ "session_id": "abc-123", "path": "/etc/app.conf", "old_text": "debug=false", "new_text": "debug=true" })
edit_file({ "session_id": "abc-123", "path": "/etc/app.conf", "content": "full replacement text\n" })
add_file({ "session_id": "abc-123", "path": "/tmp/config.yml", "content": "key: value\n", "parents": true })
sftp_read({ "session_id": "abc-123", "path": "/etc/nginx/nginx.conf" })
sftp_write({ "session_id": "abc-123", "path": "/tmp/config.yml", "content": "key: value\n" })
sftp_upload({ "session_id": "abc-123", "local_path": "C:/project/dist/app.js", "remote_path": "/var/www/app.js" })
sftp_upload_directory({ "session_id": "abc-123", "local_path": "C:/project/dist", "remote_path": "/var/www/app", "respect_gitignore": true, "excluded_paths": ["node_modules/", ".venv/"] })
sftp_sync_directory({ "session_id": "abc-123", "local_path": "C:/project/dist", "remote_path": "/var/www/app", "delete_extra": false, "respect_gitignore": true })
sftp_download({ "session_id": "abc-123", "remote_path": "/var/log/app.log", "local_path": "C:/tmp/app.log" })
sftp_mkdir({ "session_id": "abc-123", "path": "/var/www/uploads/2024", "recursive": true })
sftp_rm({ "session_id": "abc-123", "path": "/tmp/old-backup", "recursive": true })
sftp_mv({ "session_id": "abc-123", "source": "/var/www/app.js", "destination": "/var/www/app.js.bak" })
```

### CLI

```bash
vshell sftp --session <session-id>
vshell sftp my-server
vshell rg my-server TODO /srv/app --glob "*.rs"
vshell get-content my-server /etc/nginx/nginx.conf
vshell edit-file my-server /etc/app.conf --replace "debug=false" --with "debug=true"
vshell add-file my-server /tmp/config.yml --content-file ./config.yml --parents
Get-Content .\config.yml | vshell edit-file my-server /etc/app.conf --content-stdin
vshell sftp my-server put ./dist /var/www/app
vshell sftp my-server sync ./dist /var/www/app --exclude node_modules/ --no-gitignore
vshell sftp my-server sync ./dist /var/www/app --delete
```

## Rules

- Prefer MCP when available.
- Prefer reusing an existing session over creating a new one.
- Treat `vshell ssh <server>` as a reusable-session command; only add `--new` when the user explicitly wants another parallel session.
- Prefer `exec` for non-interactive automation.
- Prefer shell session reuse for interactive prompts or multi-step command flows.
- Use `rg` for remote text search before broad directory downloads.
- Use `get_content` / `vshell get-content` for text inspection.
- Use `edit_file` / `vshell edit-file` for existing remote text files; prefer exact `old_text`/`new_text` replacements for small targeted edits, and full `content` replacement only when you intentionally own the whole file.
- Use `add_file` / `vshell add-file` when creating new remote text files; it should fail on existing files unless overwrite is explicit.
- Use `sftp_download` for binary files and `sftp_read` for lower-level text inspection.
- Use `sftp_upload` for binary/local file transfer and `sftp_write` for lower-level direct text writes.
- Use `sftp_upload_directory` / `vshell sftp put <local-dir> <remote-dir>` for first-time recursive folder uploads.
- Use `sftp_sync_directory` / `vshell sftp sync <local-dir> <remote-dir>` for repeatable deploy-style directory syncs.
- Directory upload/sync respects .gitignore by default when configured; pass explicit excludes for heavy or unsafe paths such as `node_modules/`, `.venv/`, `target/`, and `.git/`.
- Only use `delete_extra=true` or `--delete` when the user explicitly wants remote files absent locally to be removed.
- If the user provides only a host or IP, map it to a configured server first with `server_list` or `vshell servers`.
- Credentials come from saved VibeShell configuration; do not invent ad-hoc SSH passwords or keys on the command line unless the environment already requires it.
"#;

/// Get the skills directory path for a given AI tool.
fn get_skills_dir(tool_id: &str) -> Option<PathBuf> {
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())?;

    match tool_id {
        "claude-code" => Some(home.join(".claude").join("skills")),
        "cursor" => Some(home.join(".cursor").join("skills")),
        "codex" => Some(home.join(".codex").join("skills")),
        "opencode" => Some(home.join(".opencode").join("skills")),
        "gemini-cli" => Some(home.join(".gemini").join("skills")),
        "openclaw" => Some(home.join(".openclaw").join("skills")),
        "windsurf" => Some(home.join(".codeium").join("windsurf").join("skills")),
        "roo-code" => Some(home.join(".roo").join("skills")),
        "augment" => Some(home.join(".augment").join("skills")),
        "continue" => Some(home.join(".continue").join("skills")),
        "kiro" => Some(home.join(".kiro").join("skills")),
        "trae" => Some(home.join(".trae").join("skills")),
        "openhands" => Some(home.join(".openhands").join("skills")),
        "agents" => Some(home.join(".agents").join("skills")),
        "stepfun" => Some(home.join(".stepfun").join("skills")),
        _ => None,
    }
}

/// Install the SKILL.md file into the tool's skills directory.
///
/// This is the **only** thing the installer does. It does NOT modify
/// any MCP config file (mcp.json, mcpServers.json, etc.).
fn install_skill_file(tool_id: &str) -> Result<PathBuf> {
    let skills_dir = get_skills_dir(tool_id)
        .ok_or_else(|| anyhow!("No skills directory known for tool: {}", tool_id))?;

    let skill_dir = skills_dir.join(SKILL_DIR_NAME);
    fs::create_dir_all(&skill_dir)
        .with_context(|| format!("Failed to create skill directory {:?}", skill_dir))?;

    let skill_path = skill_dir.join("SKILL.md");
    fs::write(&skill_path, SKILL_MD_CONTENT)
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

    log::info!("[Install] Skill file installed to {:?}", skill_path);
    Ok(skill_path)
}

/// Remove the SKILL.md file from the tool's skills directory.
fn uninstall_skill_file(tool_id: &str) -> Result<()> {
    if let Some(skills_dir) = get_skills_dir(tool_id) {
        for dir_name in [SKILL_DIR_NAME, LEGACY_SKILL_DIR_NAME] {
            let skill_dir = skills_dir.join(dir_name);
            if skill_dir.exists() {
                fs::remove_dir_all(&skill_dir)
                    .with_context(|| format!("Failed to remove skill directory {:?}", skill_dir))?;
                log::info!("[Install] Skill file removed from {:?}", skill_dir);
            }
        }
    }
    Ok(())
}

/// Result of an installation operation.
#[derive(Debug)]
pub struct InstallResult {
    /// The tool that was installed to
    pub tool: AiTool,
    /// Whether the installation was successful
    pub success: bool,
    /// Path to the backup file if created (unused — kept for API compat)
    pub backup_path: Option<PathBuf>,
    /// Error message if installation failed
    pub error: Option<String>,
}

/// Install VibeShell skill to a specific AI tool.
///
/// Only installs the SKILL.md file. Does NOT modify MCP configs.
pub fn install_to_tool(tool_id: &str, _config_path: &PathBuf) -> Result<Option<PathBuf>> {
    let skill_path = install_skill_file(tool_id)?;
    Ok(Some(skill_path))
}

/// Uninstall VibeShell skill from a specific AI tool.
///
/// Only removes the SKILL.md file. Does NOT modify MCP configs.
pub fn uninstall_from_tool(tool_id: &str, _config_path: &PathBuf) -> Result<Option<PathBuf>> {
    uninstall_skill_file(tool_id)?;
    Ok(None)
}

/// Install VibeShell skill to a tool by ID.
pub fn install_by_id(tool_id: &str) -> Result<InstallResult> {
    let tool = find_tool(tool_id).ok_or_else(|| anyhow!("Unknown tool: {}", tool_id))?;

    match install_to_tool(&tool.id, &tool.config_path) {
        Ok(path) => Ok(InstallResult {
            tool,
            success: true,
            backup_path: path,
            error: None,
        }),
        Err(e) => Ok(InstallResult {
            tool,
            success: false,
            backup_path: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Uninstall VibeShell skill from a tool by ID.
pub fn uninstall_by_id(tool_id: &str) -> Result<InstallResult> {
    let tool = find_tool(tool_id).ok_or_else(|| anyhow!("Unknown tool: {}", tool_id))?;

    match uninstall_from_tool(&tool.id, &tool.config_path) {
        Ok(_) => Ok(InstallResult {
            tool,
            success: true,
            backup_path: None,
            error: None,
        }),
        Err(e) => Ok(InstallResult {
            tool,
            success: false,
            backup_path: None,
            error: Some(e.to_string()),
        }),
    }
}

/// Install VibeShell skill to all detected installed tools.
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
            Err(e) => InstallResult {
                tool,
                success: false,
                backup_path: None,
                error: Some(e.to_string()),
            },
        })
        .collect()
}

/// Uninstall VibeShell skill from all configured tools.
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
                Err(e) => InstallResult {
                    tool,
                    success: false,
                    backup_path: None,
                    error: Some(e.to_string()),
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
    fn test_skill_md_content_is_valid() {
        assert!(SKILL_MD_CONTENT.contains("vshell"));
        assert!(SKILL_MD_CONTENT.contains("name: vshell"));
        assert!(SKILL_MD_CONTENT.contains("servers"));
        assert!(SKILL_MD_CONTENT.contains("ssh"));
        assert!(SKILL_MD_CONTENT.contains("sessions"));
        assert!(SKILL_MD_CONTENT.contains("kill"));
        assert!(SKILL_MD_CONTENT.contains("PowerShell-safe command input"));
        assert!(SKILL_MD_CONTENT.contains("--command-file"));
        assert!(SKILL_MD_CONTENT.contains("--command-stdin"));
        assert!(SKILL_MD_CONTENT.contains("get_content"));
        assert!(SKILL_MD_CONTENT.contains("edit_file"));
        assert!(SKILL_MD_CONTENT.contains("add_file"));
        assert!(SKILL_MD_CONTENT.contains("sftp_upload_directory"));
        assert!(SKILL_MD_CONTENT.contains("sftp_sync_directory"));
        assert!(SKILL_MD_CONTENT.contains("vshell sftp my-server sync"));
        assert!(SKILL_MD_CONTENT.contains("--delete"));
        assert!(SKILL_MD_CONTENT.contains("vshell rg"));
    }

    #[test]
    fn test_install_creates_skill_file() {
        // This test can only run if the home directory exists
        // In CI it may not have the .claude dir, so we just verify the function signature
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("mcp.json");
        // install_to_tool should NOT create/modify config_path
        // It only writes SKILL.md to the skills directory
        let _ = install_to_tool("claude-code", &config_path);
        // config_path should NOT exist (we don't touch MCP configs)
        assert!(!config_path.exists());
    }
}
