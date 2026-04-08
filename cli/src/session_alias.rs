//! Persistent short-alias mapping for CLI SSH sessions.
//!
//! Maps auto-incrementing 3-digit IDs (001, 002, …) to the real session UUIDs
//! so users can reference sessions by memorable numeric aliases instead of long
//! UUIDs from the terminal.
//!
//! The alias store is a small JSON file kept in the platform's local data
//! directory and is shared across all CLI invocations.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Single alias entry linking a short ID to its backing session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasEntry {
    pub session_id: String,
    pub server_name: String,
    pub created_at: i64,
}

/// On-disk representation of the alias store.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AliasData {
    next_id: u32,
    aliases: BTreeMap<String, AliasEntry>,
}

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

fn store_path() -> PathBuf {
    // Allow override for testing
    if let Ok(override_path) = std::env::var("VIBESHELL_SESSION_STORE") {
        return PathBuf::from(override_path);
    }

    #[cfg(windows)]
    {
        let base = std::env::var("LOCALAPPDATA")
            .or_else(|_| std::env::var("APPDATA"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(base)
            .join("vibeshell")
            .join("cli-sessions.json")
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home)
            .join(".vibeshell")
            .join("cli-sessions.json")
    }
}

fn load() -> AliasData {
    let path = store_path();
    if !path.exists() {
        return AliasData::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AliasData::default(),
    }
}

fn save(data: &AliasData) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create session alias directory")?;
    }
    let json = serde_json::to_string_pretty(data).context("Failed to serialize session aliases")?;
    std::fs::write(&path, json).context("Failed to write session alias file")?;
    Ok(())
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Register a new session and return its 3-digit alias.
pub fn register(session_id: &str, server_name: &str) -> Result<String> {
    let mut data = load();

    if data.next_id == 0 {
        data.next_id = 1;
    }

    let alias = format!("{:03}", data.next_id);
    data.next_id += 1;

    data.aliases.insert(
        alias.clone(),
        AliasEntry {
            session_id: session_id.to_string(),
            server_name: server_name.to_string(),
            created_at: now_epoch(),
        },
    );

    save(&data)?;
    Ok(alias)
}

/// Resolve an alias (e.g. "001") to the backing session UUID.
///
/// Also accepts full UUIDs as a passthrough so callers don't need to
/// distinguish between alias and UUID input.
pub fn resolve(alias_or_id: &str) -> Option<String> {
    let data = load();

    if let Some(entry) = data.aliases.get(alias_or_id) {
        return Some(entry.session_id.clone());
    }

    // Treat anything that looks like a UUID as a passthrough
    if alias_or_id.contains('-') && alias_or_id.len() > 8 {
        return Some(alias_or_id.to_string());
    }

    None
}

/// Look up the full entry for an alias.
pub fn get_entry(alias: &str) -> Option<AliasEntry> {
    let data = load();
    data.aliases.get(alias).cloned()
}

/// Find the alias for a given session UUID (reverse lookup).
pub fn find_by_session_id(session_id: &str) -> Option<String> {
    let data = load();
    data.aliases
        .iter()
        .find(|(_, e)| e.session_id == session_id)
        .map(|(alias, _)| alias.clone())
}

/// Remove an alias by its short ID.
#[allow(dead_code)]
pub fn remove(alias: &str) -> Result<()> {
    let mut data = load();
    data.aliases.remove(alias);
    save(&data)
}

/// Remove any alias that points to the given session UUID.
pub fn remove_by_session_id(session_id: &str) -> Result<()> {
    let mut data = load();
    data.aliases
        .retain(|_, entry| entry.session_id != session_id);
    save(&data)
}

/// Return all current aliases, ordered by alias ID.
#[allow(dead_code)]
pub fn list_all() -> Vec<(String, AliasEntry)> {
    let data = load();
    data.aliases.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize tests that share the env-var–controlled store path
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_store<F: FnOnce()>(f: F) {
        let _guard = TEST_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("vibeshell_test_{}.json", std::process::id()));
        std::env::set_var("VIBESHELL_SESSION_STORE", &tmp);
        // Start each test with a clean file
        let _ = std::fs::remove_file(&tmp);
        f();
        let _ = std::fs::remove_file(&tmp);
        std::env::remove_var("VIBESHELL_SESSION_STORE");
    }

    #[test]
    fn register_produces_sequential_3_digit_ids() {
        with_temp_store(|| {
            let a1 = register("uuid-aaa", "server-a").unwrap();
            let a2 = register("uuid-bbb", "server-b").unwrap();
            let a3 = register("uuid-ccc", "server-c").unwrap();

            assert_eq!(a1, "001");
            assert_eq!(a2, "002");
            assert_eq!(a3, "003");
        });
    }

    #[test]
    fn resolve_returns_session_id_for_known_alias() {
        with_temp_store(|| {
            register("uuid-111", "web").unwrap();
            assert_eq!(resolve("001"), Some("uuid-111".to_string()));
        });
    }

    #[test]
    fn resolve_passes_through_uuid_like_strings() {
        with_temp_store(|| {
            let uuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
            assert_eq!(resolve(uuid), Some(uuid.to_string()));
        });
    }

    #[test]
    fn resolve_returns_none_for_unknown_alias() {
        with_temp_store(|| {
            assert_eq!(resolve("999"), None);
        });
    }

    #[test]
    fn get_entry_returns_full_metadata() {
        with_temp_store(|| {
            register("uuid-xyz", "prod-web").unwrap();
            let entry = get_entry("001").unwrap();
            assert_eq!(entry.session_id, "uuid-xyz");
            assert_eq!(entry.server_name, "prod-web");
            assert!(entry.created_at > 0);
        });
    }

    #[test]
    fn find_by_session_id_reverse_lookup() {
        with_temp_store(|| {
            register("uuid-abc", "staging").unwrap();
            register("uuid-def", "prod").unwrap();

            assert_eq!(find_by_session_id("uuid-def"), Some("002".to_string()));
            assert_eq!(find_by_session_id("no-such-id"), None);
        });
    }

    #[test]
    fn remove_deletes_alias() {
        with_temp_store(|| {
            register("uuid-1", "s1").unwrap();
            register("uuid-2", "s2").unwrap();

            remove("001").unwrap();

            assert_eq!(resolve("001"), None);
            assert_eq!(resolve("002"), Some("uuid-2".to_string()));
        });
    }

    #[test]
    fn remove_by_session_id_deletes_matching_alias() {
        with_temp_store(|| {
            register("uuid-aaa", "s1").unwrap();
            register("uuid-bbb", "s2").unwrap();

            remove_by_session_id("uuid-aaa").unwrap();

            assert_eq!(resolve("001"), None);
            assert_eq!(resolve("002"), Some("uuid-bbb".to_string()));
        });
    }

    #[test]
    fn list_all_returns_ordered_entries() {
        with_temp_store(|| {
            register("u1", "a").unwrap();
            register("u2", "b").unwrap();
            register("u3", "c").unwrap();

            let all = list_all();
            assert_eq!(all.len(), 3);
            assert_eq!(all[0].0, "001");
            assert_eq!(all[1].0, "002");
            assert_eq!(all[2].0, "003");
        });
    }

    #[test]
    fn ids_persist_across_load_cycles() {
        with_temp_store(|| {
            register("uuid-1", "s1").unwrap();
            register("uuid-2", "s2").unwrap();

            // The next register should continue from 003, not restart
            let a3 = register("uuid-3", "s3").unwrap();
            assert_eq!(a3, "003");
        });
    }

    #[test]
    fn empty_store_starts_from_001() {
        with_temp_store(|| {
            let data = load();
            assert_eq!(data.next_id, 0);
            assert!(data.aliases.is_empty());

            let alias = register("first", "server").unwrap();
            assert_eq!(alias, "001");
        });
    }
}
