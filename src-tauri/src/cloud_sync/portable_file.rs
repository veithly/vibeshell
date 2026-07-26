use std::{fs, path::Path};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::storage::{Database, PendingSyncBatch};

const PORTABLE_FILE_FORMAT: &str = "vibeshell-portable-sync";
const PORTABLE_FILE_VERSION: u32 = 1;
const MAX_PORTABLE_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncFileReport {
    pub operation: CloudSyncFileOperation,
    pub path: String,
    pub exported: usize,
    pub imported: usize,
    pub applied: usize,
    pub ignored: usize,
    pub conflicts: usize,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudSyncFileOperation {
    #[default]
    Export,
    Import,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortableSyncFile {
    format: String,
    version: u32,
    exported_at: i64,
    batch: PendingSyncBatch,
}

pub fn export_to_path(database: &Database, path: &Path) -> Result<CloudSyncFileReport> {
    let batch = database.cloud_sync().export_snapshot()?;
    let exported = batch.changes.len();
    let file = PortableSyncFile {
        format: PORTABLE_FILE_FORMAT.to_string(),
        version: PORTABLE_FILE_VERSION,
        exported_at: Utc::now().timestamp(),
        batch,
    };
    let contents = serde_json::to_vec_pretty(&file)?;
    if contents.len() as u64 > MAX_PORTABLE_FILE_BYTES {
        bail!("Portable sync export exceeds the 16 MiB size limit");
    }
    fs::write(path, contents)
        .with_context(|| format!("Failed to write portable sync file: {}", path.display()))?;

    Ok(CloudSyncFileReport {
        operation: CloudSyncFileOperation::Export,
        path: path.to_string_lossy().into_owned(),
        exported,
        ..CloudSyncFileReport::default()
    })
}

pub fn import_from_path(database: &Database, path: &Path) -> Result<CloudSyncFileReport> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to inspect portable sync file: {}", path.display()))?;
    if metadata.len() > MAX_PORTABLE_FILE_BYTES {
        bail!("Portable sync import exceeds the 16 MiB size limit");
    }
    let contents = fs::read(path)
        .with_context(|| format!("Failed to read portable sync file: {}", path.display()))?;
    if contents.len() as u64 > MAX_PORTABLE_FILE_BYTES {
        bail!("Portable sync import exceeds the 16 MiB size limit");
    }
    let file: PortableSyncFile =
        serde_json::from_slice(&contents).context("Invalid VibeShell portable sync file")?;
    if file.format != PORTABLE_FILE_FORMAT || file.version != PORTABLE_FILE_VERSION {
        bail!("Unsupported VibeShell portable sync file version");
    }
    if file.batch.changes.iter().any(|change| change.deleted) {
        bail!("Portable sync files cannot contain deletion records");
    }

    let imported = file.batch.changes.len();
    let report = database
        .cloud_sync()
        .apply_imported_changes(&file.batch.changes)?;
    Ok(CloudSyncFileReport {
        operation: CloudSyncFileOperation::Import,
        path: path.to_string_lossy().into_owned(),
        imported,
        applied: report.applied,
        ignored: report.ignored,
        conflicts: report.conflicts.len(),
        ..CloudSyncFileReport::default()
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::storage::{AuthType, Server};

    use super::*;

    fn database(directory: &tempfile::TempDir, name: &str) -> Arc<Database> {
        Arc::new(Database::new_at(directory.path().join(name)).unwrap())
    }

    #[test]
    fn portable_file_round_trip_merges_workspace_records_without_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let source = database(&directory, "source.db");
        let target = database(&directory, "target.db");
        let mut server = Server {
            id: String::new(),
            name: "portable-production".to_string(),
            host: "portable.example.com".to_string(),
            port: 22,
            username: "deploy".to_string(),
            auth_type: AuthType::Password,
            credential_id: Some("device-only-secret".to_string()),
            group_id: None,
            tags: vec!["portable".to_string()],
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: None,
            agent_forwarding: false,
        };
        source.server_add(&mut server).unwrap();
        let path = directory.path().join("workspace.vibeshell-sync.json");

        let exported = export_to_path(&source, &path).unwrap();
        let imported = import_from_path(&target, &path).unwrap();

        assert_eq!(exported.exported, 1);
        assert_eq!(imported.imported, 1);
        assert_eq!(imported.applied, 1);
        let restored = target.server_get(&server.id).unwrap().unwrap();
        assert_eq!(restored.host, "portable.example.com");
        assert!(restored.credential_id.is_none());
    }

    #[test]
    fn portable_file_rejects_unknown_versions_without_changing_the_database() {
        let directory = tempfile::tempdir().unwrap();
        let target = database(&directory, "target.db");
        let path = directory.path().join("future.json");
        fs::write(
            &path,
            r#"{"format":"vibeshell-portable-sync","version":99,"exportedAt":0,"batch":{"deviceId":"device","changes":[]}}"#,
        )
        .unwrap();

        assert!(import_from_path(&target, &path).is_err());
        assert!(target.server_list(None, None).unwrap().is_empty());
    }

    #[test]
    fn portable_file_rejects_tombstones_instead_of_deleting_local_records() {
        let directory = tempfile::tempdir().unwrap();
        let target = database(&directory, "target.db");
        let mut server = Server {
            id: String::new(),
            name: "must-remain".to_string(),
            host: "keep.example.com".to_string(),
            port: 22,
            username: "deploy".to_string(),
            auth_type: AuthType::Password,
            credential_id: None,
            group_id: None,
            tags: Vec::new(),
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: None,
            agent_forwarding: false,
        };
        target.server_add(&mut server).unwrap();
        let path = directory.path().join("deletion.json");
        let deletion = PortableSyncFile {
            format: PORTABLE_FILE_FORMAT.to_string(),
            version: PORTABLE_FILE_VERSION,
            exported_at: 0,
            batch: PendingSyncBatch {
                device_id: "import-device".to_string(),
                changes: vec![crate::storage::SyncChange {
                    schema_version: crate::storage::SYNC_CHANGE_SCHEMA_VERSION,
                    change_id: uuid::Uuid::new_v4().to_string(),
                    entity_kind: crate::storage::SyncEntityKind::Server,
                    entity_id: server.id.clone(),
                    revision: format!(
                        "{:020}:import-device",
                        chrono::Utc::now().timestamp_millis()
                    ),
                    deleted: true,
                    payload: None,
                }],
            },
        };
        fs::write(&path, serde_json::to_vec(&deletion).unwrap()).unwrap();

        assert!(import_from_path(&target, &path).is_err());
        assert!(target.server_get(&server.id).unwrap().is_some());
    }
}
