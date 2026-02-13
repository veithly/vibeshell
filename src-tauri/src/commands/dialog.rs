//! Dialog commands for file picking operations.
//!
//! This module provides Tauri commands for file system dialogs,
//! specifically for picking SSH key files.

use std::fs;
use tauri_plugin_dialog::DialogExt;

/// Opens a file dialog to pick an SSH private key file.
///
/// # Arguments
///
/// * `app` - The Tauri application handle
///
/// # Returns
///
/// Returns `Ok(Some(path))` if a file was selected, `Ok(None)` if cancelled,
/// or an error if the dialog failed to open.
#[tauri::command]
pub async fn pick_ssh_key_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let file = app
        .dialog()
        .file()
        .add_filter("SSH Keys", &["pem", "key", "pub", "ppk"])
        .add_filter("All Files", &["*"])
        .set_title("Select SSH Private Key")
        .blocking_pick_file();

    Ok(file.map(|f| f.to_string()))
}

/// Opens a file dialog to pick any file for upload.
///
/// # Arguments
///
/// * `app` - The Tauri application handle
///
/// # Returns
///
/// Returns `Ok(Some(path))` if a file was selected, `Ok(None)` if cancelled.
#[tauri::command]
pub async fn pick_file_for_upload(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let file = app
        .dialog()
        .file()
        .add_filter("All Files", &["*"])
        .set_title("Select File to Upload")
        .blocking_pick_file();

    Ok(file.map(|f| f.to_string()))
}

/// Opens a directory dialog to pick a download location.
///
/// # Arguments
///
/// * `app` - The Tauri application handle
///
/// # Returns
///
/// Returns `Ok(Some(path))` if a directory was selected, `Ok(None)` if cancelled.
#[tauri::command]
pub async fn pick_download_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let folder = app
        .dialog()
        .file()
        .set_title("Select Download Location")
        .blocking_pick_folder();

    Ok(folder.map(|f| f.to_string()))
}

/// Reads the contents of an SSH key file.
///
/// # Arguments
///
/// * `path` - The path to the SSH key file
///
/// # Returns
///
/// Returns the file contents as a string, or an error if the file cannot be read.
#[tauri::command]
pub async fn read_ssh_key_file(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| format!("Failed to read SSH key file: {}", e))
}
