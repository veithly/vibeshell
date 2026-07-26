use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use crate::cloud_sync::{
    CloudSyncFileReport, CloudSyncManager, CloudSyncPairingInfo, CloudSyncReport, CloudSyncStatus,
    SyncProviderKind,
};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use rfd::FileDialog;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCloudSyncVaultRequest {
    pub provider: SyncProviderKind,
    pub endpoint: Option<String>,
    pub gist_id: Option<String>,
    pub token: Option<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinCloudSyncVaultRequest {
    pub pairing_code: String,
}

#[tauri::command]
pub async fn cloud_sync_create_vault(
    manager: State<'_, Arc<CloudSyncManager>>,
    request: CreateCloudSyncVaultRequest,
) -> Result<CloudSyncPairingInfo, String> {
    let result = match request.provider {
        SyncProviderKind::GithubGist => {
            let token = request
                .token
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "A GitHub token is required".to_string())?;
            let gist_id = request
                .gist_id
                .and_then(|value| normalize_optional_gist_id(&value));
            manager.create_github_gist_vault(gist_id, token).await
        }
        SyncProviderKind::WebDav => {
            let endpoint = request
                .endpoint
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "A WebDAV file URL is required".to_string())?;
            manager
                .create_webdav_vault(endpoint, request.username, request.password)
                .await
        }
    };
    result.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cloud_sync_join_vault(
    manager: State<'_, Arc<CloudSyncManager>>,
    request: JoinCloudSyncVaultRequest,
) -> Result<CloudSyncPairingInfo, String> {
    manager
        .join_vault(&request.pairing_code)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cloud_sync_lock(manager: State<'_, Arc<CloudSyncManager>>) -> Result<(), String> {
    manager.lock().await;
    Ok(())
}

#[tauri::command]
pub fn cloud_sync_status(manager: State<'_, Arc<CloudSyncManager>>) -> CloudSyncStatus {
    manager.status()
}

#[tauri::command]
pub async fn cloud_sync_now(
    manager: State<'_, Arc<CloudSyncManager>>,
) -> Result<CloudSyncReport, String> {
    manager.sync_now().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cloud_sync_export_file(
    manager: State<'_, Arc<CloudSyncManager>>,
) -> Result<Option<CloudSyncFileReport>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = manager;
        return Err("Portable sync file export is unavailable on mobile".to_string());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let path = FileDialog::new()
            .set_title("Export VibeShell Workspace")
            .set_file_name("vibeshell-workspace.vibeshell-sync.json")
            .add_filter("VibeShell Sync", &["json"])
            .save_file();
        path.map(|path| {
            manager
                .export_to_file(&path)
                .map_err(|error| error.to_string())
        })
        .transpose()
    }
}

#[tauri::command]
pub async fn cloud_sync_import_file(
    manager: State<'_, Arc<CloudSyncManager>>,
) -> Result<Option<CloudSyncFileReport>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = manager;
        return Err("Portable sync file import is unavailable on mobile".to_string());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let path = FileDialog::new()
            .set_title("Import VibeShell Workspace")
            .add_filter("VibeShell Sync", &["json"])
            .pick_file();
        path.map(|path| {
            manager
                .import_from_file(&path)
                .map_err(|error| error.to_string())
        })
        .transpose()
    }
}

fn normalize_optional_gist_id(value: &str) -> Option<String> {
    value
        .trim()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::normalize_optional_gist_id;

    #[test]
    fn accepts_a_gist_id_or_full_gist_url() {
        assert_eq!(
            normalize_optional_gist_id("aabbcc"),
            Some("aabbcc".to_string())
        );
        assert_eq!(
            normalize_optional_gist_id("https://gist.github.com/rick/aabbcc/"),
            Some("aabbcc".to_string())
        );
        assert_eq!(normalize_optional_gist_id("  "), None);
    }
}
