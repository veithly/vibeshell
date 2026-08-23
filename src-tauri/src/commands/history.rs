use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

use crate::storage::{CommandHistoryEntry, Database};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryListInput {
    pub server_id: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub favorites_only: bool,
    #[serde(default = "default_history_limit")]
    pub limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRecordInput {
    pub server_id: String,
    pub command: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFavoriteInput {
    pub id: String,
    pub is_favorite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryClearInput {
    pub server_id: String,
    #[serde(default)]
    pub include_favorites: bool,
}

fn default_history_limit() -> u32 {
    200
}

#[tauri::command]
pub fn history_list(
    db: State<'_, Arc<Database>>,
    input: HistoryListInput,
) -> Result<Vec<CommandHistoryEntry>, String> {
    db.history_list(
        &input.server_id,
        input.query.as_deref(),
        input.favorites_only,
        input.limit,
    )
    .map_err(|error| format!("Failed to list command history: {}", error))
}

#[tauri::command]
pub fn history_record(
    db: State<'_, Arc<Database>>,
    input: HistoryRecordInput,
) -> Result<CommandHistoryEntry, String> {
    db.history_record(&input.server_id, &input.command)
        .map_err(|error| format!("Failed to record command history: {}", error))
}

#[tauri::command]
pub fn history_set_favorite(
    db: State<'_, Arc<Database>>,
    input: HistoryFavoriteInput,
) -> Result<(), String> {
    db.history_set_favorite(&input.id, input.is_favorite)
        .map_err(|error| format!("Failed to update command favorite: {}", error))
}

#[tauri::command]
pub fn history_delete(db: State<'_, Arc<Database>>, id: String) -> Result<(), String> {
    db.history_delete(&id)
        .map_err(|error| format!("Failed to delete command history: {}", error))
}

#[tauri::command]
pub fn history_clear(db: State<'_, Arc<Database>>, input: HistoryClearInput) -> Result<(), String> {
    db.history_clear(&input.server_id, input.include_favorites)
        .map_err(|error| format!("Failed to clear command history: {}", error))
}
