use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use crate::ssh_import::{
    self, DetectedImportSource, ImportPreview, ImportReport, ImportSourceKind,
};
use crate::storage::Database;

#[tauri::command]
pub fn detect_ssh_import_sources() -> Vec<DetectedImportSource> {
    ssh_import::detect_import_sources()
}

#[tauri::command]
pub fn preview_ssh_import(
    source: ImportSourceKind,
    path: Option<String>,
) -> Result<ImportPreview, String> {
    ssh_import::preview_import(source, path.map(PathBuf::from)).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn import_ssh_profiles(
    database: State<'_, Arc<Database>>,
    source: ImportSourceKind,
    path: Option<String>,
) -> Result<ImportReport, String> {
    let preview = ssh_import::preview_import(source, path.map(PathBuf::from))
        .map_err(|error| error.to_string())?;
    ssh_import::import_preview(&database, &preview).map_err(|error| error.to_string())
}
