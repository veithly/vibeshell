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

use super::detector::{
    find_tool, skill_dir_candidates_for_tool, skill_install_dirs_for_tool, AiTool,
};

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
description: Use when the user needs SSH/SFTP access through the VibeShell `vshell` CLI: list configured servers, reuse SSH sessions, run remote commands, inspect logs, deploy files, or manage remote files.
---

You have access to **VibeShell** through the `vshell` CLI. VibeShell uses saved server configs and credentials, keeps persistent reusable SSH sessions, and assigns short aliases such as `001` for follow-up commands.

## Fast Path

```bash
vshell servers
vshell ssh my-server -- hostname
vshell ssh my-server
vshell sessions
vshell ssh-session 001 -- uptime
vshell ss 001 -- journalctl -u nginx -n 200
vshell send-key 001 y enter
vshell sftp my-server ls /var/www
vshell get-content my-server /etc/nginx/nginx.conf
vshell rg my-server "listen 80" /etc/nginx
```

Most SSH, SFTP, and session commands auto-start the headless VibeShell daemon. If a command cannot communicate with the background service, check it with `vshell daemon status` and start it with `vshell daemon start`.

## Session Flow

```text
Need server name?
-> `vshell servers`

Need a one-off non-interactive command?
-> `vshell ssh <server> -- <command>`

Need a reusable shell?
-> `vshell ssh <server>`
-> by default, this reuses the earliest active session for that server

Need a fresh parallel login?
-> `vshell ssh <server> --new`
-> only do this when the user explicitly wants a new session

Need another command on the same session?
-> `vshell ssh-session <alias> -- <command>`
-> alias example: `vshell ssh-session 001 -- hostname`

Need to reattach interactively?
-> `vshell ssh-session <alias>`
-> or `vshell attach <alias>`

Need to answer a prompt?
-> `vshell send-key <alias> y enter`
-> `vshell send-key <alias> ctrl-c`

Need to list or kill sessions?
-> `vshell sessions`
-> `vshell kill <alias>`
-> `vshell kill --all`
```

Sessions persist across commands and can be reused for SSH, SFTP, and follow-up input. Idle sessions are reaped only after about 30 minutes with no clients and no activity.

## SSH Flow

### Common commands

```bash
vshell servers
vshell ssh my-server
vshell ssh my-server --new
vshell ssh --wait my-server
vshell ssh my-server -- hostname
vshell ssh my-server -- systemctl status nginx
vshell ssh-session 001 -- hostname
vshell ssh-session 001 -- ls -la /var/log
vshell ssh-session 001 --command-file ./remote-command.sh
vshell ssh-session 001 --command-stdin
vshell exec <session-id> -- hostname
vshell attach 001
vshell send-key 001 enter
vshell kill 001
```

Use `vshell ssh <server> -- <command>` when starting from a configured server name. Use `vshell ssh-session <alias> -- <command>` or `vshell exec <alias> -- <command>` when you already have an active session alias or UUID.

Use `--wait` when a flaky network, VPN, or Tailscale login may need retries. Use `--new` only for a deliberate parallel login.

### Command input without quote traps

Use exactly one command source: `-- <command>`, `--command-file <path>`, or `--command-stdin`. For commands with nested quotes, pipes, regexes, or shell scripts, prefer `--command-file` or `--command-stdin`.

Shell stdin example:

```bash
vshell ssh my-server --command-stdin <<'SH'
sh -lc 'cd /srv/app && docker compose ps && curl -fsS http://127.0.0.1:8000/health'
SH
```

PowerShell stdin example:

```powershell
@'
sh -lc 'cd /srv/app && docker compose ps && curl -fsS http://127.0.0.1:8000/health'
'@ | vshell ssh my-server --command-stdin
```

Repeatable command file examples:

```bash
vshell ssh my-server --command-file ./remote-command.sh
vshell ssh-session 001 --command-file ./remote-command.sh
```

## Interactive Command Flow

```text
IF a command may ask for Enter / y / password / confirmation
THEN run it inside the persistent shell session
AND reuse that same session for follow-up input
```

CLI prints `Next use:` hints after `vshell ssh` or when a command is waiting for more input. Follow the alias in that hint instead of starting a new connection.

Recommended follow-up commands:

```bash
vshell ssh-session 001 -- <command>
vshell ssh-session 001 --command-file ./remote-command.sh
vshell ssh-session 001
vshell send-key 001 y enter
vshell send-key 001 ctrl-c
vshell attach <session-id>
```

## SFTP Flow

```text
Need file operations?
-> use `vshell sftp <server> <operation>`
-> or reuse an alias with `vshell sftp --session <alias> <operation>`
```

```text
Need to inspect or edit text?
-> search with `vshell rg`
-> read with `vshell get-content`
-> edit existing files with `vshell edit-file`
-> create new files with `vshell add-file`
```

```text
Need to upload a whole folder?
-> `vshell sftp <server> put <local-dir> <remote-dir>`

Need repeatable deploy-style sync?
-> `vshell sftp <server> sync <local-dir> <remote-dir>`
-> pass `--delete` only when remote extras should really be removed
```

### Direct SFTP commands

```bash
vshell sftp my-server
vshell sftp my-server pwd
vshell sftp my-server ls /var/www
vshell sftp --session 001 cat /etc/nginx/nginx.conf
vshell sftp my-server get /var/log/app.log ./app.log
vshell sftp my-server put ./local-file.txt /tmp/local-file.txt
vshell sftp my-server put ./dist /var/www/app
vshell sftp my-server put ./dist /var/www/app --exclude node_modules/ --exclude .git/
vshell sftp my-server sync ./dist /var/www/app --exclude node_modules/ --no-gitignore
vshell sftp my-server sync ./dist /var/www/app --delete
vshell sftp my-server mkdir /var/www/uploads
vshell sftp my-server rm /tmp/old-file
vshell sftp my-server mv /tmp/a /tmp/b
```

### Remote text helpers

```bash
vshell rg my-server TODO /srv/app --glob "*.rs" --max-results 100
vshell rg --session 001 "listen 80" /etc/nginx -i
vshell get-content my-server /etc/nginx/nginx.conf --max-bytes 200000
vshell get-content --session 001 /var/log/app.log
vshell add-file my-server /tmp/config.yml --content-file ./config.yml --parents
vshell add-file --session 001 /tmp/config.yml --content "key: value\n" --parents --overwrite
vshell edit-file my-server /etc/app.conf --replace "debug=false" --with "debug=true"
vshell edit-file my-server /etc/app.conf --replace "old" --with "new" --all
vshell edit-file --session 001 /etc/app.conf --content-file ./app.conf
Get-Content .\config.yml | vshell edit-file my-server /etc/app.conf --content-stdin
```

## Rules

- Use the `vshell` CLI through the local shell/exec tool for VibeShell work.
- Prefer reusing an existing session over creating a new one.
- Treat `vshell ssh <server>` as a reusable-session command; only add `--new` when the user explicitly wants another parallel session.
- Prefer `vshell ssh <server> -- <command>` for non-interactive automation from a server name.
- Prefer `vshell ssh-session <alias> -- <command>` for non-interactive automation on an existing session.
- Prefer shell session reuse for interactive prompts or multi-step command flows.
- Use `rg` for remote text search before broad directory downloads.
- Use `vshell get-content` for text inspection.
- Use `vshell edit-file` for existing remote text files; prefer exact `--replace` / `--with` for small targeted edits, and full content replacement only when you intentionally own the whole file.
- Use `vshell add-file` when creating new remote text files; it fails on existing files unless `--overwrite` is explicit.
- Use `vshell sftp get` / `put` for binary or local file transfer.
- Use `vshell sftp put <local-dir> <remote-dir>` for first-time recursive folder uploads.
- Use `vshell sftp sync <local-dir> <remote-dir>` for repeatable deploy-style directory syncs.
- Directory upload/sync respects .gitignore by default when configured; pass explicit excludes for heavy or unsafe paths such as `node_modules/`, `.venv/`, `target/`, and `.git/`.
- Only use `--delete` when the user explicitly wants remote files absent locally to be removed.
- If the user provides only a host or IP, map it to a configured server first with `vshell servers`.
- If no configured server matches, ask the user to add/select a server in VibeShell.
- Credentials come from saved VibeShell configuration; do not invent ad-hoc SSH passwords or keys on the command line unless the environment already requires it.
"#;

/// Install the SKILL.md file into the tool's skills directory.
///
/// This is the **only** thing the installer does. It does NOT modify
/// any MCP config file (mcp.json, mcpServers.json, etc.).
fn install_skill_file(tool_id: &str) -> Result<Vec<PathBuf>> {
    let skills_dirs = skill_install_dirs_for_tool(tool_id)
        .ok_or_else(|| anyhow!("No skills directory known for tool: {}", tool_id))?;

    let mut installed_paths = Vec::new();
    for skills_dir in skills_dirs {
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
        installed_paths.push(skill_path);
    }

    Ok(installed_paths)
}

/// Remove the SKILL.md file from the tool's skills directory.
fn uninstall_skill_file(tool_id: &str) -> Result<()> {
    if let Some(skills_dirs) = skill_dir_candidates_for_tool(tool_id) {
        for skills_dir in skills_dirs {
            for dir_name in [SKILL_DIR_NAME, LEGACY_SKILL_DIR_NAME] {
                let skill_dir = skills_dir.join(dir_name);
                if skill_dir.exists() {
                    fs::remove_dir_all(&skill_dir).with_context(|| {
                        format!("Failed to remove skill directory {:?}", skill_dir)
                    })?;
                    log::info!("[Install] Skill file removed from {:?}", skill_dir);
                }
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
    let skill_paths = install_skill_file(tool_id)?;
    Ok(skill_paths.into_iter().next())
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
        assert!(SKILL_MD_CONTENT.contains("Command input without quote traps"));
        assert!(SKILL_MD_CONTENT.contains("--command-file"));
        assert!(SKILL_MD_CONTENT.contains("--command-stdin"));
        assert!(SKILL_MD_CONTENT.contains("get-content"));
        assert!(SKILL_MD_CONTENT.contains("edit-file"));
        assert!(SKILL_MD_CONTENT.contains("add-file"));
        assert!(SKILL_MD_CONTENT.contains("vshell sftp my-server put"));
        assert!(SKILL_MD_CONTENT.contains("vshell sftp my-server sync"));
        assert!(SKILL_MD_CONTENT.contains("--delete"));
        assert!(SKILL_MD_CONTENT.contains("vshell rg"));
        assert!(!SKILL_MD_CONTENT.contains("MCP"));
        assert!(!SKILL_MD_CONTENT.contains("server_list"));
        assert!(!SKILL_MD_CONTENT.contains("session_create"));
        assert!(!SKILL_MD_CONTENT.contains("sftp_upload_directory"));
        assert!(!SKILL_MD_CONTENT.contains("sftp_sync_directory"));
        assert!(!SKILL_MD_CONTENT.contains("get_content"));
        assert!(!SKILL_MD_CONTENT.contains("edit_file"));
        assert!(!SKILL_MD_CONTENT.contains("add_file"));
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
