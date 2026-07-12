use std::collections::HashMap;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::fs;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::commands::SessionAccessState;
use crate::ipc::{IpcClient, IpcMessage};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::plugins::MAX_MANIFEST_BYTES;
use crate::plugins::{
    builtin_catalog, parse_manifest, render_remote_command, ManifestValidationPolicy, PluginEntry,
    PluginExecuteRequest, PluginExecutionResult, PluginManifest, PluginPermission, PluginRecord,
    PluginSource, MAX_PLUGIN_OUTPUT_BYTES, MAX_PLUGIN_SETTINGS_BYTES,
};
use crate::session::SessionManager;
use crate::storage::{Database, PluginInstallation};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginIdRequest {
    pub plugin_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEnabledRequest {
    pub plugin_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingsRequest {
    pub plugin_id: String,
    pub settings: Value,
}

#[tauri::command]
pub fn plugin_list(db: State<'_, Arc<Database>>) -> Result<Vec<PluginRecord>, String> {
    list_plugins(&db)
}

#[tauri::command]
pub fn plugin_install(
    db: State<'_, Arc<Database>>,
    request: PluginIdRequest,
) -> Result<PluginRecord, String> {
    install_builtin(&db, &request.plugin_id)
}

#[tauri::command]
pub async fn plugin_import(db: State<'_, Arc<Database>>) -> Result<Option<PluginRecord>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = db;
        return Err("Plugin manifest import is unavailable on mobile".to_string());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let path = FileDialog::new()
            .add_filter("VibeShell plugin manifest", &["json"])
            .set_title("Import VibeShell Plugin")
            .pick_file();

        let Some(path) = path else {
            return Ok(None);
        };

        let metadata = fs::metadata(&path)
            .map_err(|error| format!("Failed to inspect plugin manifest: {}", error))?;
        if metadata.len() as usize > MAX_MANIFEST_BYTES {
            return Err(format!(
                "Plugin manifest exceeds the {} KB limit",
                MAX_MANIFEST_BYTES / 1024
            ));
        }

        let manifest_json = fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read plugin manifest: {}", error))?;
        install_external_manifest(&db, &manifest_json).map(Some)
    }
}

#[tauri::command]
pub fn plugin_uninstall(
    db: State<'_, Arc<Database>>,
    request: PluginIdRequest,
) -> Result<(), String> {
    let existing = db
        .plugin_installation_get(&request.plugin_id)
        .map_err(|error| error.to_string())?;
    if existing.is_none() {
        return Err(format!("Plugin is not installed: {}", request.plugin_id));
    }

    db.plugin_installation_delete(&request.plugin_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn plugin_set_enabled(
    db: State<'_, Arc<Database>>,
    request: PluginEnabledRequest,
) -> Result<PluginRecord, String> {
    set_plugin_enabled(&db, &request.plugin_id, request.enabled)
}

#[tauri::command]
pub fn plugin_update_settings(
    db: State<'_, Arc<Database>>,
    request: PluginSettingsRequest,
) -> Result<PluginRecord, String> {
    if !request.settings.is_object() {
        return Err("Plugin settings must be a JSON object".to_string());
    }
    let settings_json = serde_json::to_string(&request.settings)
        .map_err(|error| format!("Failed to encode plugin settings: {}", error))?;
    if settings_json.len() > MAX_PLUGIN_SETTINGS_BYTES {
        return Err(format!(
            "Plugin settings exceed the {} KB limit",
            MAX_PLUGIN_SETTINGS_BYTES / 1024
        ));
    }

    let existing = db
        .plugin_installation_get(&request.plugin_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Plugin is not installed: {}", request.plugin_id))?;

    db.plugin_installation_update_settings(&request.plugin_id, &settings_json)
        .map_err(|error| error.to_string())?;
    let mut updated = existing;
    updated.settings_json = settings_json;
    record_from_installation(&updated)
}

#[tauri::command]
pub async fn plugin_execute(
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    db: State<'_, Arc<Database>>,
    request: PluginExecuteRequest,
) -> Result<PluginExecutionResult, String> {
    let installation = db
        .plugin_installation_get(&request.plugin_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Plugin is not installed: {}", request.plugin_id))?;

    if !installation.enabled {
        return Err(format!("Plugin is disabled: {}", request.plugin_id));
    }

    let source = PluginSource::parse(&installation.source)?;
    let manifest = manifest_for_installation(&installation, &source)?;
    let granted_permissions: Vec<PluginPermission> =
        serde_json::from_str(&installation.granted_permissions_json)
            .map_err(|error| format!("Stored plugin permissions are invalid: {}", error))?;
    if !granted_permissions.contains(&PluginPermission::RemoteExec) {
        return Err(format!(
            "Plugin {} has not been granted remote_exec permission",
            request.plugin_id
        ));
    }

    let PluginEntry::Commands { actions } = &manifest.entry else {
        return Err(format!(
            "Plugin {} does not expose command actions",
            request.plugin_id
        ));
    };
    let action = actions
        .iter()
        .find(|action| action.id == request.action_id)
        .ok_or_else(|| {
            format!(
                "Plugin action not found: {}/{}",
                request.plugin_id, request.action_id
            )
        })?;
    let command = bound_remote_output(&render_remote_command(action, &request.inputs)?);

    let started = Instant::now();
    let output = if let Some(session) = manager.get(&request.session_id).await {
        session
            .exec_command(&command)
            .await
            .map_err(|error| format!("Plugin command failed: {}", error))?
    } else if access_state.is_remote() {
        match ipc_send(IpcMessage::ExecCommand {
            session_id: request.session_id.clone(),
            command,
        })
        .await?
        {
            IpcMessage::CommandOutput { output } => output,
            IpcMessage::Error { message } => return Err(message),
            other => {
                return Err(format!(
                    "Unexpected IPC response while running plugin: {:?}",
                    other
                ));
            }
        }
    } else {
        return Err(format!("Session not found: {}", request.session_id));
    };

    let (output, truncated) = truncate_output(output);
    Ok(PluginExecutionResult {
        plugin_id: request.plugin_id,
        action_id: request.action_id,
        output,
        duration_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        truncated,
    })
}

fn list_plugins(db: &Database) -> Result<Vec<PluginRecord>, String> {
    let installations = db
        .plugin_installation_list()
        .map_err(|error| error.to_string())?;
    let installed_by_id: HashMap<&str, &PluginInstallation> = installations
        .iter()
        .map(|installation| (installation.plugin_id.as_str(), installation))
        .collect();

    let mut records = Vec::new();
    for manifest in builtin_catalog()? {
        if let Some(installation) = installed_by_id.get(manifest.id.as_str()) {
            let mut record = record_from_installation(installation)?;
            record.manifest = manifest;
            records.push(record);
        } else {
            records.push(PluginRecord {
                settings: manifest.default_settings.clone(),
                manifest,
                source: PluginSource::Builtin,
                installed: false,
                enabled: false,
                granted_permissions: Vec::new(),
                installed_at: None,
            });
        }
    }

    for installation in installations {
        if installation.source == PluginSource::External.as_str() {
            records.push(record_from_installation(&installation)?);
        }
    }

    Ok(records)
}

fn install_builtin(db: &Database, plugin_id: &str) -> Result<PluginRecord, String> {
    let manifest = builtin_catalog()?
        .into_iter()
        .find(|manifest| manifest.id == plugin_id)
        .ok_or_else(|| format!("Built-in plugin not found: {}", plugin_id))?;
    let existing = db
        .plugin_installation_get(plugin_id)
        .map_err(|error| error.to_string())?;
    if existing
        .as_ref()
        .is_some_and(|installation| installation.source != PluginSource::Builtin.as_str())
    {
        return Err(format!(
            "Plugin id is already used by an external plugin: {}",
            plugin_id
        ));
    }

    let now = Utc::now().timestamp();
    let installation = PluginInstallation {
        plugin_id: manifest.id.clone(),
        version: manifest.version.clone(),
        manifest_json: serde_json::to_string(&manifest)
            .map_err(|error| format!("Failed to encode plugin manifest: {}", error))?,
        source: PluginSource::Builtin.as_str().to_string(),
        enabled: true,
        granted_permissions_json: serde_json::to_string(&manifest.permissions)
            .map_err(|error| format!("Failed to encode plugin permissions: {}", error))?,
        settings_json: existing
            .as_ref()
            .map(|installation| installation.settings_json.clone())
            .unwrap_or_else(|| manifest.default_settings.to_string()),
        installed_at: existing
            .as_ref()
            .map(|installation| installation.installed_at)
            .unwrap_or(now),
        updated_at: now,
    };
    db.plugin_installation_upsert(&installation)
        .map_err(|error| error.to_string())?;
    record_from_installation(&installation)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn install_external_manifest(db: &Database, manifest_json: &str) -> Result<PluginRecord, String> {
    let manifest = parse_manifest(manifest_json, ManifestValidationPolicy::External)?;
    if builtin_catalog()?
        .iter()
        .any(|builtin| builtin.id == manifest.id)
    {
        return Err(format!(
            "External plugin id conflicts with a built-in plugin: {}",
            manifest.id
        ));
    }

    let existing = db
        .plugin_installation_get(&manifest.id)
        .map_err(|error| error.to_string())?;
    let now = Utc::now().timestamp();
    let installation = PluginInstallation {
        plugin_id: manifest.id.clone(),
        version: manifest.version.clone(),
        manifest_json: serde_json::to_string(&manifest)
            .map_err(|error| format!("Failed to encode plugin manifest: {}", error))?,
        source: PluginSource::External.as_str().to_string(),
        enabled: false,
        granted_permissions_json: "[]".to_string(),
        settings_json: existing
            .as_ref()
            .filter(|installation| installation.source == PluginSource::External.as_str())
            .map(|installation| installation.settings_json.clone())
            .unwrap_or_else(|| manifest.default_settings.to_string()),
        installed_at: existing
            .as_ref()
            .map(|installation| installation.installed_at)
            .unwrap_or(now),
        updated_at: now,
    };
    db.plugin_installation_upsert(&installation)
        .map_err(|error| error.to_string())?;
    record_from_installation(&installation)
}

fn set_plugin_enabled(
    db: &Database,
    plugin_id: &str,
    enabled: bool,
) -> Result<PluginRecord, String> {
    let mut installation = db
        .plugin_installation_get(plugin_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Plugin is not installed: {}", plugin_id))?;
    let source = PluginSource::parse(&installation.source)?;
    let manifest = manifest_for_installation(&installation, &source)?;

    installation.enabled = enabled;
    installation.granted_permissions_json = if enabled {
        serde_json::to_string(&manifest.permissions)
            .map_err(|error| format!("Failed to encode plugin permissions: {}", error))?
    } else {
        "[]".to_string()
    };
    installation.updated_at = Utc::now().timestamp();
    db.plugin_installation_upsert(&installation)
        .map_err(|error| error.to_string())?;
    record_from_installation(&installation)
}

fn manifest_for_installation(
    installation: &PluginInstallation,
    source: &PluginSource,
) -> Result<PluginManifest, String> {
    if source == &PluginSource::Builtin {
        return builtin_catalog()?
            .into_iter()
            .find(|manifest| manifest.id == installation.plugin_id)
            .ok_or_else(|| format!("Built-in plugin not found: {}", installation.plugin_id));
    }

    parse_manifest(
        &installation.manifest_json,
        ManifestValidationPolicy::External,
    )
}

fn record_from_installation(installation: &PluginInstallation) -> Result<PluginRecord, String> {
    let source = PluginSource::parse(&installation.source)?;
    let manifest = manifest_for_installation(installation, &source)?;
    let granted_permissions =
        serde_json::from_str(&installation.granted_permissions_json).unwrap_or_default();
    let settings = serde_json::from_str(&installation.settings_json)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| manifest.default_settings.clone());

    Ok(PluginRecord {
        manifest,
        source,
        installed: true,
        enabled: installation.enabled,
        granted_permissions,
        settings,
        installed_at: Some(installation.installed_at),
    })
}

fn truncate_output(mut output: String) -> (String, bool) {
    if output.len() <= MAX_PLUGIN_OUTPUT_BYTES {
        return (output, false);
    }

    let mut boundary = MAX_PLUGIN_OUTPUT_BYTES;
    while !output.is_char_boundary(boundary) {
        boundary -= 1;
    }
    output.truncate(boundary);
    (output, true)
}

fn bound_remote_output(command: &str) -> String {
    format!(
        "({}) 2>&1 | head -c {}",
        command,
        MAX_PLUGIN_OUTPUT_BYTES + 1
    )
}

async fn ipc_send(message: IpcMessage) -> Result<IpcMessage, String> {
    tokio::task::spawn_blocking(move || {
        IpcClient::send(&message).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("IPC worker failed: {}", error))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (tempfile::TempDir, Database) {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::new_at(directory.path().join("plugins.db")).unwrap();
        (directory, db)
    }

    #[test]
    fn built_in_install_enables_plugin_and_uninstall_keeps_catalog_entry() {
        let (_directory, db) = test_db();
        let installed = install_builtin(&db, "server-performance").unwrap();
        assert!(installed.installed);
        assert!(installed.enabled);

        db.plugin_installation_delete("server-performance").unwrap();
        let catalog_entry = list_plugins(&db)
            .unwrap()
            .into_iter()
            .find(|plugin| plugin.manifest.id == "server-performance")
            .unwrap();
        assert!(!catalog_entry.installed);
        assert!(!catalog_entry.enabled);
    }

    #[test]
    fn external_import_starts_disabled_and_cannot_shadow_builtins() {
        let (_directory, db) = test_db();
        let json = r#"{
          "schemaVersion": 1,
          "id": "example.remote-tools",
          "name": "Remote Tools",
          "description": "Read remote tool output",
          "version": "1.0.0",
          "author": "Example",
          "category": "operations",
          "icon": "wrench",
          "permissions": ["remote_exec"],
          "sessionTypes": ["ssh"],
          "entry": {
            "type": "commands",
            "actions": [{
              "id": "version",
              "name": "Version",
              "description": "Show the tool version",
              "program": "tool",
              "args": ["--version"]
            }]
          }
        }"#;
        let imported = install_external_manifest(&db, json).unwrap();
        assert!(imported.installed);
        assert!(!imported.enabled);
        assert_eq!(imported.source, PluginSource::External);

        let conflicting = json.replace("example.remote-tools", "server-performance");
        assert!(install_external_manifest(&db, &conflicting)
            .unwrap_err()
            .contains("conflicts with a built-in plugin"));
    }

    #[test]
    fn external_reimport_preserves_settings_and_revokes_permissions() {
        let (_directory, db) = test_db();
        let manifest = r#"{
          "schemaVersion": 1,
          "id": "example.updatable",
          "name": "Updatable",
          "description": "Plugin update fixture",
          "version": "1.0.0",
          "author": "Example",
          "category": "operations",
          "icon": "wrench",
          "permissions": ["remote_exec"],
          "sessionTypes": ["ssh"],
          "entry": {
            "type": "commands",
            "actions": [{
              "id": "version",
              "name": "Version",
              "description": "Show a version",
              "program": "tool"
            }]
          }
        }"#;
        install_external_manifest(&db, manifest).unwrap();
        db.plugin_installation_update_settings("example.updatable", r#"{"rows":25}"#)
            .unwrap();
        set_plugin_enabled(&db, "example.updatable", true).unwrap();

        let updated_manifest = manifest.replace("1.0.0", "2.0.0");
        let updated = install_external_manifest(&db, &updated_manifest).unwrap();
        assert_eq!(updated.manifest.version, "2.0.0");
        assert_eq!(updated.settings, serde_json::json!({ "rows": 25 }));
        assert!(!updated.enabled);
        assert!(updated.granted_permissions.is_empty());
    }

    #[test]
    fn disabling_plugin_revokes_remote_execution_permission() {
        let (_directory, db) = test_db();
        install_builtin(&db, "docker-containers").unwrap();
        let disabled = set_plugin_enabled(&db, "docker-containers", false).unwrap();
        assert!(!disabled.enabled);
        assert!(disabled.granted_permissions.is_empty());
    }

    #[test]
    fn remote_command_caps_output_before_it_reaches_the_client() {
        assert_eq!(
            bound_remote_output("docker ps"),
            "(docker ps) 2>&1 | head -c 1000001"
        );
    }

    #[test]
    fn output_truncation_preserves_utf8_boundaries() {
        let output = "é".repeat(600_000);
        let (truncated, was_truncated) = truncate_output(output);
        assert!(was_truncated);
        assert!(truncated.len() <= MAX_PLUGIN_OUTPUT_BYTES);
        assert!(truncated.is_char_boundary(truncated.len()));
    }
}
