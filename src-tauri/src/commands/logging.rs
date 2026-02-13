use std::sync::Arc;
use tauri::State;

use crate::logging::SessionLogger;
use crate::session::SessionManager;
use crate::storage::{Database, Recording};

/// Start recording a session's terminal output
#[tauri::command]
pub async fn start_recording(
    logger: State<'_, Arc<SessionLogger>>,
    manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    server_id: String,
) -> Result<String, String> {
    let session = manager.get(&session_id).await
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    logger.start_recording(session, &server_id).await
        .map_err(|e| format!("Failed to start recording: {}", e))
}

/// Stop an active recording
#[tauri::command]
pub async fn stop_recording(
    logger: State<'_, Arc<SessionLogger>>,
    recording_id: String,
) -> Result<(), String> {
    logger.stop_recording(&recording_id).await
        .map_err(|e| format!("Failed to stop recording: {}", e))
}

/// List recordings, optionally filtered by server_id
#[tauri::command]
pub fn list_recordings(
    db: State<'_, Arc<Database>>,
    server_id: Option<String>,
) -> Result<Vec<Recording>, String> {
    db.recording_list(server_id.as_deref())
        .map_err(|e| format!("Failed to list recordings: {}", e))
}

/// Check if a session is currently being recorded
#[tauri::command]
pub async fn is_session_recording(
    logger: State<'_, Arc<SessionLogger>>,
    session_id: String,
) -> Result<bool, String> {
    Ok(logger.is_recording(&session_id).await)
}

/// Get the recording ID for a session
#[tauri::command]
pub async fn get_session_recording_id(
    logger: State<'_, Arc<SessionLogger>>,
    session_id: String,
) -> Result<Option<String>, String> {
    Ok(logger.get_recording_id(&session_id).await)
}

/// Delete a recording
#[tauri::command]
pub fn delete_recording(
    db: State<'_, Arc<Database>>,
    recording_id: String,
) -> Result<(), String> {
    // Get recording to find file path
    if let Ok(Some(recording)) = db.recording_get(&recording_id) {
        // Try to delete the file
        let _ = std::fs::remove_file(&recording.file_path);
    }
    db.recording_delete(&recording_id)
        .map_err(|e| format!("Failed to delete recording: {}", e))
}

/// Read the content of a recording file
#[tauri::command]
pub fn get_recording_content(
    db: State<'_, Arc<Database>>,
    recording_id: String,
) -> Result<String, String> {
    let recording = db.recording_get(&recording_id)
        .map_err(|e| format!("Failed to get recording: {}", e))?
        .ok_or_else(|| "Recording not found".to_string())?;

    std::fs::read_to_string(&recording.file_path)
        .map_err(|e| format!("Failed to read recording file: {}", e))
}
