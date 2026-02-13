use std::sync::Arc;
use tauri::State;
use serde::Deserialize;

use crate::storage::{Database, CommandSnippet};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetInput {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetUpdateInput {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// List all command snippets, optionally filtered by category
#[tauri::command]
pub fn snippet_list(
    db: State<'_, Arc<Database>>,
    category: Option<String>,
) -> Result<Vec<CommandSnippet>, String> {
    db.snippet_list(category.as_deref())
        .map_err(|e| format!("Failed to list snippets: {}", e))
}

/// Add a new command snippet
#[tauri::command]
pub fn snippet_add(
    db: State<'_, Arc<Database>>,
    input: SnippetInput,
) -> Result<CommandSnippet, String> {
    let mut snippet = CommandSnippet {
        id: String::new(),
        name: input.name,
        command: input.command,
        category: input.category.unwrap_or_default(),
        description: input.description.unwrap_or_default(),
        tags: input.tags.unwrap_or_default(),
        created_at: 0,
        updated_at: 0,
    };

    db.snippet_add(&mut snippet)
        .map_err(|e| format!("Failed to add snippet: {}", e))?;

    Ok(snippet)
}

/// Update an existing command snippet
#[tauri::command]
pub fn snippet_update(
    db: State<'_, Arc<Database>>,
    input: SnippetUpdateInput,
) -> Result<(), String> {
    // Get existing snippet to merge with updates
    let snippets = db.snippet_list(None)
        .map_err(|e| format!("Failed to get snippets: {}", e))?;

    let existing = snippets.iter()
        .find(|s| s.id == input.id)
        .ok_or_else(|| "Snippet not found".to_string())?;

    let updated = CommandSnippet {
        id: input.id,
        name: input.name.unwrap_or_else(|| existing.name.clone()),
        command: input.command.unwrap_or_else(|| existing.command.clone()),
        category: input.category.unwrap_or_else(|| existing.category.clone()),
        description: input.description.unwrap_or_else(|| existing.description.clone()),
        tags: input.tags.unwrap_or_else(|| existing.tags.clone()),
        created_at: existing.created_at,
        updated_at: 0, // will be set by db
    };

    db.snippet_update(&updated)
        .map_err(|e| format!("Failed to update snippet: {}", e))
}

/// Delete a command snippet
#[tauri::command]
pub fn snippet_delete(
    db: State<'_, Arc<Database>>,
    id: String,
) -> Result<(), String> {
    db.snippet_delete(&id)
        .map_err(|e| format!("Failed to delete snippet: {}", e))
}

/// Search snippets by name, command, or tags
#[tauri::command]
pub fn snippet_search(
    db: State<'_, Arc<Database>>,
    query: String,
) -> Result<Vec<CommandSnippet>, String> {
    db.snippet_search(&query)
        .map_err(|e| format!("Failed to search snippets: {}", e))
}
