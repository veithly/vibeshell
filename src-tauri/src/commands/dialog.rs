//! Dialog commands for file picking operations.
//!
//! This module provides Tauri commands for file system dialogs,
//! specifically for picking SSH key files.

use rfd::FileDialog;
use std::fs;

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
pub async fn pick_ssh_key_file() -> Result<Option<String>, String> {
    let file = FileDialog::new()
        .add_filter("SSH Keys", &["pem", "key", "pub", "ppk"])
        .add_filter("All Files", &["*"])
        .set_title("Select SSH Private Key")
        .pick_file();

    Ok(file.map(|f| f.to_string_lossy().to_string()))
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
pub async fn pick_file_for_upload() -> Result<Option<String>, String> {
    let file = FileDialog::new()
        .add_filter("All Files", &["*"])
        .set_title("Select File to Upload")
        .pick_file();

    Ok(file.map(|f| f.to_string_lossy().to_string()))
}

/// Opens a directory dialog to pick a folder for recursive upload or sync.
#[tauri::command]
pub async fn pick_directory_for_upload() -> Result<Option<String>, String> {
    let folder = FileDialog::new()
        .set_title("Select Directory to Upload")
        .pick_folder();

    Ok(folder.map(|f| f.to_string_lossy().to_string()))
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
pub async fn pick_download_directory() -> Result<Option<String>, String> {
    let folder = FileDialog::new()
        .set_title("Select Download Location")
        .pick_folder();

    Ok(folder.map(|f| f.to_string_lossy().to_string()))
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
