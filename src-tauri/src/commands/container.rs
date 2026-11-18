use super::session::SessionAccessState;
use crate::{
    ipc::runtime_services::{self, RuntimeRequest},
    session::{
        container::{ContainerInfo, ContainerOpenRequest},
        SessionInfo, SessionManager,
    },
};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn container_list(
    manager: State<'_, Arc<SessionManager>>,
    access: State<'_, Arc<SessionAccessState>>,
    parent_session_id: String,
) -> Result<Vec<ContainerInfo>, String> {
    if access.is_remote() {
        return runtime_services::call(RuntimeRequest::ContainerList { parent_session_id }).await;
    }
    manager
        .list_containers(&parent_session_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn container_open(
    manager: State<'_, Arc<SessionManager>>,
    access: State<'_, Arc<SessionAccessState>>,
    request: ContainerOpenRequest,
) -> Result<SessionInfo, String> {
    if access.is_remote() {
        return runtime_services::call(RuntimeRequest::ContainerOpen { request }).await;
    }
    Ok(manager
        .create_container(request)
        .await
        .map_err(|e| e.to_string())?
        .get_info()
        .await)
}
