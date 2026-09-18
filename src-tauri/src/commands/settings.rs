//! Tauri commands for app settings persistence.
//!
//! The frontend (`settingsStore.ts`) persists the whole `AppSettings` blob on
//! every change and loads it on startup. The blob is stored verbatim as JSON
//! in the SQLite `settings` key-value table under one key; the frontend owns
//! the schema and merges it with defaults, so new settings stay
//! forward-compatible without backend changes.

use serde_json::Value;
use std::sync::Arc;
use tauri::State;

use crate::storage::Database;

const APP_SETTINGS_KEY: &str = "app_settings";

/// Load the persisted app settings, or `null` when nothing was saved yet.
#[tauri::command]
pub fn load_settings(db: State<'_, Arc<Database>>) -> Result<Option<Value>, String> {
    let stored = db
        .get_setting(APP_SETTINGS_KEY)
        .map_err(|e| format!("Failed to load settings: {}", e))?;

    match stored {
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| format!("Stored settings are not valid JSON: {}", e)),
        None => Ok(None),
    }
}

/// Persist the app settings blob (stored verbatim).
#[tauri::command]
pub fn save_settings(db: State<'_, Arc<Database>>, settings: Value) -> Result<(), String> {
    let json = serde_json::to_string(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    db.set_setting(APP_SETTINGS_KEY, &json)
        .map_err(|e| format!("Failed to save settings: {}", e))
}

#[cfg(test)]
mod tests {
    use super::APP_SETTINGS_KEY;

    #[test]
    fn app_settings_key_is_stable() {
        // The frontend has no migration path for the key; changing it would
        // silently drop every saved setting.
        assert_eq!(APP_SETTINGS_KEY, "app_settings");
    }
}
