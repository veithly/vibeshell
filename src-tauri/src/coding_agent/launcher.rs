use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
#[cfg(not(target_os = "windows"))]
use std::process::Command;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
#[cfg(target_os = "windows")]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};

use crate::local_shell::{LocalShellManager, LocalShellSession};

const MAX_INITIAL_PROMPT_BYTES: usize = 32 * 1024;
#[cfg(target_os = "windows")]
const MAX_WINDOWS_PROMPT_UTF16: usize = 8 * 1024;
#[cfg(target_os = "windows")]
const WINDOWS_SHIM_PAYLOAD_ENV: &str = "VIBESHELL_AGENT_LAUNCH";
#[cfg(target_os = "windows")]
const WINDOWS_SHIM_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$payloadJson = $env:VIBESHELL_AGENT_LAUNCH
Remove-Item Env:VIBESHELL_AGENT_LAUNCH -ErrorAction SilentlyContinue
$payload = $payloadJson | ConvertFrom-Json
$agentArgs = @($payload.arguments | ForEach-Object { [string]$_ })
& ([string]$payload.executable) @agentArgs
if ($null -eq $LASTEXITCODE) { exit 0 }
exit $LASTEXITCODE
"#;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStartMode {
    New,
    ContinueLast,
    ResumePicker,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAccessMode {
    Default,
    ReadOnly,
    AutoEdit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolInfo {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub executable_path: Option<String>,
    pub start_modes: Vec<AgentStartMode>,
    pub access_modes: Vec<AgentAccessMode>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLaunchRequest {
    pub agent_id: String,
    pub cwd: String,
    #[serde(default)]
    pub prompt: Option<String>,
    pub start_mode: AgentStartMode,
    pub access_mode: AgentAccessMode,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

#[derive(Debug, Clone, Copy)]
struct AgentDefinition {
    id: &'static str,
    name: &'static str,
    executables: &'static [&'static str],
    start_modes: &'static [AgentStartMode],
    access_modes: &'static [AgentAccessMode],
}

const NEW_CONTINUE_RESUME: &[AgentStartMode] = &[
    AgentStartMode::New,
    AgentStartMode::ContinueLast,
    AgentStartMode::ResumePicker,
];
const NEW_CONTINUE: &[AgentStartMode] = &[AgentStartMode::New, AgentStartMode::ContinueLast];
const DEFAULT_READ_ONLY_AUTO_EDIT: &[AgentAccessMode] = &[
    AgentAccessMode::Default,
    AgentAccessMode::ReadOnly,
    AgentAccessMode::AutoEdit,
];
const DEFAULT_READ_ONLY: &[AgentAccessMode] =
    &[AgentAccessMode::Default, AgentAccessMode::ReadOnly];

const AGENTS: &[AgentDefinition] = &[
    AgentDefinition {
        id: "claude",
        name: "Claude Code",
        executables: &["claude"],
        start_modes: NEW_CONTINUE_RESUME,
        access_modes: DEFAULT_READ_ONLY_AUTO_EDIT,
    },
    AgentDefinition {
        id: "codex",
        name: "Codex",
        executables: &["codex"],
        start_modes: NEW_CONTINUE_RESUME,
        access_modes: DEFAULT_READ_ONLY_AUTO_EDIT,
    },
    AgentDefinition {
        id: "opencode",
        name: "OpenCode",
        executables: &["opencode"],
        start_modes: NEW_CONTINUE,
        access_modes: DEFAULT_READ_ONLY_AUTO_EDIT,
    },
    AgentDefinition {
        id: "pi",
        name: "Pi",
        executables: &["pi", "pi-agent"],
        start_modes: NEW_CONTINUE_RESUME,
        access_modes: DEFAULT_READ_ONLY,
    },
];

struct AgentLaunchPlan {
    agent_id: String,
    display_name: String,
    executable: PathBuf,
    args: Vec<OsString>,
    cwd: PathBuf,
    path: OsString,
}

#[cfg(target_os = "windows")]
#[derive(Serialize)]
struct WindowsShimPayload {
    executable: String,
    arguments: Vec<String>,
}

pub fn list_agents() -> Vec<AgentToolInfo> {
    let search_path = effective_path();
    AGENTS
        .iter()
        .map(|definition| {
            let executable = resolve_executable(definition, &search_path);
            AgentToolInfo {
                id: definition.id.to_string(),
                name: definition.name.to_string(),
                installed: executable.is_some(),
                executable_path: executable
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                start_modes: definition.start_modes.to_vec(),
                access_modes: definition.access_modes.to_vec(),
            }
        })
        .collect()
}

pub async fn launch_agent(
    manager: &LocalShellManager,
    request: AgentLaunchRequest,
) -> Result<Arc<LocalShellSession>> {
    let cols = request.cols.unwrap_or(100).clamp(20, 500);
    let rows = request.rows.unwrap_or(30).clamp(5, 300);
    let plan = build_launch_plan(&request)?;

    let mut command = build_process_command(&plan)?;
    command.cwd(&plan.cwd);
    command.env("PATH", &plan.path);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "VibeShell");
    command.env("VIBESHELL_AGENT_ID", &plan.agent_id);

    manager
        .create_process_session(
            format!("agent:{}", plan.agent_id),
            plan.display_name,
            Some(plan.cwd),
            Some(plan.agent_id),
            command,
            cols,
            rows,
        )
        .await
}

fn build_launch_plan(request: &AgentLaunchRequest) -> Result<AgentLaunchPlan> {
    let definition = AGENTS
        .iter()
        .find(|definition| definition.id == request.agent_id)
        .ok_or_else(|| anyhow!("Unsupported coding agent: {}", request.agent_id))?;

    if !definition.start_modes.contains(&request.start_mode) {
        bail!(
            "{} does not support the requested start mode",
            definition.name
        );
    }
    if !definition.access_modes.contains(&request.access_mode) {
        bail!(
            "{} does not support the requested access mode",
            definition.name
        );
    }

    let prompt = request.prompt.as_deref().map(str::trim).unwrap_or("");
    if prompt.len() > MAX_INITIAL_PROMPT_BYTES {
        bail!("Initial prompt is too large");
    }
    #[cfg(target_os = "windows")]
    if prompt.encode_utf16().count() > MAX_WINDOWS_PROMPT_UTF16 {
        bail!("Initial prompt is too large for a Windows coding agent launch");
    }
    if prompt.contains('\0') {
        bail!("Initial prompt contains an invalid null byte");
    }

    let cwd = PathBuf::from(&request.cwd)
        .canonicalize()
        .with_context(|| format!("Workspace does not exist: {}", request.cwd))?;
    if !cwd.is_dir() {
        bail!("Workspace is not a directory: {}", cwd.display());
    }

    let path = effective_path();
    let executable = resolve_executable(definition, &path)
        .ok_or_else(|| anyhow!("{} executable was not found", definition.name))?;
    let args = build_args(
        definition.id,
        request.start_mode,
        request.access_mode,
        prompt,
    )?;
    let workspace_name = cwd
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("workspace");

    Ok(AgentLaunchPlan {
        agent_id: definition.id.to_string(),
        display_name: format!("{} · {}", definition.name, workspace_name),
        executable,
        args: args.into_iter().map(OsString::from).collect(),
        cwd,
        path,
    })
}

fn build_process_command(plan: &AgentLaunchPlan) -> Result<CommandBuilder> {
    #[cfg(target_os = "windows")]
    if is_windows_script(&plan.executable) {
        let executable = prefer_powershell_shim(&plan.executable);
        if is_windows_batch_script(&executable) && has_unsafe_windows_batch_argument(&plan.args) {
            bail!(
                "The Windows coding agent batch shim cannot safely receive this prompt; install the matching PowerShell shim or native executable"
            );
        }
        let powershell = resolve_powershell(&plan.path, &plan.cwd)
            .context("PowerShell is required to launch this coding agent shim")?;
        let payload = WindowsShimPayload {
            executable: executable.to_string_lossy().into_owned(),
            arguments: plan
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
        };
        let payload =
            serde_json::to_string(&payload).context("Could not encode launch arguments")?;
        let script_utf16le: Vec<u8> = WINDOWS_SHIM_SCRIPT
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let encoded_script = STANDARD.encode(script_utf16le);

        let mut command = CommandBuilder::new(powershell);
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
        ]);
        command.arg(encoded_script);
        command.env(WINDOWS_SHIM_PAYLOAD_ENV, payload);
        return Ok(command);
    }

    let mut command = CommandBuilder::new(&plan.executable);
    command.args(&plan.args);
    Ok(command)
}

#[cfg(any(target_os = "windows", test))]
fn is_windows_script(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cmd" | "bat" | "ps1"
            )
        })
        .unwrap_or(false)
}

#[cfg(any(target_os = "windows", test))]
fn is_windows_batch_script(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| matches!(extension.to_ascii_lowercase().as_str(), "cmd" | "bat"))
        .unwrap_or(false)
}

#[cfg(any(target_os = "windows", test))]
fn prefer_powershell_shim(path: &std::path::Path) -> PathBuf {
    if is_windows_batch_script(path) {
        let powershell_shim = path.with_extension("ps1");
        if powershell_shim.is_file() {
            return powershell_shim;
        }
    }
    path.to_path_buf()
}

#[cfg(any(target_os = "windows", test))]
fn has_unsafe_windows_batch_argument(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        arg.to_string_lossy().chars().any(|character| {
            matches!(
                character,
                '"' | '&' | '|' | '<' | '>' | '^' | '(' | ')' | '%' | '!' | '\r' | '\n'
            )
        })
    })
}

#[cfg(target_os = "windows")]
fn resolve_powershell(search_path: &OsString, cwd: &std::path::Path) -> Option<PathBuf> {
    let system_powershell = env::var_os("SystemRoot")
        .map(PathBuf::from)
        .map(|root| {
            root.join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .filter(|path| path.is_file());

    system_powershell
        .or_else(|| which::which_in("powershell.exe", Some(search_path), cwd).ok())
        .or_else(|| which::which_in("pwsh.exe", Some(search_path), cwd).ok())
}

fn build_args(
    agent_id: &str,
    start_mode: AgentStartMode,
    access_mode: AgentAccessMode,
    prompt: &str,
) -> Result<Vec<String>> {
    if start_mode == AgentStartMode::ResumePicker
        && !prompt.is_empty()
        && matches!(agent_id, "claude" | "codex")
    {
        bail!("{agent_id} accepts the initial prompt after its resume picker opens");
    }

    let mut args = Vec::new();

    match agent_id {
        "claude" => {
            match start_mode {
                AgentStartMode::New => {}
                AgentStartMode::ContinueLast => args.push("--continue".into()),
                AgentStartMode::ResumePicker => args.push("--resume".into()),
            }
            match access_mode {
                AgentAccessMode::Default => {}
                AgentAccessMode::ReadOnly => {
                    args.extend(["--permission-mode".into(), "plan".into()]);
                }
                AgentAccessMode::AutoEdit => {
                    args.extend(["--permission-mode".into(), "acceptEdits".into()]);
                }
            }
            if !prompt.is_empty() {
                args.push(prompt.to_string());
            }
        }
        "codex" => {
            // Inline mode keeps Codex output in VibeShell's scrollback and replay buffer.
            args.push("--no-alt-screen".into());
            match access_mode {
                AgentAccessMode::Default => {}
                AgentAccessMode::ReadOnly => {
                    args.extend(["--sandbox".into(), "read-only".into()]);
                }
                AgentAccessMode::AutoEdit => {
                    args.extend([
                        "--sandbox".into(),
                        "workspace-write".into(),
                        "--ask-for-approval".into(),
                        "on-request".into(),
                    ]);
                }
            }
            match start_mode {
                AgentStartMode::New => {}
                AgentStartMode::ContinueLast => {
                    args.extend(["resume".into(), "--last".into()]);
                }
                AgentStartMode::ResumePicker => args.push("resume".into()),
            }
            if !prompt.is_empty() {
                args.push(prompt.to_string());
            }
        }
        "opencode" => {
            match start_mode {
                AgentStartMode::New => {}
                AgentStartMode::ContinueLast => args.push("--continue".into()),
                AgentStartMode::ResumePicker => {
                    bail!("OpenCode does not expose a session picker launch flag")
                }
            }
            match access_mode {
                AgentAccessMode::Default => {}
                AgentAccessMode::ReadOnly => {
                    args.extend(["--agent".into(), "plan".into()]);
                }
                AgentAccessMode::AutoEdit => args.push("--auto".into()),
            }
            if !prompt.is_empty() {
                args.extend(["--prompt".into(), prompt.to_string()]);
            }
        }
        "pi" => {
            match start_mode {
                AgentStartMode::New => {}
                AgentStartMode::ContinueLast => args.push("--continue".into()),
                AgentStartMode::ResumePicker => args.push("--resume".into()),
            }
            match access_mode {
                AgentAccessMode::Default => {}
                AgentAccessMode::ReadOnly => {
                    args.extend(["--tools".into(), "read,grep,find,ls".into()]);
                }
                AgentAccessMode::AutoEdit => bail!("Pi has no portable auto-edit flag"),
            }
            if !prompt.is_empty() {
                args.push(prompt.to_string());
            }
        }
        _ => bail!("Unsupported coding agent: {agent_id}"),
    }

    Ok(args)
}

fn resolve_executable(definition: &AgentDefinition, search_path: &OsString) -> Option<PathBuf> {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    definition.executables.iter().find_map(|candidate| {
        which::which_in(candidate, Some(search_path), &cwd)
            .ok()
            .or_else(|| which::which(candidate).ok())
    })
}

fn effective_path() -> OsString {
    let mut paths: Vec<PathBuf> = env::var_os("PATH")
        .as_deref()
        .map(env::split_paths)
        .map(Iterator::collect)
        .unwrap_or_default();

    #[cfg(not(target_os = "windows"))]
    if let Some(login_path) = login_shell_path() {
        paths.splice(0..0, env::split_paths(&login_path));
    }

    if let Some(home) = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
        paths.extend([
            home.join(".local/bin"),
            home.join(".opencode/bin"),
            home.join(".bun/bin"),
            home.join(".npm-global/bin"),
            home.join(".local/share/pnpm"),
            home.join(".volta/bin"),
            home.join(".cargo/bin"),
            home.join("Library/pnpm"),
        ]);
    }

    #[cfg(not(target_os = "windows"))]
    paths.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);

    let mut unique_paths = Vec::new();
    for path in paths {
        if !unique_paths.iter().any(|existing| existing == &path) {
            unique_paths.push(path);
        }
    }
    env::join_paths(unique_paths).unwrap_or_else(|_| env::var_os("PATH").unwrap_or_default())
}

#[cfg(not(target_os = "windows"))]
fn login_shell_path() -> Option<OsString> {
    let shell = env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh"));
    let output = Command::new(shell)
        .args(["-lc", "command printf '__VIBESHELL_PATH__%s' \"$PATH\""])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .rsplit_once("__VIBESHELL_PATH__")
        .map(|(_, path)| OsString::from(path.trim()))
        .filter(|path| !path.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn string_args(
        agent: &str,
        start: AgentStartMode,
        access: AgentAccessMode,
        prompt: &str,
    ) -> Vec<String> {
        build_args(agent, start, access, prompt).expect("launch args should build")
    }

    #[test]
    fn claude_maps_semantic_modes_without_shell_quoting() {
        assert_eq!(
            string_args(
                "claude",
                AgentStartMode::ContinueLast,
                AgentAccessMode::AutoEdit,
                "fix the parser; then test"
            ),
            vec![
                "--continue",
                "--permission-mode",
                "acceptEdits",
                "fix the parser; then test"
            ]
        );
    }

    #[test]
    fn codex_uses_inline_tui_and_resume_last() {
        assert_eq!(
            string_args(
                "codex",
                AgentStartMode::ContinueLast,
                AgentAccessMode::ReadOnly,
                "review"
            ),
            vec![
                "--no-alt-screen",
                "--sandbox",
                "read-only",
                "resume",
                "--last",
                "review"
            ]
        );
    }

    #[test]
    fn opencode_uses_explicit_prompt_flag() {
        assert_eq!(
            string_args(
                "opencode",
                AgentStartMode::New,
                AgentAccessMode::AutoEdit,
                "implement this"
            ),
            vec!["--auto", "--prompt", "implement this"]
        );
    }

    #[test]
    fn pi_read_only_limits_tools() {
        assert_eq!(
            string_args(
                "pi",
                AgentStartMode::ResumePicker,
                AgentAccessMode::ReadOnly,
                ""
            ),
            vec!["--resume", "--tools", "read,grep,find,ls"]
        );
    }

    #[test]
    fn recognizes_windows_script_shims_case_insensitively() {
        assert!(is_windows_script(std::path::Path::new(
            "C:/tools/codex.CMD"
        )));
        assert!(is_windows_script(std::path::Path::new("claude.bat")));
        assert!(is_windows_script(std::path::Path::new("agent.ps1")));
        assert!(!is_windows_script(std::path::Path::new("codex.exe")));
        assert!(!is_windows_script(std::path::Path::new("codex")));
    }

    #[test]
    fn prefers_a_sibling_powershell_shim_for_windows_batch_launchers() {
        let temp = tempfile::tempdir().unwrap();
        let batch = temp.path().join("codex.cmd");
        let powershell = temp.path().join("codex.ps1");
        std::fs::write(&batch, "@echo off\r\n").unwrap();
        std::fs::write(&powershell, "exit $LASTEXITCODE\n").unwrap();

        assert_eq!(prefer_powershell_shim(&batch), powershell);
    }

    #[test]
    fn rejects_arguments_that_cmd_would_reparse() {
        assert!(has_unsafe_windows_batch_argument(&[OsString::from(
            "foo&whoami"
        )]));
        assert!(has_unsafe_windows_batch_argument(&[OsString::from(
            "%PATH%"
        )]));
        assert!(has_unsafe_windows_batch_argument(&[OsString::from(
            "line one\nline two"
        )]));
        assert!(!has_unsafe_windows_batch_argument(&[OsString::from(
            "review src/main.rs"
        )]));
    }

    async fn smoke_launch_installed_agent(agent_id: &str) {
        let manager = LocalShellManager::new();
        let session = launch_agent(
            &manager,
            AgentLaunchRequest {
                agent_id: agent_id.into(),
                cwd: env!("CARGO_MANIFEST_DIR").into(),
                prompt: None,
                start_mode: AgentStartMode::New,
                access_mode: AgentAccessMode::Default,
                cols: Some(100),
                rows: Some(30),
            },
        )
        .await
        .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while session.replay_output().is_empty()
            && session.get_state().await == crate::local_shell::LocalShellState::Running
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        assert_eq!(
            session.get_state().await,
            crate::local_shell::LocalShellState::Running,
            "{agent_id} exited immediately after its PTY launch"
        );
        manager.kill_session(&session.id).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires an installed Claude Code CLI"]
    async fn launches_installed_claude_in_a_real_pty() {
        smoke_launch_installed_agent("claude").await;
    }

    #[tokio::test]
    #[ignore = "requires an installed Codex CLI"]
    async fn launches_installed_codex_in_a_real_pty() {
        smoke_launch_installed_agent("codex").await;
    }
}
