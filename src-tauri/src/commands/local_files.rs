//! Standalone documents: no SSH connection or local shell is required.
use super::sftp::{get_mime_type, SftpFileContent};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{Emitter, Manager, State};

const TEXT_LIMIT: u64 = 4 * 1024 * 1024;
const BINARY_LIMIT: u64 = 64 * 1024 * 1024;
#[derive(Default)]
pub struct PendingOpenFiles(pub Mutex<Vec<String>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFileInfo {
    path: String,
    name: String,
    size: u64,
}

fn regular_file(path: &str) -> Result<PathBuf, String> {
    let requested = Path::new(path);
    if !requested.is_absolute() {
        return Err("An absolute file path is required".into());
    }
    let canonical = requested
        .canonicalize()
        .map_err(|e| format!("Cannot open {path}: {e}"))?;
    if !canonical.is_file() {
        return Err("Only regular files can be opened".into());
    }
    Ok(canonical)
}
fn file_info(path: &str) -> Result<LocalFileInfo, String> {
    let canonical = regular_file(path)?;
    let size = fs::metadata(&canonical).map_err(|e| e.to_string())?.len();
    Ok(LocalFileInfo {
        name: canonical
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: canonical.to_string_lossy().into_owned(),
        size,
    })
}

pub fn queue_open_files(app: &tauri::AppHandle, paths: impl IntoIterator<Item = PathBuf>) {
    let state = app.state::<PendingOpenFiles>();
    if let Ok(mut pending) = state.0.lock() {
        for path in paths.into_iter().take(128) {
            if let Ok(path) = path.canonicalize() {
                if path.is_file() && pending.len() < 128 {
                    let path = path.to_string_lossy().into_owned();
                    if !pending.contains(&path) {
                        pending.push(path);
                    }
                }
            }
        }
    }
    let _ = app.emit_to("main", "vibeshell://open-files-pending", ());
}

pub fn queue_file_arguments(
    app: &tauri::AppHandle,
    args: impl IntoIterator<Item = String>,
    cwd: &Path,
) {
    // Only explicit existing paths, never execute a command or follow a URL.
    queue_open_files(
        app,
        args.into_iter()
            .skip(1)
            .filter(|a| !a.starts_with('-'))
            .map(|a| cwd.join(a)),
    );
}

#[tauri::command]
pub fn take_pending_open_files(
    window: tauri::Window,
    state: State<'_, PendingOpenFiles>,
) -> Result<Vec<String>, String> {
    if window.label() != "main" {
        return Err("Only the main window may consume file-open requests".into());
    }
    let mut paths = state.0.lock().map_err(|e| e.to_string())?;
    Ok(std::mem::take(&mut *paths))
}

#[tauri::command]
pub async fn pick_local_files() -> Result<Vec<String>, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Err("Desktop file selection is unavailable on mobile".into())
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        Ok(rfd::AsyncFileDialog::new()
            .set_title("Open documents or images in VibeShell")
            .pick_files()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|file| file.path().to_string_lossy().into_owned())
            .collect())
    }
}

#[tauri::command]
pub async fn local_file_stat(path: String) -> Result<LocalFileInfo, String> {
    tauri::async_runtime::spawn_blocking(move || file_info(&path))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalReadRequest {
    path: String,
    as_binary: Option<bool>,
    max_size: Option<u64>,
}

fn read_local(request: LocalReadRequest) -> Result<SftpFileContent, String> {
    let path = regular_file(&request.path)?;
    let mut file = fs::File::open(&path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let binary = request.as_binary.unwrap_or(false);
    let hard_limit = if binary { BINARY_LIMIT } else { TEXT_LIMIT };
    let limit = request.max_size.unwrap_or(hard_limit).min(hard_limit);
    if binary && size > limit {
        return Err(format!(
            "File too large for preview ({size} bytes, limit {limit})"
        ));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let truncated = size > bytes.len() as u64;
    let content = if binary {
        STANDARD.encode(bytes)
    } else {
        if bytes.contains(&0) {
            return Err("This is a binary or UTF-16 file, not UTF-8 text".into());
        }
        // Truncation can bisect the final UTF-8 code point. Do not corrupt it or
        // silently replace malformed text and later save those replacements.
        if truncated {
            if let Err(error) = std::str::from_utf8(&bytes) {
                if error.error_len().is_none() {
                    bytes.truncate(error.valid_up_to());
                }
            }
        }
        String::from_utf8(bytes).map_err(|_| {
            "Unsupported text encoding. Convert the file to UTF-8 before editing".to_string()
        })?
    };
    Ok(SftpFileContent {
        content,
        is_binary: binary,
        size,
        truncated,
        mime_type: get_mime_type(&path.to_string_lossy()),
    })
}

#[tauri::command]
pub async fn local_file_read(request: LocalReadRequest) -> Result<SftpFileContent, String> {
    tauri::async_runtime::spawn_blocking(move || read_local(request))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalWriteRequest {
    path: String,
    content: String,
    expected_content: String,
}
fn write_local(request: LocalWriteRequest) -> Result<(), String> {
    if request.content.len() as u64 > TEXT_LIMIT {
        return Err("Text exceeds the editor's 4 MiB save limit".into());
    }
    let path = regular_file(&request.path)?;
    let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
    if metadata.len() > TEXT_LIMIT
        || fs::read_to_string(&path).map_err(|e| e.to_string())? != request.expected_content
    {
        return Err("The file changed on disk. Reload or copy your edits before saving; the disk version was not overwritten".into());
    }
    if metadata.permissions().readonly() {
        return Err("The file is read-only".into());
    }
    // Replace atomically in the same directory; retain permissions. Refuse to
    // overwrite another process's staging file, even under name collisions.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = path.with_file_name(format!(".vibeshell-save-{}-{stamp}", std::process::id()));
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = (|| -> std::io::Result<()> {
        output.set_permissions(metadata.permissions())?;
        output.write_all(request.content.as_bytes())?;
        output.sync_all()?;
        drop(output);
        // Check again immediately before replace; best-effort external-change detection.
        if fs::read_to_string(&path)? != request.expected_content {
            return Err(std::io::Error::other("File changed while saving"));
        }
        fs::rename(&temporary, &path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn local_file_write(request: LocalWriteRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || write_local(request))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn export_theme_css(css: String) -> Result<Option<String>, String> {
    if css.len() > 2 * 1024 * 1024 {
        return Err("CSS export exceeds 2 MiB".into());
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Err("Desktop CSS export is unavailable on mobile".into())
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let file = rfd::AsyncFileDialog::new()
            .set_title("Export VibeShell CSS theme")
            .set_file_name("vibeshell-theme.css")
            .add_filter("CSS theme", &["css"])
            .save_file()
            .await;
        let Some(file) = file else {
            return Ok(None);
        };
        let path = file.path().to_path_buf();
        tauri::async_runtime::spawn_blocking(move || {
            fs::write(&path, css).map_err(|e| e.to_string())?;
            Ok(Some(path.to_string_lossy().into_owned()))
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn install_workspace_menu(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItemBuilder, Submenu};
    let open = MenuItemBuilder::with_id("vibeshell-open-files", "Open Files… / 打开文件")
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let recover = MenuItemBuilder::with_id(
        "vibeshell-recover-theme",
        "Disable Custom CSS / 停用自定义 CSS",
    )
    .accelerator("CmdOrCtrl+Shift+F12")
    .build(app)?;
    let submenu = Submenu::with_items(app, "Workspace", true, &[&open, &recover])?;
    let menu = Menu::default(app.handle())?;
    menu.append(&submenu)?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| match event.id().as_ref() {
        "vibeshell-open-files" => {
            let _ = app.emit_to("main", "vibeshell://choose-local-files", ());
        }
        "vibeshell-recover-theme" => {
            let _ = app.emit("vibeshell://disable-custom-css", ());
        }
        _ => {}
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_utf8_and_binary_without_a_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("文档.md");
        fs::write(&path, "# Hello\r\n你好").unwrap();
        let read = read_local(LocalReadRequest {
            path: path.to_string_lossy().into(),
            as_binary: None,
            max_size: None,
        })
        .unwrap();
        assert_eq!(read.content, "# Hello\r\n你好");
        assert!(!read.is_binary);
        let binary = read_local(LocalReadRequest {
            path: path.to_string_lossy().into(),
            as_binary: Some(true),
            max_size: None,
        })
        .unwrap();
        assert_eq!(
            STANDARD.decode(binary.content).unwrap(),
            fs::read(&path).unwrap()
        );
    }
    #[test]
    fn saves_atomically_and_refuses_stale_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        fs::write(&path, "old").unwrap();
        let request = |text: &str| LocalWriteRequest {
            path: path.to_string_lossy().into(),
            expected_content: "old".into(),
            content: text.into(),
        };
        write_local(request("new")).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(write_local(request("stale")).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "new");
    }
    #[test]
    fn rejects_directories_relative_paths_and_invalid_utf8() {
        let dir = tempfile::tempdir().unwrap();
        assert!(regular_file("relative.txt").is_err());
        assert!(regular_file(&dir.path().to_string_lossy()).is_err());
        let path = dir.path().join("bad.txt");
        fs::write(&path, [0xff, 0xfe, 0]).unwrap();
        assert!(read_local(LocalReadRequest {
            path: path.to_string_lossy().into(),
            as_binary: None,
            max_size: None
        })
        .is_err());
    }
}
