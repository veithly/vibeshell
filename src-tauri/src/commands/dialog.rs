//! Dialog commands for file picking operations.
//!
//! This module provides Tauri commands for file system dialogs,
//! specifically for picking SSH key files.

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use rfd::FileDialog;
use std::{fs, path::Path};

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
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return Err("SSH key file selection is unavailable on mobile".to_string());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // OpenSSH private keys commonly use names like `id_ed25519` with no extension,
        // while rfd filters are extension-based and can hide them.
        let file = FileDialog::new()
            .set_title("Select SSH Private Key")
            .pick_file();

        Ok(file.map(|f| f.to_string_lossy().to_string()))
    }
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
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return Err("Upload file selection is unavailable on mobile".to_string());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let file = FileDialog::new()
            .add_filter("All Files", &["*"])
            .set_title("Select File to Upload")
            .pick_file();

        Ok(file.map(|f| f.to_string_lossy().to_string()))
    }
}

/// Opens a file dialog that accepts one or more files for batch upload.
#[tauri::command]
pub async fn pick_files_for_upload() -> Result<Vec<String>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return Err("Upload file selection is unavailable on mobile".to_string());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let files = FileDialog::new()
            .add_filter("All Files", &["*"])
            .set_title("Select Files to Upload")
            .pick_files();

        Ok(files
            .unwrap_or_default()
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect())
    }
}

/// Opens a directory dialog to pick a folder for recursive upload or sync.
#[tauri::command]
pub async fn pick_directory_for_upload() -> Result<Option<String>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return Err("Upload directory selection is unavailable on mobile".to_string());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let folder = FileDialog::new()
            .set_title("Select Directory to Upload")
            .pick_folder();

        Ok(folder.map(|f| f.to_string_lossy().to_string()))
    }
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
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return Err("Download directory selection is unavailable on mobile".to_string());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let folder = FileDialog::new()
            .set_title("Select Download Location")
            .pick_folder();

        Ok(folder.map(|f| f.to_string_lossy().to_string()))
    }
}

/// Opens a directory dialog to select a local coding workspace.
#[tauri::command]
pub async fn pick_workspace_directory() -> Result<Option<String>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return Err("Coding workspace selection is unavailable on mobile".to_string());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let folder = FileDialog::new()
            .set_title("Select Coding Workspace")
            .pick_folder();

        Ok(folder.map(|path| path.to_string_lossy().to_string()))
    }
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
    read_ssh_key_file_content(path)
}

fn read_ssh_key_file_content(path: impl AsRef<Path>) -> Result<String, String> {
    fs::read_to_string(path.as_ref()).map_err(|e| format!("Failed to read SSH key file: {}", e))
}

#[cfg(test)]
mod tests {
    use super::read_ssh_key_file_content;

    #[test]
    fn reads_extensionless_private_key_files() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let key_path = temp_dir.path().join("id_ed25519");
        let key_content =
            "-----BEGIN OPENSSH PRIVATE KEY-----\nkey\n-----END OPENSSH PRIVATE KEY-----\n";

        std::fs::write(&key_path, key_content).expect("extensionless key should be written");

        assert_eq!(
            read_ssh_key_file_content(&key_path).expect("extensionless key should be read"),
            key_content
        );
    }
}
