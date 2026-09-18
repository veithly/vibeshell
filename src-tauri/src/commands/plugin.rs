use std::collections::HashMap;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::fs;
use std::sync::Arc;

use chrono::Utc;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::commands::SessionAccessState;
use crate::ipc::{IpcClient, IpcMessage};
use crate::local_shell::LocalShellManager;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::plugins::MAX_MANIFEST_BYTES;
use crate::plugins::{
    builtin_catalog, parse_manifest, ManifestValidationPolicy, PluginExecuteRequest,
    PluginExecutionResult, PluginManifest, PluginPermission, PluginRecord, PluginSource,
    MAX_PLUGIN_OUTPUT_BYTES, MAX_PLUGIN_SETTINGS_BYTES,
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
pub struct PluginExportRequest {
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

/// Export a plugin manifest as a portable, spec-compliant JSON file. Installed
/// external plugins export the manifest they were imported from; built-in
/// plugins export the shipped manifest, which doubles as an authoring
/// template. Returns `None` when the user cancels the save dialog. Settings
/// stay out of the export on purpose — they are device state and travel with
/// the workspace backup instead.
#[tauri::command]
pub async fn plugin_export(
    db: State<'_, Arc<Database>>,
    request: PluginExportRequest,
) -> Result<Option<String>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (&db, &request);
        return Err("Plugin manifest export is unavailable on mobile".to_string());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let manifest = export_manifest_for(&db, &request.plugin_id)?;

        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("Failed to encode plugin manifest: {}", error))?
            + "\n";

        let path = FileDialog::new()
            .add_filter("VibeShell plugin manifest", &["json"])
            .set_title("Export VibeShell Plugin")
            .set_file_name(format!("{}-{}.plugin.json", manifest.id, manifest.version))
            .save_file();

        let Some(path) = path else {
            return Ok(None);
        };

        fs::write(&path, manifest_json)
            .map_err(|error| format!("Failed to write plugin manifest: {}", error))?;
        Ok(Some(path.to_string_lossy().into_owned()))
    }
}

fn export_manifest_for(db: &Database, plugin_id: &str) -> Result<PluginManifest, String> {
    if let Some(installation) = db
        .plugin_installation_get(plugin_id)
        .map_err(|error| error.to_string())?
    {
        let source = PluginSource::parse(&installation.source)?;
        return manifest_for_installation(&installation, &source);
    }

    // Not installed: only built-in catalog entries can be exported (as an
    // authoring template). External plugins disappear with their installation.
    builtin_catalog()?
        .into_iter()
        .find(|manifest| manifest.id == plugin_id)
        .ok_or_else(|| format!("Plugin not found: {}", plugin_id))
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

/// Whether an installed plugin may exercise `required` right now.
///
/// Built-in manifests ship with the app: when an update grants a new
/// permission (e.g. `local_exec` for local sessions), installations made by an
/// older build still hold the stale grant snapshot — deriving from the current
/// catalog manifest self-heals those instead of forcing a reinstall. External
/// manifests are user-reviewed at enable time and can only change through a
/// re-import (which revokes grants), so their stored snapshot stays
/// authoritative.
pub(crate) fn permission_satisfied(
    source: PluginSource,
    manifest: &PluginManifest,
    granted: &[PluginPermission],
    required: &PluginPermission,
) -> bool {
    match source {
        PluginSource::Builtin => manifest.permissions.contains(required),
        PluginSource::External => granted.contains(required),
    }
}

#[tauri::command]
pub async fn plugin_execute(
    manager: State<'_, Arc<SessionManager>>,
    access_state: State<'_, Arc<SessionAccessState>>,
    db: State<'_, Arc<Database>>,
    request: PluginExecuteRequest,
    local_shell_manager: State<'_, Arc<LocalShellManager>>,
) -> Result<PluginExecutionResult, String> {
    let local = local_shell_manager
        .get_session(&request.session_id)
        .await
        .is_some();
    if !local && access_state.is_remote_session(&request.session_id).await {
        return match ipc_send(IpcMessage::PluginExecute { request }).await? {
            IpcMessage::PluginData { data } => {
                serde_json::from_value(data).map_err(|error| error.to_string())
            }
            IpcMessage::Error { message } => Err(message),
            _ => Err("Unexpected plugin RPC response".into()),
        };
    }
    crate::plugins::agent::execute(&db, &manager, request, "ui.plugin", None).await
}

/// Capture at most one plugin-sized buffer per stream, but keep draining both
/// streams so a verbose child cannot deadlock on a full pipe. All work is timed.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) async fn run_local_command(
    command: &str,
    stdin: Option<&str>,
) -> Result<String, String> {
    use std::process::Stdio;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
    async fn capture(mut reader: impl AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
        let mut output = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let count = reader.read(&mut chunk).await?;
            if count == 0 {
                return Ok(output);
            }
            let keep = count.min((MAX_PLUGIN_OUTPUT_BYTES + 1).saturating_sub(output.len()));
            output.extend_from_slice(&chunk[..keep]);
        }
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut child = tokio::process::Command::new(shell)
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("Failed to spawn plugin: {error}"))?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("Missing stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| std::io::Error::other("Missing stderr"))?;
        let write = async {
            if let Some(mut handle) = child.stdin.take() {
                if let Some(password) = stdin {
                    handle.write_all(format!("{password}\n").as_bytes()).await?;
                }
            }
            Ok::<_, std::io::Error>(())
        };
        let (mut output, stderr, ()) = tokio::try_join!(capture(stdout), capture(stderr), write)?;
        let status = child.wait().await?;
        output.extend_from_slice(&stderr);
        Ok::<_, std::io::Error>((status, String::from_utf8_lossy(&output).into_owned()))
    })
    .await
    .map_err(|_| "Local plugin timed out after 60s".to_string())?
    .map_err(|error| format!("Local plugin failed: {error}"))?;
    if !result.0.success() {
        return Err(format!(
            "Local plugin exited with {}: {}",
            result.0,
            result.1.chars().take(4096).collect::<String>()
        ));
    }
    Ok(result.1)
}

pub(crate) fn list_plugins(db: &Database) -> Result<Vec<PluginRecord>, String> {
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

pub(crate) fn manifest_for_installation(
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

pub(crate) fn truncate_output(mut output: String) -> (String, bool) {
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
    fn built_in_grants_self_heal_from_the_current_manifest() {
        let manifest = builtin_catalog()
            .unwrap()
            .into_iter()
            .find(|manifest| manifest.id == "docker-containers")
            .unwrap();
        // An installation made before local_exec existed holds only
        // remote_exec, but a local session must still work: the current
        // built-in manifest is authoritative.
        assert!(permission_satisfied(
            PluginSource::Builtin,
            &manifest,
            &[PluginPermission::RemoteExec],
            &PluginPermission::LocalExec,
        ));
        // External plugins keep the reviewed snapshot as the source of truth.
        let external = serde_json::from_value::<PluginManifest>(serde_json::json!({
            "schemaVersion": 1,
            "id": "example.tools",
            "name": "Tools",
            "description": "Example",
            "version": "1.0.0",
            "author": "Example",
            "category": "operations",
            "icon": "wrench",
            "permissions": ["remote_exec", "local_exec"],
            "sessionTypes": ["ssh", "local"],
            "entry": {
                "type": "commands",
                "actions": [{
                    "id": "version", "name": "Version", "description": "Show version",
                    "program": "tool", "args": ["--version"]
                }]
            }
        }))
        .unwrap();
        assert!(permission_satisfied(
            PluginSource::External,
            &external,
            &[PluginPermission::LocalExec],
            &PluginPermission::LocalExec,
        ));
        assert!(!permission_satisfied(
            PluginSource::External,
            &external,
            &[PluginPermission::LocalExec],
            &PluginPermission::RemoteExec,
        ));
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

    #[cfg(unix)]
    #[tokio::test]
    async fn local_plugin_reports_failure_and_closes_unused_stdin() {
        let output = run_local_command("cat; printf PLUGIN_OK", None)
            .await
            .unwrap();
        assert_eq!(output, "PLUGIN_OK");
        let error = run_local_command("printf PLUGIN_FAILED >&2; exit 7", None)
            .await
            .unwrap_err();
        assert!(error.contains("7") && error.contains("PLUGIN_FAILED"));
    }

    #[test]
    fn every_plugin_exposes_live_actions_and_reference_without_settings() {
        use crate::plugins::agent;
        let (_directory, db) = test_db();
        assert!(agent::list(&db, true).unwrap().is_empty());
        let catalog = builtin_catalog().unwrap();
        for manifest in &catalog {
            install_builtin(&db, &manifest.id).unwrap();
            db.plugin_installation_update_settings(
                &manifest.id,
                r#"{"privateSetting":"must-not-leak"}"#,
            )
            .unwrap();
            let description = agent::describe(&db, &manifest.id, false).unwrap();
            assert_eq!(description["installed"], true);
            assert!(!description["actions"].as_array().unwrap().is_empty());
            assert!(!description.to_string().contains("must-not-leak"));
            let reference = agent::describe(&db, &manifest.id, true).unwrap();
            let reference = reference.as_str().unwrap();
            assert!(reference.contains(&format!("vibeshell plugins docs {}", manifest.id)));
            assert!(!reference.contains("must-not-leak"));
            for action in description["actions"].as_array().unwrap() {
                assert_eq!(action["inputSchema"]["additionalProperties"], false);
                assert!(reference.contains(&format!("## `{}`", action["id"].as_str().unwrap())));
            }
        }
        assert_eq!(agent::list(&db, true).unwrap().len(), catalog.len());
        set_plugin_enabled(&db, "server-performance", false).unwrap();
        assert_eq!(
            agent::describe(&db, "server-performance", false).unwrap()["enabled"],
            false
        );
        assert!(agent::describe(&db, "does-not-exist", true).is_err());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[tokio::test]
    async fn imported_plugin_api_enforces_grants_inputs_and_exact_reviewed_command() {
        use crate::plugins::{agent, PluginExecuteRequest};
        let (_directory, db) = test_db();
        let manifest = r#"{
          "schemaVersion":1,"id":"example.ai","name":"AI Fixture","description":"Safe fixture",
          "version":"1.0.0","author":"Tests","category":"operations","icon":"wrench",
          "permissions":["remote_exec"],"sessionTypes":["ssh"],
          "entry":{"type":"commands","actions":[{"id":"show","name":"Show","description":"Show test text",
            "program":"printf","args":["%s","{{input.text}}"],"requiresConfirmation":true,
            "inputs":[{"id":"text","label":"Text","kind":"text","required":true}]}]}}
        "#;
        let imported = install_external_manifest(&db, manifest).unwrap();
        assert!(!imported.enabled);
        let mut request: PluginExecuteRequest = serde_json::from_value(serde_json::json!({
            "pluginId":"example.ai","actionId":"show","sessionId":"not-connected",
            "inputs":{"text":"a 'quoted' value"}
        }))
        .unwrap();
        assert!(agent::prepare(&db, &request, false).is_err());
        set_plugin_enabled(&db, "example.ai", true).unwrap();
        let prepared = agent::prepare(&db, &request, false).unwrap();
        assert!(prepared.requires_confirmation);
        assert!(agent::describe(&db, "example.ai", true)
            .unwrap()
            .as_str()
            .unwrap()
            .contains("## `show`"));
        assert!(agent::prepare(&db, &request, true).is_err());
        let manager = SessionManager::new(Arc::new(
            Database::new_at(_directory.path().join("empty.db")).unwrap(),
        ));
        assert!(
            agent::execute(&db, &manager, request.clone(), "test.plugin", None)
                .await
                .unwrap_err()
                .contains("approval")
        );
        request.confirmed = true;
        assert!(agent::execute(
            &db,
            &manager,
            request.clone(),
            "test.plugin",
            Some("a different reviewed command")
        )
        .await
        .unwrap_err()
        .contains("changed"));
        request
            .inputs
            .insert("extra".into(), serde_json::json!(true));
        assert!(agent::prepare(&db, &request, false).is_err());
        request.inputs.remove("extra");
        request
            .inputs
            .insert("text".into(), serde_json::json!("new\nline"));
        assert!(agent::prepare(&db, &request, false).is_err());
        request
            .inputs
            .insert("text".into(), serde_json::json!("ordinary"));
        request.sudo_password = Some("hidden-password".into());
        assert!(!format!("{request:?}").contains("hidden-password"));
        request.try_sudo = true;
        assert!(agent::prepare(&db, &request, false).is_err());
        request.try_sudo = false;
        let mut installation = db.plugin_installation_get("example.ai").unwrap().unwrap();
        installation.granted_permissions_json = "[]".into();
        db.plugin_installation_upsert(&installation).unwrap();
        assert!(agent::prepare(&db, &request, false).is_err());
        assert!(db.agent_activity_list(None, None, 100).unwrap().is_empty());
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
