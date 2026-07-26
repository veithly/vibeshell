//! Tauri commands for launching local coding agents and inspecting their worktrees.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::coding_agent::{
    self, AgentLaunchRequest, AgentToolInfo, GitWorkspaceDiff, GitWorkspaceStatus,
    WorkspaceDiffRequest, WorkspaceStatusRequest,
};
use crate::local_shell::{LocalShellInfo, LocalShellManager};

#[tauri::command]
pub async fn coding_agent_list() -> Vec<AgentToolInfo> {
    tokio::task::spawn_blocking(coding_agent::list_agents)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn coding_agent_launch(
    app: AppHandle,
    manager: State<'_, Arc<LocalShellManager>>,
    request: AgentLaunchRequest,
) -> Result<LocalShellInfo, String> {
    let session = coding_agent::launch_agent(manager.inner(), request)
        .await
        .map_err(|error| error.to_string())?;
    let info = session.get_info().await;
    super::local_shell::ensure_local_shell_output_bridge(app, session);
    Ok(info)
}

#[tauri::command]
pub async fn coding_agent_workspace_status(
    request: WorkspaceStatusRequest,
) -> Result<GitWorkspaceStatus, String> {
    tokio::task::spawn_blocking(move || coding_agent::get_workspace_status(request))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn coding_agent_workspace_diff(
    request: WorkspaceDiffRequest,
) -> Result<GitWorkspaceDiff, String> {
    tokio::task::spawn_blocking(move || coding_agent::get_workspace_diff(request))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}
