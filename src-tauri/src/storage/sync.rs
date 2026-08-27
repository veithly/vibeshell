use std::cmp::Ordering;
use std::collections::HashSet;

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::database::{Database, Group};
use super::models::{AuthType, CommandSnippet, PluginInstallation, Server};

pub const SYNC_CHANGE_SCHEMA_VERSION: u32 = 1;

const DEVICE_ID_KEY: &str = "device_id";
const LAST_REVISION_MILLIS_KEY: &str = "last_revision_millis";
const REMOTE_CURSOR_KEY: &str = "remote_cursor";
const ACTIVE_VAULT_ID_KEY: &str = "active_vault_id";
const MAX_DEVICE_ID_LEN: usize = 128;
const MAX_REMOTE_CLOCK_SKEW_MILLIS: i64 = 24 * 60 * 60 * 1_000;

/// Stable entity identifiers used by the provider-independent sync protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncEntityKind {
    Server,
    Group,
    CommandSnippet,
    PluginInstallation,
}

impl SyncEntityKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Group => "group",
            Self::CommandSnippet => "command_snippet",
            Self::PluginInstallation => "plugin_installation",
        }
    }

    fn from_db(value: &str) -> Result<Self> {
        match value {
            "server" => Ok(Self::Server),
            "group" => Ok(Self::Group),
            "command_snippet" => Ok(Self::CommandSnippet),
            "plugin_installation" => Ok(Self::PluginInstallation),
            _ => bail!("Unknown sync entity kind: {value}"),
        }
    }
}

/// An opaque provider-facing mutation. Payloads contain portable domain data only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncChange {
    pub schema_version: u32,
    pub change_id: String,
    pub entity_kind: SyncEntityKind,
    pub entity_id: String,
    /// Lexicographically sortable hybrid timestamp: `<millis>:<device-id>`.
    pub revision: String,
    pub deleted: bool,
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSyncBatch {
    pub device_id: String,
    pub changes: Vec<SyncChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSyncUpload {
    pub vault_id: String,
    pub envelope_id: String,
    pub ciphertext: String,
    pub change_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    LocalKept,
    RemoteApplied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub entity_kind: SyncEntityKind,
    pub entity_id: String,
    pub local_revision: String,
    pub remote_revision: String,
    pub resolution: ConflictResolution,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteApplyReport {
    pub applied: usize,
    pub ignored: usize,
    pub conflicts: Vec<SyncConflict>,
}

#[derive(Debug, Clone)]
struct PreparedChange {
    change: SyncChange,
    content_hash: String,
}

#[derive(Debug, Clone)]
struct EntityState {
    revision: String,
    content_hash: String,
    deleted: bool,
}

enum NameCollisionDecision {
    NoCollision,
    IncomingWins {
        loser_id: String,
        loser_state: EntityState,
    },
    ExistingWins {
        winner_id: String,
        winner_state: EntityState,
    },
}

/// The single storage seam used by a future CloudSync module.
///
/// Network transport, encryption, retries, and scheduling stay outside this module. This
/// interface owns transactional change capture, acknowledgement, conflict ordering, and
/// applying provider-independent remote batches.
pub struct CloudSyncStorage<'a> {
    database: &'a Database,
}

impl Database {
    pub fn cloud_sync(&self) -> CloudSyncStorage<'_> {
        CloudSyncStorage { database: self }
    }
}

impl CloudSyncStorage<'_> {
    pub fn export_pending_changes(&self, limit: usize) -> Result<PendingSyncBatch> {
        let conn = self.database.conn.lock().unwrap();
        let device_id = metadata_get(&conn, DEVICE_ID_KEY)?
            .ok_or_else(|| anyhow!("Sync device ID is not initialized"))?;

        if limit == 0 {
            return Ok(PendingSyncBatch {
                device_id,
                changes: Vec::new(),
            });
        }

        let sql_limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut stmt = conn.prepare(
            r#"SELECT schema_version, change_id, entity_kind, entity_id, revision,
                      is_tombstone, payload
               FROM sync_outbox
               ORDER BY revision ASC, change_id ASC
               LIMIT ?1"#,
        )?;

        let rows = stmt.query_map([sql_limit], |row| {
            let entity_kind: String = row.get(2)?;
            let payload_json: Option<String> = row.get(6)?;
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, String>(1)?,
                entity_kind,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)? != 0,
                payload_json,
            ))
        })?;

        let mut changes = Vec::new();
        for row in rows {
            let (schema_version, change_id, entity_kind, entity_id, revision, deleted, payload) =
                row?;
            changes.push(SyncChange {
                schema_version,
                change_id,
                entity_kind: SyncEntityKind::from_db(&entity_kind)?,
                entity_id,
                revision,
                deleted,
                payload: payload
                    .map(|raw| serde_json::from_str(&raw))
                    .transpose()
                    .context("Invalid JSON payload in sync outbox")?,
            });
        }

        Ok(PendingSyncBatch { device_id, changes })
    }

    /// Export the current portable workspace state without changing the provider outbox.
    ///
    /// Tombstones are intentionally omitted: importing a file merges the records contained in
    /// that file and never deletes unrelated records already present on the destination device.
    pub fn export_snapshot(&self) -> Result<PendingSyncBatch> {
        let conn = self.database.conn.lock().unwrap();
        let device_id = metadata_get(&conn, DEVICE_ID_KEY)?
            .ok_or_else(|| anyhow!("Sync device ID is not initialized"))?;
        let states = {
            let mut statement = conn.prepare(
                r#"SELECT entity_kind, entity_id, revision
                   FROM sync_entity_state
                   WHERE is_tombstone = 0
                   ORDER BY entity_kind, entity_id"#,
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };

        let mut changes = Vec::with_capacity(states.len());
        for (kind, entity_id, revision) in states {
            let entity_kind = SyncEntityKind::from_db(&kind)?;
            let payload =
                current_domain_payload(&conn, entity_kind, &entity_id)?.ok_or_else(|| {
                    anyhow!("Sync state references a missing {kind} record {entity_id}")
                })?;
            changes.push(SyncChange {
                schema_version: SYNC_CHANGE_SCHEMA_VERSION,
                change_id: Uuid::new_v4().to_string(),
                entity_kind,
                entity_id,
                revision,
                deleted: false,
                payload: Some(payload),
            });
        }

        Ok(PendingSyncBatch { device_id, changes })
    }

    /// Merge a portable file snapshot without advancing any network provider cursor.
    pub fn apply_imported_changes(&self, changes: &[SyncChange]) -> Result<RemoteApplyReport> {
        let ordered = prepare_remote_batch(changes)?;
        let mut conn = self.database.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let report = apply_prepared_batch(&tx, &ordered)?;
        tx.commit()?;
        Ok(report)
    }

    pub fn acknowledge_changes(&self, change_ids: &[String]) -> Result<usize> {
        if change_ids.is_empty() {
            return Ok(0);
        }

        let mut conn = self.database.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut acknowledged = 0;
        {
            let mut stmt = tx.prepare("DELETE FROM sync_outbox WHERE change_id = ?1")?;
            for change_id in change_ids {
                acknowledged += stmt.execute([change_id])?;
            }
        }
        tx.commit()?;
        Ok(acknowledged)
    }

    pub fn pending_change_count(&self) -> Result<usize> {
        let conn = self.database.conn.lock().unwrap();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0))?;
        usize::try_from(count).context("Sync outbox count is outside the supported range")
    }

    pub fn active_vault_id(&self) -> Result<Option<String>> {
        let conn = self.database.conn.lock().unwrap();
        metadata_get(&conn, ACTIVE_VAULT_ID_KEY)
    }

    pub fn activate_vault(&self, vault_id: &str) -> Result<bool> {
        validate_vault_id(vault_id)?;
        let mut conn = self.database.conn.lock().unwrap();
        let tx = conn.transaction()?;
        if metadata_get(&tx, ACTIVE_VAULT_ID_KEY)?.as_deref() == Some(vault_id) {
            tx.commit()?;
            return Ok(false);
        }

        rebuild_full_outbox(&tx)?;
        metadata_set(&tx, ACTIVE_VAULT_ID_KEY, vault_id)?;
        tx.commit()?;
        Ok(true)
    }

    pub fn load_pending_upload(&self, vault_id: &str) -> Result<Option<PendingSyncUpload>> {
        validate_vault_id(vault_id)?;
        let conn = self.database.conn.lock().unwrap();
        pending_upload_get(&conn, vault_id)
    }

    pub fn save_pending_upload(&self, upload: &PendingSyncUpload) -> Result<()> {
        validate_pending_upload(upload)?;
        let conn = self.database.conn.lock().unwrap();
        if let Some(existing) = pending_upload_get(&conn, &upload.vault_id)? {
            if existing == *upload {
                return Ok(());
            }
            bail!(
                "Vault {} already has a different pending sync upload",
                upload.vault_id
            );
        }

        conn.execute(
            r#"INSERT INTO sync_pending_uploads
               (vault_id, envelope_id, ciphertext, change_ids_json)
               VALUES (?1, ?2, ?3, ?4)"#,
            params![
                upload.vault_id,
                upload.envelope_id,
                upload.ciphertext,
                serde_json::to_string(&upload.change_ids)?,
            ],
        )?;
        Ok(())
    }

    pub fn acknowledge_pending_upload(&self, vault_id: &str, envelope_id: &str) -> Result<usize> {
        validate_vault_id(vault_id)?;
        validate_opaque_sync_id("sync envelope ID", envelope_id)?;
        let mut conn = self.database.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let Some(upload) = pending_upload_get(&tx, vault_id)? else {
            tx.commit()?;
            return Ok(0);
        };
        if upload.envelope_id != envelope_id {
            bail!("Pending sync upload envelope ID does not match");
        }

        let mut acknowledged = 0;
        {
            let mut statement = tx.prepare("DELETE FROM sync_outbox WHERE change_id = ?1")?;
            for change_id in &upload.change_ids {
                acknowledged += statement.execute([change_id])?;
            }
        }
        tx.execute(
            "DELETE FROM sync_pending_uploads WHERE vault_id = ?1 AND envelope_id = ?2",
            params![vault_id, envelope_id],
        )?;
        tx.commit()?;
        Ok(acknowledged)
    }

    /// Return the last opaque cursor committed with a remote batch.
    pub fn current_remote_cursor(&self) -> Result<Option<String>> {
        let conn = self.database.conn.lock().unwrap();
        metadata_get(&conn, REMOTE_CURSOR_KEY)
    }

    pub fn current_remote_cursor_for_vault(&self, vault_id: &str) -> Result<Option<String>> {
        validate_vault_id(vault_id)?;
        let conn = self.database.conn.lock().unwrap();
        vault_cursor_get(&conn, vault_id)
    }

    /// Atomically apply a remote batch and advance its opaque provider cursor.
    ///
    /// The cursor is never interpreted by storage. Any validation, conflict, or SQLite error
    /// rolls back both the domain changes and the cursor update.
    pub fn apply_remote_batch(
        &self,
        cursor: &str,
        changes: &[SyncChange],
    ) -> Result<RemoteApplyReport> {
        let ordered = prepare_remote_batch(changes)?;
        let mut conn = self.database.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let report = apply_prepared_batch(&tx, &ordered)?;
        metadata_set(&tx, REMOTE_CURSOR_KEY, cursor)?;
        tx.commit()?;
        Ok(report)
    }

    pub fn apply_remote_batch_for_vault(
        &self,
        vault_id: &str,
        cursor: &str,
        changes: &[SyncChange],
    ) -> Result<RemoteApplyReport> {
        validate_vault_id(vault_id)?;
        let ordered = prepare_remote_batch(changes)?;
        let mut conn = self.database.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let report = apply_prepared_batch(&tx, &ordered)?;
        vault_cursor_set(&tx, vault_id, cursor)?;
        tx.commit()?;
        Ok(report)
    }
}

fn prepare_remote_batch(changes: &[SyncChange]) -> Result<Vec<PreparedChange>> {
    let now = Utc::now().timestamp_millis();
    let mut ordered = changes
        .iter()
        .map(|change| prepare_change(change, now))
        .collect::<Result<Vec<_>>>()?;
    ordered.sort_by(|left, right| {
        left.change
            .revision
            .cmp(&right.change.revision)
            .then_with(|| left.change.change_id.cmp(&right.change.change_id))
    });
    Ok(ordered)
}

fn apply_prepared_batch(
    conn: &Connection,
    ordered: &[PreparedChange],
) -> Result<RemoteApplyReport> {
    let mut report = RemoteApplyReport::default();
    for prepared in ordered {
        apply_prepared_change(conn, prepared, &mut report)?;
    }
    Ok(report)
}

fn apply_prepared_change(
    conn: &Connection,
    prepared: &PreparedChange,
    report: &mut RemoteApplyReport,
) -> Result<()> {
    let name_collision = resolve_name_collision(conn, prepared)?;
    let effective_prepared = match &name_collision {
        NameCollisionDecision::ExistingWins { .. } => conflict_named_change(prepared)?,
        _ => prepared.clone(),
    };
    let change = &effective_prepared.change;
    observe_revision(conn, &change.revision)?;

    let current_state = entity_state(conn, change.entity_kind, &change.entity_id)?;
    let current_has_pending = has_pending_change(conn, change.entity_kind, &change.entity_id)?;

    if let Some(state) = &current_state {
        match change.revision.cmp(&state.revision) {
            Ordering::Less => {
                report.ignored += 1;
                if current_has_pending {
                    report.conflicts.push(SyncConflict {
                        entity_kind: change.entity_kind,
                        entity_id: change.entity_id.clone(),
                        local_revision: state.revision.clone(),
                        remote_revision: change.revision.clone(),
                        resolution: ConflictResolution::LocalKept,
                    });
                }
                return Ok(());
            }
            Ordering::Equal => {
                if effective_prepared.content_hash != state.content_hash {
                    bail!(
                        "Equal sync revision has different content for {}:{}",
                        change.entity_kind.as_str(),
                        change.entity_id
                    );
                }
                report.ignored += 1;
                return Ok(());
            }
            Ordering::Greater => {}
        }
    }

    match name_collision {
        NameCollisionDecision::NoCollision => {
            if current_has_pending {
                report.conflicts.push(SyncConflict {
                    entity_kind: change.entity_kind,
                    entity_id: change.entity_id.clone(),
                    local_revision: current_state
                        .as_ref()
                        .map(|state| state.revision.clone())
                        .unwrap_or_default(),
                    remote_revision: change.revision.clone(),
                    resolution: ConflictResolution::RemoteApplied,
                });
            }
        }
        NameCollisionDecision::IncomingWins {
            loser_id,
            loser_state,
        } => {
            report.conflicts.push(SyncConflict {
                entity_kind: change.entity_kind,
                entity_id: loser_id.clone(),
                local_revision: loser_state.revision.clone(),
                remote_revision: change.revision.clone(),
                resolution: ConflictResolution::RemoteApplied,
            });
            if current_has_pending {
                report.conflicts.push(SyncConflict {
                    entity_kind: change.entity_kind,
                    entity_id: change.entity_id.clone(),
                    local_revision: current_state
                        .as_ref()
                        .map(|state| state.revision.clone())
                        .unwrap_or_default(),
                    remote_revision: change.revision.clone(),
                    resolution: ConflictResolution::RemoteApplied,
                });
            }

            materialize_existing_conflict_name(conn, change.entity_kind, &loser_id, &loser_state)?;
        }
        NameCollisionDecision::ExistingWins {
            winner_id,
            winner_state,
        } => {
            move_existing_server_credential_for_conflict(conn, change)?;
            report.conflicts.push(SyncConflict {
                entity_kind: change.entity_kind,
                entity_id: winner_id,
                local_revision: winner_state.revision.clone(),
                remote_revision: change.revision.clone(),
                resolution: ConflictResolution::LocalKept,
            });
            if current_has_pending {
                report.conflicts.push(SyncConflict {
                    entity_kind: change.entity_kind,
                    entity_id: change.entity_id.clone(),
                    local_revision: current_state
                        .as_ref()
                        .map(|state| state.revision.clone())
                        .unwrap_or_default(),
                    remote_revision: winner_state.revision.clone(),
                    resolution: ConflictResolution::RemoteApplied,
                });
            }
        }
    }

    apply_remote_change(conn, change)?;
    upsert_entity_state(
        conn,
        change.entity_kind,
        &change.entity_id,
        &change.revision,
        change.deleted,
        &effective_prepared.content_hash,
    )?;
    remove_outbox_through(
        conn,
        change.entity_kind,
        &change.entity_id,
        &change.revision,
    )?;
    report.applied += 1;
    Ok(())
}

pub(super) fn initialize(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sync_metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sync_entity_state (
            entity_kind TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            revision TEXT NOT NULL,
            is_tombstone INTEGER NOT NULL DEFAULT 0 CHECK (is_tombstone IN (0, 1)),
            content_hash TEXT NOT NULL,
            PRIMARY KEY (entity_kind, entity_id)
        );

        CREATE TABLE IF NOT EXISTS sync_outbox (
            change_id TEXT PRIMARY KEY,
            schema_version INTEGER NOT NULL,
            entity_kind TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            revision TEXT NOT NULL,
            is_tombstone INTEGER NOT NULL DEFAULT 0 CHECK (is_tombstone IN (0, 1)),
            payload TEXT,
            created_at INTEGER NOT NULL,
            CHECK (
                (is_tombstone = 1 AND payload IS NULL) OR
                (is_tombstone = 0 AND payload IS NOT NULL)
            )
        );

        CREATE INDEX IF NOT EXISTS idx_sync_outbox_revision
            ON sync_outbox (revision, change_id);
        CREATE INDEX IF NOT EXISTS idx_sync_outbox_entity
            ON sync_outbox (entity_kind, entity_id, revision);

        CREATE TABLE IF NOT EXISTS sync_vault_state (
            vault_id TEXT PRIMARY KEY,
            remote_cursor TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sync_pending_uploads (
            vault_id TEXT PRIMARY KEY,
            envelope_id TEXT NOT NULL,
            ciphertext TEXT NOT NULL,
            change_ids_json TEXT NOT NULL
        );
        "#,
    )?;

    // Databases created by the initial sync foundation need the canonical hash column backfilled.
    // SQLite cannot add a NOT NULL column without a default to a populated table.
    let _ = conn.execute(
        "ALTER TABLE sync_entity_state ADD COLUMN content_hash TEXT",
        [],
    );

    let tx = conn.transaction()?;
    ensure_metadata(&tx)?;
    backfill_content_hashes(&tx)?;
    bootstrap_existing_rows(&tx)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn record_server_upsert(
    conn: &Connection,
    server: &Server,
    created_at: i64,
    updated_at: i64,
) -> Result<()> {
    let payload = ServerSyncPayload {
        name: server.name.clone(),
        host: server.host.clone(),
        port: server.port,
        username: server.username.clone(),
        auth_type: server.auth_type.clone(),
        group_id: server.group_id.clone(),
        tags: server.tags.clone(),
        created_at,
        updated_at,
        jump_host_id: server.jump_host_id.clone(),
        post_login_command: server.post_login_command.clone(),
        agent_forwarding: server.agent_forwarding,
    };
    record_local_upsert(
        conn,
        SyncEntityKind::Server,
        &server.id,
        serde_json::to_value(payload)?,
    )?;
    Ok(())
}

pub(super) fn record_group_upsert(conn: &Connection, group: &Group) -> Result<()> {
    let payload = GroupSyncPayload {
        name: group.name.clone(),
        parent_id: group.parent_id.clone(),
        color: group.color.clone(),
    };
    record_local_upsert(
        conn,
        SyncEntityKind::Group,
        &group.id,
        serde_json::to_value(payload)?,
    )?;
    Ok(())
}

pub(super) fn record_snippet_upsert(
    conn: &Connection,
    snippet: &CommandSnippet,
    created_at: i64,
    updated_at: i64,
) -> Result<()> {
    let payload = SnippetSyncPayload {
        name: snippet.name.clone(),
        command: snippet.command.clone(),
        category: snippet.category.clone(),
        description: snippet.description.clone(),
        tags: snippet.tags.clone(),
        created_at,
        updated_at,
    };
    record_local_upsert(
        conn,
        SyncEntityKind::CommandSnippet,
        &snippet.id,
        serde_json::to_value(payload)?,
    )?;
    Ok(())
}

pub(super) fn record_plugin_installation_upsert(
    conn: &Connection,
    installation: &PluginInstallation,
) -> Result<()> {
    let payload = PluginInstallationSyncPayload {
        plugin_id: installation.plugin_id.clone(),
        version: installation.version.clone(),
        source: installation.source.clone(),
        enabled: installation.enabled,
        manifest_json: if installation.source == "external" {
            Some(installation.manifest_json.clone())
        } else {
            None
        },
        settings_json: installation.settings_json.clone(),
        installed_at: installation.installed_at,
        updated_at: installation.updated_at,
    };
    record_local_upsert(
        conn,
        SyncEntityKind::PluginInstallation,
        &installation.plugin_id,
        serde_json::to_value(payload)?,
    )?;
    Ok(())
}

pub(super) fn record_local_delete(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
) -> Result<()> {
    record_local_change(conn, entity_kind, entity_id, true, None)?;
    Ok(())
}

pub(super) fn detach_group_references(
    conn: &Connection,
    group_id: &str,
    record_changes: bool,
) -> Result<()> {
    let child_group_ids =
        query_string_column(conn, "SELECT id FROM groups WHERE parent_id = ?1", group_id)?;
    let member_server_ids =
        query_string_column(conn, "SELECT id FROM servers WHERE group_id = ?1", group_id)?;

    conn.execute(
        "UPDATE groups SET parent_id = NULL WHERE parent_id = ?1",
        [group_id],
    )?;
    conn.execute(
        "UPDATE servers SET group_id = NULL WHERE group_id = ?1",
        [group_id],
    )?;

    if record_changes {
        for entity_id in child_group_ids {
            record_current_upsert(conn, SyncEntityKind::Group, &entity_id)?;
        }
        for entity_id in member_server_ids {
            record_current_upsert(conn, SyncEntityKind::Server, &entity_id)?;
        }
    }
    Ok(())
}

pub(super) fn detach_server_references(
    conn: &Connection,
    server_id: &str,
    record_changes: bool,
) -> Result<()> {
    let dependent_server_ids = query_string_column(
        conn,
        "SELECT id FROM servers WHERE jump_host_id = ?1",
        server_id,
    )?;
    conn.execute(
        "UPDATE servers SET jump_host_id = NULL WHERE jump_host_id = ?1",
        [server_id],
    )?;

    if record_changes {
        for entity_id in dependent_server_ids {
            record_current_upsert(conn, SyncEntityKind::Server, &entity_id)?;
        }
    }
    Ok(())
}

fn query_string_column(conn: &Connection, sql: &str, value: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare(sql)?;
    let values = statement
        .query_map([value], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(values)
}

pub(super) fn record_current_upsert(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
) -> Result<()> {
    let payload = current_domain_payload(conn, entity_kind, entity_id)?.ok_or_else(|| {
        anyhow!(
            "Cannot record detached {}:{} because it no longer exists",
            entity_kind.as_str(),
            entity_id
        )
    })?;
    record_local_upsert(conn, entity_kind, entity_id, payload)?;
    Ok(())
}

fn record_local_upsert(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
    payload: Value,
) -> Result<String> {
    record_local_change(conn, entity_kind, entity_id, false, Some(payload))
}

fn record_local_change(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
    deleted: bool,
    payload: Option<Value>,
) -> Result<String> {
    if entity_id.is_empty() {
        bail!("Cannot record a sync change with an empty entity ID");
    }

    let payload = normalize_payload(entity_kind, deleted, payload)?;
    let content_hash = canonical_content_hash(entity_kind, entity_id, deleted, payload.as_ref())?;
    let revision = next_revision(conn)?;
    let change_id = Uuid::new_v4().to_string();
    let payload_json = payload
        .map(|value| serde_json::to_string(&value))
        .transpose()?;

    upsert_entity_state(
        conn,
        entity_kind,
        entity_id,
        &revision,
        deleted,
        &content_hash,
    )?;
    conn.execute(
        r#"INSERT INTO sync_outbox
           (change_id, schema_version, entity_kind, entity_id, revision,
            is_tombstone, payload, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
        params![
            change_id,
            SYNC_CHANGE_SCHEMA_VERSION,
            entity_kind.as_str(),
            entity_id,
            revision,
            deleted as i32,
            payload_json,
            Utc::now().timestamp_millis(),
        ],
    )?;
    Ok(revision)
}

fn ensure_metadata(conn: &Connection) -> Result<()> {
    if metadata_get(conn, DEVICE_ID_KEY)?.is_none() {
        conn.execute(
            "INSERT INTO sync_metadata (key, value) VALUES (?1, ?2)",
            params![DEVICE_ID_KEY, Uuid::new_v4().to_string()],
        )?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO sync_metadata (key, value) VALUES (?1, '0')",
        [LAST_REVISION_MILLIS_KEY],
    )?;
    Ok(())
}

fn metadata_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM sync_metadata WHERE key = ?1",
        [key],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn metadata_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        r#"INSERT INTO sync_metadata (key, value) VALUES (?1, ?2)
           ON CONFLICT(key) DO UPDATE SET value = excluded.value"#,
        params![key, value],
    )?;
    Ok(())
}

fn vault_cursor_get(conn: &Connection, vault_id: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT remote_cursor FROM sync_vault_state WHERE vault_id = ?1",
        [vault_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn vault_cursor_set(conn: &Connection, vault_id: &str, cursor: &str) -> Result<()> {
    conn.execute(
        r#"INSERT INTO sync_vault_state (vault_id, remote_cursor) VALUES (?1, ?2)
           ON CONFLICT(vault_id) DO UPDATE SET remote_cursor = excluded.remote_cursor"#,
        params![vault_id, cursor],
    )?;
    Ok(())
}

fn pending_upload_get(conn: &Connection, vault_id: &str) -> Result<Option<PendingSyncUpload>> {
    conn.query_row(
        r#"SELECT envelope_id, ciphertext, change_ids_json
           FROM sync_pending_uploads WHERE vault_id = ?1"#,
        [vault_id],
        |row| {
            let change_ids_json: String = row.get(2)?;
            let change_ids = serde_json::from_str(&change_ids_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(PendingSyncUpload {
                vault_id: vault_id.to_string(),
                envelope_id: row.get(0)?,
                ciphertext: row.get(1)?,
                change_ids,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn validate_pending_upload(upload: &PendingSyncUpload) -> Result<()> {
    validate_vault_id(&upload.vault_id)?;
    validate_opaque_sync_id("sync envelope ID", &upload.envelope_id)?;
    if upload.ciphertext.is_empty() {
        bail!("Pending sync upload ciphertext cannot be empty");
    }
    if upload.change_ids.is_empty() {
        bail!("Pending sync upload must contain at least one change ID");
    }
    let mut unique = HashSet::with_capacity(upload.change_ids.len());
    for change_id in &upload.change_ids {
        validate_opaque_sync_id("sync change ID", change_id)?;
        if !unique.insert(change_id) {
            bail!("Pending sync upload contains duplicate change IDs");
        }
    }
    Ok(())
}

fn next_revision(conn: &Connection) -> Result<String> {
    let device_id = metadata_get(conn, DEVICE_ID_KEY)?
        .ok_or_else(|| anyhow!("Sync device ID is not initialized"))?;
    validate_device_id(&device_id)?;
    let previous = metadata_get(conn, LAST_REVISION_MILLIS_KEY)?
        .unwrap_or_else(|| "0".to_string())
        .parse::<i64>()
        .context("Invalid last sync revision timestamp")?;
    let next_logical = previous
        .checked_add(1)
        .ok_or_else(|| anyhow!("Sync revision clock is exhausted"))?;
    let logical_millis = Utc::now().timestamp_millis().max(next_logical);

    metadata_set(conn, LAST_REVISION_MILLIS_KEY, &logical_millis.to_string())?;

    Ok(format!("{logical_millis:020}:{device_id}"))
}

fn observe_revision(conn: &Connection, revision: &str) -> Result<()> {
    let observed_millis = revision_millis(revision)?;
    let previous = metadata_get(conn, LAST_REVISION_MILLIS_KEY)?
        .unwrap_or_else(|| "0".to_string())
        .parse::<i64>()
        .context("Invalid last sync revision timestamp")?;

    if observed_millis > previous {
        metadata_set(conn, LAST_REVISION_MILLIS_KEY, &observed_millis.to_string())?;
    }
    Ok(())
}

fn revision_millis(revision: &str) -> Result<i64> {
    let (millis, device_id) = revision
        .split_once(':')
        .ok_or_else(|| anyhow!("Invalid sync revision: {revision}"))?;
    if millis.len() != 20 || !millis.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("Invalid sync revision: {revision}");
    }
    validate_device_id(device_id)?;
    millis
        .parse::<i64>()
        .with_context(|| format!("Invalid sync revision: {revision}"))
}

fn validate_device_id(device_id: &str) -> Result<()> {
    if device_id.is_empty()
        || device_id.len() > MAX_DEVICE_ID_LEN
        || !device_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("Invalid sync revision device ID");
    }
    Ok(())
}

fn validate_vault_id(vault_id: &str) -> Result<()> {
    validate_opaque_sync_id("sync vault ID", vault_id)
}

fn validate_opaque_sync_id(label: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("Invalid {label}");
    }
    Ok(())
}

fn validate_remote_revision(revision: &str, now_millis: i64) -> Result<()> {
    let millis = revision_millis(revision)?;
    let latest_allowed = now_millis
        .checked_add(MAX_REMOTE_CLOCK_SKEW_MILLIS)
        .unwrap_or(i64::MAX);
    if millis > latest_allowed {
        bail!("Remote sync revision is implausibly far in the future");
    }
    Ok(())
}

fn upsert_entity_state(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
    revision: &str,
    deleted: bool,
    content_hash: &str,
) -> Result<()> {
    conn.execute(
        r#"INSERT INTO sync_entity_state
           (entity_kind, entity_id, revision, is_tombstone, content_hash)
           VALUES (?1, ?2, ?3, ?4, ?5)
           ON CONFLICT(entity_kind, entity_id) DO UPDATE SET
               revision = excluded.revision,
               is_tombstone = excluded.is_tombstone,
               content_hash = excluded.content_hash"#,
        params![
            entity_kind.as_str(),
            entity_id,
            revision,
            deleted as i32,
            content_hash
        ],
    )?;
    Ok(())
}

fn entity_state(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
) -> Result<Option<EntityState>> {
    conn.query_row(
        r#"SELECT revision, content_hash, is_tombstone FROM sync_entity_state
           WHERE entity_kind = ?1 AND entity_id = ?2"#,
        params![entity_kind.as_str(), entity_id],
        |row| {
            Ok(EntityState {
                revision: row.get(0)?,
                content_hash: row.get(1)?,
                deleted: row.get::<_, i64>(2)? != 0,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn has_pending_change(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
) -> Result<bool> {
    conn.query_row(
        r#"SELECT EXISTS(
               SELECT 1 FROM sync_outbox
               WHERE entity_kind = ?1 AND entity_id = ?2
           )"#,
        params![entity_kind.as_str(), entity_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn prepare_change(change: &SyncChange, now_millis: i64) -> Result<PreparedChange> {
    if change.schema_version != SYNC_CHANGE_SCHEMA_VERSION {
        bail!(
            "Unsupported sync change schema version: {}",
            change.schema_version
        );
    }
    if change.change_id.is_empty() || change.entity_id.is_empty() {
        bail!("Sync change IDs cannot be empty");
    }
    validate_remote_revision(&change.revision, now_millis)?;

    let payload = normalize_payload(change.entity_kind, change.deleted, change.payload.clone())?;
    let content_hash = canonical_content_hash(
        change.entity_kind,
        &change.entity_id,
        change.deleted,
        payload.as_ref(),
    )?;
    let mut canonical = change.clone();
    canonical.payload = payload;
    Ok(PreparedChange {
        change: canonical,
        content_hash,
    })
}

fn normalize_payload(
    entity_kind: SyncEntityKind,
    deleted: bool,
    payload: Option<Value>,
) -> Result<Option<Value>> {
    if deleted {
        if payload.is_some() {
            bail!("A sync tombstone cannot contain a payload");
        }
        return Ok(None);
    }

    let payload = payload.ok_or_else(|| anyhow!("A sync upsert requires a payload"))?;
    let canonical = match entity_kind {
        SyncEntityKind::Server => serde_json::to_value(
            serde_json::from_value::<ServerSyncPayload>(payload)
                .context("Invalid server payload in sync change")?,
        )?,
        SyncEntityKind::Group => serde_json::to_value(
            serde_json::from_value::<GroupSyncPayload>(payload)
                .context("Invalid group payload in sync change")?,
        )?,
        SyncEntityKind::CommandSnippet => serde_json::to_value(
            serde_json::from_value::<SnippetSyncPayload>(payload)
                .context("Invalid command snippet payload in sync change")?,
        )?,
        SyncEntityKind::PluginInstallation => serde_json::to_value(
            serde_json::from_value::<PluginInstallationSyncPayload>(payload)
                .context("Invalid plugin installation payload in sync change")?,
        )?,
    };
    Ok(Some(canonical))
}

fn canonical_content_hash(
    entity_kind: SyncEntityKind,
    entity_id: &str,
    deleted: bool,
    payload: Option<&Value>,
) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"vibeshell-sync-content-v1\0");
    hasher.update(entity_kind.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(entity_id.as_bytes());
    hasher.update(b"\0");
    if deleted {
        hasher.update(b"tombstone");
    } else {
        hasher.update(b"upsert");
    }
    hasher.update(b"\0");
    if let Some(payload) = payload {
        hasher.update(serde_json::to_vec(payload)?);
    }
    let digest = hasher.finalize();
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn resolve_name_collision(
    conn: &Connection,
    prepared: &PreparedChange,
) -> Result<NameCollisionDecision> {
    let change = &prepared.change;
    if change.deleted
        || change.entity_kind == SyncEntityKind::CommandSnippet
        || change.entity_kind == SyncEntityKind::PluginInstallation
    {
        return Ok(NameCollisionDecision::NoCollision);
    }

    let name = change
        .payload
        .as_ref()
        .and_then(|payload| payload.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Sync payload is missing a canonical name"))?;
    let table = match change.entity_kind {
        SyncEntityKind::Server => "servers",
        SyncEntityKind::Group => "groups",
        SyncEntityKind::CommandSnippet => unreachable!(),
        SyncEntityKind::PluginInstallation => unreachable!(),
    };
    let contender_id = conn
        .query_row(
            &format!("SELECT id FROM {table} WHERE name = ?1 AND id <> ?2"),
            params![name, change.entity_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(contender_id) = contender_id else {
        return Ok(NameCollisionDecision::NoCollision);
    };

    let contender_state =
        entity_state(conn, change.entity_kind, &contender_id)?.ok_or_else(|| {
            anyhow!(
                "Name contender is missing sync state for {}:{}",
                change.entity_kind.as_str(),
                contender_id
            )
        })?;
    if contender_state.deleted {
        bail!("A tombstoned sync entity still owns a unique domain name");
    }

    let incoming_wins = match change.revision.cmp(&contender_state.revision) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => change.entity_id < contender_id,
    };

    if incoming_wins {
        Ok(NameCollisionDecision::IncomingWins {
            loser_id: contender_id,
            loser_state: contender_state,
        })
    } else {
        Ok(NameCollisionDecision::ExistingWins {
            winner_id: contender_id,
            winner_state: contender_state,
        })
    }
}

fn deterministic_conflict_name(original_name: &str, entity_id: &str) -> String {
    format!("{original_name} (conflict {entity_id})")
}

fn conflict_named_payload(
    entity_kind: SyncEntityKind,
    entity_id: &str,
    mut payload: Value,
) -> Result<Value> {
    if entity_kind == SyncEntityKind::CommandSnippet {
        bail!("Command snippets do not use unique-name conflict resolution");
    }
    let original_name = payload
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Sync payload is missing a canonical name"))?;
    let conflict_name = deterministic_conflict_name(original_name, entity_id);
    payload
        .as_object_mut()
        .ok_or_else(|| anyhow!("Sync payload must be a JSON object"))?
        .insert("name".to_string(), Value::String(conflict_name));
    Ok(payload)
}

fn conflict_named_change(prepared: &PreparedChange) -> Result<PreparedChange> {
    let mut effective = prepared.clone();
    let payload = effective
        .change
        .payload
        .take()
        .ok_or_else(|| anyhow!("A sync upsert requires a payload"))?;
    let payload = conflict_named_payload(
        effective.change.entity_kind,
        &effective.change.entity_id,
        payload,
    )?;
    effective.content_hash = canonical_content_hash(
        effective.change.entity_kind,
        &effective.change.entity_id,
        false,
        Some(&payload),
    )?;
    effective.change.payload = Some(payload);
    Ok(effective)
}

fn materialize_existing_conflict_name(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
    state: &EntityState,
) -> Result<()> {
    let payload = current_domain_payload(conn, entity_kind, entity_id)?
        .ok_or_else(|| anyhow!("Name-conflict loser is missing its domain row"))?;
    let original_name = payload
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Sync payload is missing a canonical name"))?
        .to_string();
    let conflict_payload = conflict_named_payload(entity_kind, entity_id, payload)?;
    let conflict_name = conflict_payload
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Conflict payload is missing a canonical name"))?;

    let materialized = SyncChange {
        schema_version: SYNC_CHANGE_SCHEMA_VERSION,
        change_id: format!("conflict-materialization:{entity_id}"),
        entity_kind,
        entity_id: entity_id.to_string(),
        revision: state.revision.clone(),
        deleted: false,
        payload: Some(conflict_payload.clone()),
    };
    apply_remote_change(conn, &materialized)?;
    if entity_kind == SyncEntityKind::Server {
        move_server_credential_key(conn, &original_name, conflict_name)?;
    }
    rewrite_pending_conflict_names(conn, entity_kind, entity_id, &original_name, conflict_name)?;
    let content_hash =
        canonical_content_hash(entity_kind, entity_id, false, Some(&conflict_payload))?;
    upsert_entity_state(
        conn,
        entity_kind,
        entity_id,
        &state.revision,
        false,
        &content_hash,
    )?;
    Ok(())
}

fn move_server_credential_key(
    conn: &Connection,
    original_name: &str,
    conflict_name: &str,
) -> Result<()> {
    let table_exists = conn.query_row(
        r#"SELECT EXISTS(
               SELECT 1 FROM sqlite_master
               WHERE type = 'table' AND name = 'server_credentials'
           )"#,
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !table_exists {
        return Ok(());
    }

    conn.execute(
        r#"UPDATE server_credentials SET server_name = ?2 WHERE server_name = ?1"#,
        params![original_name, conflict_name],
    )?;
    Ok(())
}

fn move_existing_server_credential_for_conflict(
    conn: &Connection,
    change: &SyncChange,
) -> Result<()> {
    if change.entity_kind != SyncEntityKind::Server {
        return Ok(());
    }
    let conflict_name = change
        .payload
        .as_ref()
        .and_then(|payload| payload.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Server conflict payload is missing its name"))?;
    let current_name = conn
        .query_row(
            "SELECT name FROM servers WHERE id = ?1",
            [&change.entity_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(current_name) = current_name {
        if current_name != conflict_name {
            move_server_credential_key(conn, &current_name, conflict_name)?;
        }
    }
    Ok(())
}

fn rewrite_pending_conflict_names(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
    original_name: &str,
    conflict_name: &str,
) -> Result<()> {
    let rows = {
        let mut stmt = conn.prepare(
            r#"SELECT change_id, payload FROM sync_outbox
               WHERE entity_kind = ?1 AND entity_id = ?2 AND is_tombstone = 0"#,
        )?;
        let rows = stmt
            .query_map(params![entity_kind.as_str(), entity_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };

    for (change_id, payload_json) in rows {
        let mut payload: Value = serde_json::from_str(&payload_json)?;
        if payload.get("name").and_then(Value::as_str) != Some(original_name) {
            continue;
        }
        payload
            .as_object_mut()
            .ok_or_else(|| anyhow!("Sync outbox payload must be a JSON object"))?
            .insert("name".to_string(), Value::String(conflict_name.to_string()));
        conn.execute(
            "UPDATE sync_outbox SET payload = ?2 WHERE change_id = ?1",
            params![change_id, serde_json::to_string(&payload)?],
        )?;
    }
    Ok(())
}

fn delete_domain_entity(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
) -> Result<()> {
    match entity_kind {
        SyncEntityKind::Server => detach_server_references(conn, entity_id, false)?,
        SyncEntityKind::Group => detach_group_references(conn, entity_id, false)?,
        SyncEntityKind::CommandSnippet => {}
        SyncEntityKind::PluginInstallation => {}
    }
    let (table, id_column) = match entity_kind {
        SyncEntityKind::Server => ("servers", "id"),
        SyncEntityKind::Group => ("groups", "id"),
        SyncEntityKind::CommandSnippet => ("command_snippets", "id"),
        SyncEntityKind::PluginInstallation => ("plugin_installations", "plugin_id"),
    };
    conn.execute(
        &format!("DELETE FROM {table} WHERE {id_column} = ?1"),
        [entity_id],
    )?;
    Ok(())
}

fn remove_outbox_through(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
    revision: &str,
) -> Result<()> {
    conn.execute(
        r#"DELETE FROM sync_outbox
           WHERE entity_kind = ?1 AND entity_id = ?2 AND revision <= ?3"#,
        params![entity_kind.as_str(), entity_id, revision],
    )?;
    Ok(())
}

fn apply_remote_change(conn: &Connection, change: &SyncChange) -> Result<()> {
    if change.deleted {
        delete_domain_entity(conn, change.entity_kind, &change.entity_id)?;
        return Ok(());
    }

    let payload = change
        .payload
        .clone()
        .ok_or_else(|| anyhow!("A sync upsert requires a payload"))?;
    match change.entity_kind {
        SyncEntityKind::Server => {
            let server: ServerSyncPayload = serde_json::from_value(payload)
                .context("Invalid server payload in remote sync change")?;
            apply_remote_server(conn, &change.entity_id, server)?;
        }
        SyncEntityKind::Group => {
            let group: GroupSyncPayload = serde_json::from_value(payload)
                .context("Invalid group payload in remote sync change")?;
            apply_remote_group(conn, &change.entity_id, group)?;
        }
        SyncEntityKind::CommandSnippet => {
            let snippet: SnippetSyncPayload = serde_json::from_value(payload)
                .context("Invalid command snippet payload in remote sync change")?;
            apply_remote_snippet(conn, &change.entity_id, snippet)?;
        }
        SyncEntityKind::PluginInstallation => {
            let installation: PluginInstallationSyncPayload = serde_json::from_value(payload)
                .context("Invalid plugin installation payload in remote sync change")?;
            apply_remote_plugin_installation(conn, &change.entity_id, installation)?;
        }
    }
    Ok(())
}

fn apply_remote_server(conn: &Connection, id: &str, mut server: ServerSyncPayload) -> Result<()> {
    if reference_is_tombstoned(conn, SyncEntityKind::Group, server.group_id.as_deref())? {
        server.group_id = None;
    }
    if reference_is_tombstoned(conn, SyncEntityKind::Server, server.jump_host_id.as_deref())? {
        server.jump_host_id = None;
    }
    let tags = serde_json::to_string(&server.tags)?;
    conn.execute(
        r#"INSERT INTO servers
           (id, name, host, port, username, auth_type, credential_id, group_id, tags,
            created_at, updated_at, jump_host_id, post_login_command, agent_forwarding)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
           ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               host = excluded.host,
               port = excluded.port,
               username = excluded.username,
               auth_type = excluded.auth_type,
               group_id = excluded.group_id,
               tags = excluded.tags,
               created_at = excluded.created_at,
               updated_at = excluded.updated_at,
               jump_host_id = excluded.jump_host_id,
               post_login_command = excluded.post_login_command,
               agent_forwarding = excluded.agent_forwarding"#,
        params![
            id,
            server.name,
            server.host,
            server.port,
            server.username,
            auth_type_to_string(&server.auth_type),
            server.group_id,
            tags,
            server.created_at,
            server.updated_at,
            server.jump_host_id,
            server.post_login_command,
            server.agent_forwarding as i32,
        ],
    )?;
    Ok(())
}

fn apply_remote_group(conn: &Connection, id: &str, mut group: GroupSyncPayload) -> Result<()> {
    if reference_is_tombstoned(conn, SyncEntityKind::Group, group.parent_id.as_deref())? {
        group.parent_id = None;
    }
    conn.execute(
        r#"INSERT INTO groups (id, name, parent_id, color)
           VALUES (?1, ?2, ?3, ?4)
           ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               parent_id = excluded.parent_id,
               color = excluded.color"#,
        params![id, group.name, group.parent_id, group.color],
    )?;
    Ok(())
}

/// Recompute the granted permission set for a synced installation. Grants are
/// never transported: an enabled plugin receives exactly the permissions its
/// manifest declares on this device, and a disabled one receives none.
fn synced_grant_permissions(installation: &PluginInstallationSyncPayload) -> Result<String> {
    if !installation.enabled {
        return Ok("[]".to_string());
    }

    let permissions = match installation.source.as_str() {
        "external" => {
            let manifest_json = installation
                .manifest_json
                .as_deref()
                .ok_or_else(|| anyhow!("External plugin sync payload is missing a manifest"))?;
            let manifest = crate::plugins::parse_manifest(
                manifest_json,
                crate::plugins::ManifestValidationPolicy::External,
            )
            .map_err(|error| anyhow!("Synced external plugin manifest is invalid: {error}"))?;
            manifest.permissions
        }
        _ => crate::plugins::builtin_catalog()
            .map_err(|error| anyhow!("Built-in plugin catalog is invalid: {error}"))?
            .into_iter()
            .find(|manifest| manifest.id == installation.plugin_id)
            .ok_or_else(|| {
                anyhow!(
                    "Synced built-in plugin {} is unknown on this device",
                    installation.plugin_id
                )
            })?
            .permissions,
    };

    serde_json::to_string(&permissions).map_err(|error| anyhow!("Failed to encode grants: {error}"))
}

fn apply_remote_plugin_installation(
    conn: &Connection,
    plugin_id: &str,
    installation: PluginInstallationSyncPayload,
) -> Result<()> {
    // An unknown or invalid plugin never enables itself on restore: it lands
    // disabled with empty grants instead of failing the whole backup import.
    let granted_permissions_json = match synced_grant_permissions(&installation) {
        Ok(grants) => grants,
        Err(error) => {
            log::warn!(
                "Disabling synced plugin {}: {}",
                installation.plugin_id,
                error
            );
            "[]".to_string()
        }
    };
    let enabled = if granted_permissions_json == "[]" {
        0
    } else {
        installation.enabled as i32
    };
    let manifest_json = installation
        .manifest_json
        .clone()
        .unwrap_or_else(|| "{}".to_string());

    conn.execute(
        r#"INSERT INTO plugin_installations
           (plugin_id, version, manifest_json, source, enabled,
            granted_permissions_json, settings_json, installed_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
           ON CONFLICT(plugin_id) DO UPDATE SET
             version = excluded.version,
             manifest_json = excluded.manifest_json,
             source = excluded.source,
             enabled = excluded.enabled,
             granted_permissions_json = excluded.granted_permissions_json,
             settings_json = excluded.settings_json,
             updated_at = excluded.updated_at"#,
        params![
            plugin_id,
            installation.version,
            manifest_json,
            installation.source,
            enabled,
            granted_permissions_json,
            installation.settings_json,
            installation.installed_at,
            installation.updated_at,
        ],
    )?;
    Ok(())
}

fn reference_is_tombstoned(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: Option<&str>,
) -> Result<bool> {    let Some(entity_id) = entity_id else {
        return Ok(false);
    };
    Ok(entity_state(conn, entity_kind, entity_id)?
        .map(|state| state.deleted)
        .unwrap_or(false))
}

fn apply_remote_snippet(conn: &Connection, id: &str, snippet: SnippetSyncPayload) -> Result<()> {
    let tags = serde_json::to_string(&snippet.tags)?;
    conn.execute(
        r#"INSERT INTO command_snippets
           (id, name, command, category, description, tags, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
           ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               command = excluded.command,
               category = excluded.category,
               description = excluded.description,
               tags = excluded.tags,
               created_at = excluded.created_at,
               updated_at = excluded.updated_at"#,
        params![
            id,
            snippet.name,
            snippet.command,
            snippet.category,
            snippet.description,
            tags,
            snippet.created_at,
            snippet.updated_at,
        ],
    )?;
    Ok(())
}

fn backfill_content_hashes(conn: &Connection) -> Result<()> {
    let states = {
        let mut stmt = conn.prepare(
            r#"SELECT entity_kind, entity_id, is_tombstone
               FROM sync_entity_state
               WHERE content_hash IS NULL OR content_hash = ''"#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for (kind, entity_id, deleted) in states {
        let entity_kind = SyncEntityKind::from_db(&kind)?;
        let payload = if deleted {
            None
        } else {
            Some(
                current_domain_payload(conn, entity_kind, &entity_id)?.ok_or_else(|| {
                    anyhow!(
                        "Cannot backfill content hash for missing {}:{}",
                        entity_kind.as_str(),
                        entity_id
                    )
                })?,
            )
        };
        let payload = normalize_payload(entity_kind, deleted, payload)?;
        let content_hash =
            canonical_content_hash(entity_kind, &entity_id, deleted, payload.as_ref())?;
        conn.execute(
            r#"UPDATE sync_entity_state SET content_hash = ?3
               WHERE entity_kind = ?1 AND entity_id = ?2"#,
            params![entity_kind.as_str(), entity_id, content_hash],
        )?;
    }
    Ok(())
}

fn current_domain_payload(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
) -> Result<Option<Value>> {
    match entity_kind {
        SyncEntityKind::PluginInstallation => {
            let payload = conn
                .query_row(
                    r#"SELECT plugin_id, version, source, enabled, manifest_json,
                              settings_json, installed_at, updated_at
                       FROM plugin_installations WHERE plugin_id = ?1"#,
                    [entity_id],
                    |row| {
                        let source: String = row.get(2)?;
                        Ok(PluginInstallationSyncPayload {
                            plugin_id: row.get(0)?,
                            version: row.get(1)?,
                            enabled: row.get::<_, i64>(3)? != 0,
                            // Built-in manifests resolve from the local catalog;
                            // only external manifests travel with the payload.
                            manifest_json: if source == "external" {
                                row.get(4)?
                            } else {
                                None
                            },
                            source,
                            settings_json: row.get(5)?,
                            installed_at: row.get(6)?,
                            updated_at: row.get(7)?,
                        })
                    },
                )
                .optional()?;
            payload
                .map(serde_json::to_value)
                .transpose()
                .map_err(Into::into)
        }
        SyncEntityKind::Server => {
            let payload = conn
                .query_row(
                    r#"SELECT name, host, port, username, auth_type, group_id, tags,
                              created_at, updated_at, jump_host_id, post_login_command,
                              agent_forwarding
                       FROM servers WHERE id = ?1"#,
                    [entity_id],
                    |row| {
                        let auth_type: String = row.get(4)?;
                        let tags: String = row.get(6)?;
                        Ok(ServerSyncPayload {
                            name: row.get(0)?,
                            host: row.get(1)?,
                            port: row.get(2)?,
                            username: row.get(3)?,
                            auth_type: string_to_auth_type(&auth_type),
                            group_id: row.get(5)?,
                            tags: serde_json::from_str(&tags).unwrap_or_default(),
                            created_at: row.get(7)?,
                            updated_at: row.get(8)?,
                            jump_host_id: row.get(9).unwrap_or(None),
                            post_login_command: row.get(10).unwrap_or(None),
                            agent_forwarding: row.get::<_, i32>(11).unwrap_or(0) != 0,
                        })
                    },
                )
                .optional()?;
            payload
                .map(serde_json::to_value)
                .transpose()
                .map_err(Into::into)
        }
        SyncEntityKind::Group => {
            let payload = conn
                .query_row(
                    "SELECT name, parent_id, color FROM groups WHERE id = ?1",
                    [entity_id],
                    |row| {
                        Ok(GroupSyncPayload {
                            name: row.get(0)?,
                            parent_id: row.get(1)?,
                            color: row.get(2)?,
                        })
                    },
                )
                .optional()?;
            payload
                .map(serde_json::to_value)
                .transpose()
                .map_err(Into::into)
        }
        SyncEntityKind::CommandSnippet => {
            let payload = conn
                .query_row(
                    r#"SELECT name, command, category, description, tags, created_at, updated_at
                       FROM command_snippets WHERE id = ?1"#,
                    [entity_id],
                    |row| {
                        let tags: String = row.get(4)?;
                        Ok(SnippetSyncPayload {
                            name: row.get(0)?,
                            command: row.get(1)?,
                            category: row.get(2)?,
                            description: row.get(3)?,
                            tags: serde_json::from_str(&tags).unwrap_or_default(),
                            created_at: row.get(5)?,
                            updated_at: row.get(6)?,
                        })
                    },
                )
                .optional()?;
            payload
                .map(serde_json::to_value)
                .transpose()
                .map_err(Into::into)
        }
    }
}

fn bootstrap_existing_rows(conn: &Connection) -> Result<()> {
    bootstrap_servers(conn)?;
    bootstrap_groups(conn)?;
    bootstrap_snippets(conn)?;
    bootstrap_plugin_installations(conn)?;
    Ok(())
}

fn bootstrap_plugin_installations(conn: &Connection) -> Result<()> {
    let plugin_ids: Vec<String> = {
        let mut stmt =
            conn.prepare("SELECT plugin_id FROM plugin_installations ORDER BY plugin_id")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for plugin_id in plugin_ids {
        record_current_upsert(conn, SyncEntityKind::PluginInstallation, &plugin_id)?;
    }
    Ok(())
}

fn rebuild_full_outbox(conn: &Connection) -> Result<()> {
    bootstrap_existing_rows(conn)?;
    let states = {
        let mut statement = conn.prepare(
            r#"SELECT entity_kind, entity_id, is_tombstone
               FROM sync_entity_state
               ORDER BY entity_kind, entity_id"#,
        )?;
        let states = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        states
    };

    conn.execute("DELETE FROM sync_outbox", [])?;
    for (kind, entity_id, deleted) in states {
        let entity_kind = SyncEntityKind::from_db(&kind)?;
        if deleted {
            record_local_delete(conn, entity_kind, &entity_id)?;
        } else {
            record_current_upsert(conn, entity_kind, &entity_id)?;
        }
    }
    Ok(())
}

fn bootstrap_servers(conn: &Connection) -> Result<()> {
    let servers = {
        let mut stmt = conn.prepare(
            r#"SELECT id, name, host, port, username, auth_type, group_id, tags,
                      created_at, updated_at, jump_host_id, post_login_command, agent_forwarding
               FROM servers"#,
        )?;
        let rows = stmt.query_map([], |row| {
            let auth_type: String = row.get(5)?;
            let tags: String = row.get(7)?;
            Ok((
                row.get::<_, String>(0)?,
                ServerSyncPayload {
                    name: row.get(1)?,
                    host: row.get(2)?,
                    port: row.get(3)?,
                    username: row.get(4)?,
                    auth_type: string_to_auth_type(&auth_type),
                    group_id: row.get(6)?,
                    tags: serde_json::from_str(&tags).unwrap_or_default(),
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    jump_host_id: row.get(10).unwrap_or(None),
                    post_login_command: row.get(11).unwrap_or(None),
                    agent_forwarding: row.get::<_, i32>(12).unwrap_or(0) != 0,
                },
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for (id, payload) in servers {
        bootstrap_untracked(
            conn,
            SyncEntityKind::Server,
            &id,
            serde_json::to_value(payload)?,
        )?;
    }
    Ok(())
}

fn bootstrap_groups(conn: &Connection) -> Result<()> {
    let groups = {
        let mut stmt = conn.prepare("SELECT id, name, parent_id, color FROM groups")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                GroupSyncPayload {
                    name: row.get(1)?,
                    parent_id: row.get(2)?,
                    color: row.get(3)?,
                },
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for (id, payload) in groups {
        bootstrap_untracked(
            conn,
            SyncEntityKind::Group,
            &id,
            serde_json::to_value(payload)?,
        )?;
    }
    Ok(())
}

fn bootstrap_snippets(conn: &Connection) -> Result<()> {
    let snippets = {
        let mut stmt = conn.prepare(
            r#"SELECT id, name, command, category, description, tags, created_at, updated_at
               FROM command_snippets"#,
        )?;
        let rows = stmt.query_map([], |row| {
            let tags: String = row.get(5)?;
            Ok((
                row.get::<_, String>(0)?,
                SnippetSyncPayload {
                    name: row.get(1)?,
                    command: row.get(2)?,
                    category: row.get(3)?,
                    description: row.get(4)?,
                    tags: serde_json::from_str(&tags).unwrap_or_default(),
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                },
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for (id, payload) in snippets {
        bootstrap_untracked(
            conn,
            SyncEntityKind::CommandSnippet,
            &id,
            serde_json::to_value(payload)?,
        )?;
    }
    Ok(())
}

fn bootstrap_untracked(
    conn: &Connection,
    entity_kind: SyncEntityKind,
    entity_id: &str,
    payload: Value,
) -> Result<()> {
    if entity_state(conn, entity_kind, entity_id)?.is_none() {
        record_local_upsert(conn, entity_kind, entity_id, payload)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct ServerSyncPayload {
    name: String,
    host: String,
    port: u16,
    username: String,
    auth_type: AuthType,
    group_id: Option<String>,
    tags: Vec<String>,
    created_at: i64,
    updated_at: i64,
    jump_host_id: Option<String>,
    post_login_command: Option<String>,
    agent_forwarding: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct GroupSyncPayload {
    name: String,
    parent_id: Option<String>,
    color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct SnippetSyncPayload {
    name: String,
    command: String,
    category: String,
    description: String,
    tags: Vec<String>,
    created_at: i64,
    updated_at: i64,
}

/// Portable representation of a plugin installation. Built-in plugins omit the
/// manifest (it is resolved from the local catalog); external plugins carry
/// their imported manifest so a backup can fully reconstruct them. Granted
/// permissions are intentionally excluded — they are recomputed from the
/// manifest on the receiving device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginInstallationSyncPayload {
    plugin_id: String,
    version: String,
    source: String,
    enabled: bool,
    manifest_json: Option<String>,
    settings_json: String,
    installed_at: i64,
    updated_at: i64,
}

fn auth_type_to_string(auth_type: &AuthType) -> &'static str {
    match auth_type {
        AuthType::Password => "password",
        AuthType::Key => "key",
        AuthType::KeyWithPassphrase => "key_with_passphrase",
    }
}

fn string_to_auth_type(value: &str) -> AuthType {
    match value {
        "key" => AuthType::Key,
        "key_with_passphrase" => AuthType::KeyWithPassphrase,
        _ => AuthType::Password,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::Path;

    use rusqlite::Connection;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::storage::{Recording, SyncStatus, TunnelConfig, TunnelType};

    fn test_database() -> (TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::new_at(dir.path().join("vibeshell.db")).unwrap();
        (dir, database)
    }

    fn server(name: &str) -> Server {
        Server {
            id: String::new(),
            name: name.to_string(),
            host: format!("{name}.example.com"),
            port: 22,
            username: "root".to_string(),
            auth_type: AuthType::Password,
            credential_id: Some("device-local-credential".to_string()),
            group_id: None,
            tags: vec!["production".to_string()],
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: Some("uptime".to_string()),
            agent_forwarding: false,
        }
    }

    fn group(name: &str) -> Group {
        Group {
            id: String::new(),
            name: name.to_string(),
            parent_id: None,
            color: "#808080".to_string(),
        }
    }

    fn snippet(name: &str) -> CommandSnippet {
        CommandSnippet {
            id: String::new(),
            name: name.to_string(),
            command: "systemctl status nginx".to_string(),
            category: "operations".to_string(),
            description: "Check nginx".to_string(),
            tags: vec!["nginx".to_string()],
            created_at: 0,
            updated_at: 0,
        }
    }

    fn pending(database: &Database) -> Vec<SyncChange> {
        database
            .cloud_sync()
            .export_pending_changes(100)
            .unwrap()
            .changes
    }

    fn acknowledge_all(database: &Database) {
        let ids = pending(database)
            .into_iter()
            .map(|change| change.change_id)
            .collect::<Vec<_>>();
        database.cloud_sync().acknowledge_changes(&ids).unwrap();
    }

    fn remote_upsert(
        entity_kind: SyncEntityKind,
        entity_id: &str,
        millis: i64,
        payload: Value,
    ) -> SyncChange {
        SyncChange {
            schema_version: SYNC_CHANGE_SCHEMA_VERSION,
            change_id: Uuid::new_v4().to_string(),
            entity_kind,
            entity_id: entity_id.to_string(),
            revision: format!("{millis:020}:remote-device"),
            deleted: false,
            payload: Some(payload),
        }
    }

    fn remote_delete(entity_kind: SyncEntityKind, entity_id: &str, millis: i64) -> SyncChange {
        SyncChange {
            schema_version: SYNC_CHANGE_SCHEMA_VERSION,
            change_id: Uuid::new_v4().to_string(),
            entity_kind,
            entity_id: entity_id.to_string(),
            revision: format!("{millis:020}:remote-device"),
            deleted: true,
            payload: None,
        }
    }

    fn winning_change<'a>(left: &'a SyncChange, right: &'a SyncChange) -> &'a SyncChange {
        match left.revision.cmp(&right.revision) {
            Ordering::Greater => left,
            Ordering::Less => right,
            Ordering::Equal if left.entity_id < right.entity_id => left,
            Ordering::Equal => right,
        }
    }

    fn apply_changes_in_arrival_order(
        database: &Database,
        cursor_prefix: &str,
        changes: &[&SyncChange],
    ) {
        for (index, change) in changes.iter().enumerate() {
            database
                .cloud_sync()
                .apply_remote_batch(
                    &format!("{cursor_prefix}-{index}"),
                    std::slice::from_ref(*change),
                )
                .unwrap();
        }
    }

    #[test]
    fn stable_entity_kind_serialization() {
        assert_eq!(
            serde_json::to_string(&SyncEntityKind::CommandSnippet).unwrap(),
            r#""command_snippet""#
        );
        assert_eq!(
            serde_json::to_string(&SyncEntityKind::PluginInstallation).unwrap(),
            r#""plugin_installation""#
        );
    }

    const EXTERNAL_MANIFEST: &str = r#"{
      "schemaVersion": 1,
      "id": "example.remote-tools",
      "name": "Remote Tools",
      "description": "Read remote tool output",
      "version": "1.0.0",
      "author": "Example",
      "category": "operations",
      "icon": "wrench",
      "permissions": ["remote_exec"],
      "sessionTypes": ["ssh"],
      "entry": {
        "type": "commands",
        "actions": [{
          "id": "version",
          "name": "Version",
          "description": "Show the tool version",
          "program": "tool",
          "args": ["--version"]
        }]
      }
    }"#;

    fn plugin_installation(
        plugin_id: &str,
        source: &str,
        enabled: bool,
        manifest_json: &str,
    ) -> PluginInstallation {
        PluginInstallation {
            plugin_id: plugin_id.to_string(),
            version: "1.0.0".to_string(),
            manifest_json: manifest_json.to_string(),
            source: source.to_string(),
            enabled,
            granted_permissions_json: r#"["remote_exec"]"#.to_string(),
            settings_json: "{}".to_string(),
            installed_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn plugin_installations_join_backups_and_carry_external_manifests() {
        let (_dir, database) = test_database();

        database
            .plugin_installation_upsert(&plugin_installation(
                "docker-containers",
                "builtin",
                true,
                "{}",
            ))
            .unwrap();
        database
            .plugin_installation_upsert(&plugin_installation(
                "example.remote-tools",
                "external",
                false,
                EXTERNAL_MANIFEST,
            ))
            .unwrap();

        // Both CRUD writes were captured in the sync outbox.
        let outbox_kinds = pending(&database)
            .into_iter()
            .filter(|change| change.entity_kind == SyncEntityKind::PluginInstallation)
            .count();
        assert_eq!(outbox_kinds, 2);

        // The backup snapshot carries external manifests; built-in plugins
        // resolve their manifest from the local catalog instead.
        let snapshot = database.cloud_sync().export_snapshot().unwrap();
        let docker = snapshot
            .changes
            .iter()
            .find(|change| change.entity_id == "docker-containers")
            .unwrap();
        assert!(docker.payload.as_ref().unwrap()["manifestJson"].is_null());
        let external = snapshot
            .changes
            .iter()
            .find(|change| change.entity_id == "example.remote-tools")
            .unwrap();
        assert_eq!(
            external.payload.as_ref().unwrap()["manifestJson"],
            json!(EXTERNAL_MANIFEST)
        );

        // Deleting emits a tombstone like every other entity.
        database.plugin_installation_delete("docker-containers").unwrap();
        let tombstone = pending(&database)
            .into_iter()
            .find(|change| {
                change.entity_kind == SyncEntityKind::PluginInstallation
                    && change.entity_id == "docker-containers"
                    && change.deleted
            })
            .expect("plugin delete should emit a tombstone");
    }

    #[test]
    fn restored_plugins_recompute_grants_and_gate_unknown_builtins() {
        let (_dir, database) = test_database();

        let enabled_external = json!({
            "pluginId": "example.remote-tools",
            "version": "1.0.0",
            "source": "external",
            "enabled": true,
            "manifestJson": EXTERNAL_MANIFEST,
            "settingsJson": "{\"rows\":25}",
            "installedAt": 1,
            "updatedAt": 2,
        });
        database
            .cloud_sync()
            .apply_imported_changes(&[remote_upsert(
                SyncEntityKind::PluginInstallation,
                "example.remote-tools",
                100,
                enabled_external,
            )])
            .unwrap();
        let restored = database
            .plugin_installation_get("example.remote-tools")
            .unwrap()
            .unwrap();
        assert!(restored.enabled);
        // Grants are recomputed from the manifest, never transported.
        assert_eq!(restored.granted_permissions_json, r#"["remote_exec"]"#);
        assert_eq!(restored.settings_json, r#"{"rows":25}"#);

        // A built-in plugin this app does not know restores disabled with no
        // grants instead of failing the whole backup import.
        let unknown_builtin = json!({
            "pluginId": "future.toolkit",
            "version": "9.0.0",
            "source": "builtin",
            "enabled": true,
            "manifestJson": null,
            "settingsJson": "{}",
            "installedAt": 1,
            "updatedAt": 2,
        });
        database
            .cloud_sync()
            .apply_imported_changes(&[remote_upsert(
                SyncEntityKind::PluginInstallation,
                "future.toolkit",
                200,
                unknown_builtin,
            )])
            .unwrap();
        let gated = database
            .plugin_installation_get("future.toolkit")
            .unwrap()
            .unwrap();
        assert!(!gated.enabled);
        assert_eq!(gated.granted_permissions_json, "[]");
    }

    #[test]
    fn local_crud_is_captured_with_monotonic_revisions_and_tombstones() {
        let (_dir, database) = test_database();
        let mut group = group("Production");
        database.group_add(&mut group).unwrap();

        let mut server = server("edge");
        server.group_id = Some(group.id.clone());
        database.server_add(&mut server).unwrap();

        let mut snippet = snippet("nginx-status");
        database.snippet_add(&mut snippet).unwrap();

        let initial = pending(&database);
        assert_eq!(initial.len(), 3);
        assert!(initial
            .windows(2)
            .all(|pair| pair[0].revision < pair[1].revision));

        let kinds = initial
            .iter()
            .map(|change| change.entity_kind)
            .collect::<HashSet<_>>();
        assert_eq!(
            kinds,
            HashSet::from([
                SyncEntityKind::Server,
                SyncEntityKind::Group,
                SyncEntityKind::CommandSnippet,
            ])
        );

        let server_change = initial
            .iter()
            .find(|change| change.entity_kind == SyncEntityKind::Server)
            .unwrap();
        let server_payload = server_change.payload.as_ref().unwrap();
        assert_eq!(server_payload["host"], "edge.example.com");
        assert!(server_payload.get("credentialId").is_none());
        assert!(server_payload.get("credential_id").is_none());

        acknowledge_all(&database);
        assert!(pending(&database).is_empty());

        server.host = "edge-2.example.com".to_string();
        database.server_update(&server).unwrap();
        snippet.command = "systemctl reload nginx".to_string();
        database.snippet_update(&snippet).unwrap();

        let updates = pending(&database);
        assert_eq!(updates.len(), 2);
        assert!(updates.iter().all(|change| !change.deleted));
        acknowledge_all(&database);

        database.server_delete(&server.id).unwrap();
        database.group_delete(&group.id).unwrap();
        database.snippet_delete(&snippet.id).unwrap();

        let tombstones = pending(&database);
        assert_eq!(tombstones.len(), 3);
        assert!(tombstones.iter().all(|change| change.deleted));
        assert!(tombstones.iter().all(|change| change.payload.is_none()));
        assert!(tombstones
            .windows(2)
            .all(|pair| pair[0].revision < pair[1].revision));
        assert!(database.server_get(&server.id).unwrap().is_none());
        assert!(database.group_list().unwrap().is_empty());
        assert!(database.snippet_list(None).unwrap().is_empty());
    }

    #[test]
    fn acknowledgement_removes_only_exact_outbox_changes() {
        let (_dir, database) = test_database();
        let mut first = group("First");
        let mut second = group("Second");
        database.group_add(&mut first).unwrap();
        database.group_add(&mut second).unwrap();

        let changes = pending(&database);
        let first_id = changes[0].change_id.clone();
        assert_eq!(
            database
                .cloud_sync()
                .acknowledge_changes(&[first_id.clone(), first_id])
                .unwrap(),
            1
        );

        let remaining = pending(&database);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].change_id, changes[1].change_id);
    }

    #[test]
    fn failed_domain_write_does_not_leave_an_outbox_change() {
        let (_dir, database) = test_database();
        let mut first = server("duplicate");
        database.server_add(&mut first).unwrap();
        acknowledge_all(&database);

        let mut duplicate = server("duplicate");
        assert!(database.server_add(&mut duplicate).is_err());
        assert!(pending(&database).is_empty());
    }

    #[test]
    fn remote_apply_supports_all_v1_entities_and_is_idempotent() {
        let (_source_dir, source) = test_database();
        let mut group = group("Shared");
        source.group_add(&mut group).unwrap();
        let mut server = server("shared-edge");
        server.group_id = Some(group.id.clone());
        source.server_add(&mut server).unwrap();
        let mut snippet = snippet("shared-snippet");
        source.snippet_add(&mut snippet).unwrap();
        let changes = pending(&source);

        let (_target_dir, target) = test_database();
        let report = target
            .cloud_sync()
            .apply_remote_batch("cursor-1", &changes)
            .unwrap();
        assert_eq!(report.applied, 3);
        assert_eq!(report.ignored, 0);
        assert!(report.conflicts.is_empty());
        assert!(pending(&target).is_empty());

        let stored_server = target.server_get(&server.id).unwrap().unwrap();
        assert_eq!(stored_server.host, server.host);
        assert_eq!(stored_server.group_id, Some(group.id));
        assert!(stored_server.credential_id.is_none());
        assert_eq!(target.group_list().unwrap().len(), 1);
        assert_eq!(target.snippet_list(None).unwrap().len(), 1);

        let duplicate = target
            .cloud_sync()
            .apply_remote_batch("cursor-1", &changes)
            .unwrap();
        assert_eq!(duplicate.applied, 0);
        assert_eq!(duplicate.ignored, 3);
        assert!(duplicate.conflicts.is_empty());
        assert_eq!(
            target.cloud_sync().current_remote_cursor().unwrap(),
            Some("cursor-1".to_string())
        );
        target
            .cloud_sync()
            .apply_remote_batch("cursor-2", &[])
            .unwrap();
        assert_eq!(
            target.cloud_sync().current_remote_cursor().unwrap(),
            Some("cursor-2".to_string())
        );
    }

    #[test]
    fn equal_revision_with_different_upsert_hash_rolls_back_separate_batch() {
        let (_dir, database) = test_database();
        let original = remote_upsert(
            SyncEntityKind::Group,
            "hash-group",
            100,
            json!({"name": "Hash Group", "parentId": null, "color": "#111111"}),
        );
        database
            .cloud_sync()
            .apply_remote_batch("hash-cursor-1", std::slice::from_ref(&original))
            .unwrap();

        let mut divergent = original;
        divergent.change_id = Uuid::new_v4().to_string();
        divergent.payload =
            Some(json!({"name": "Hash Group", "parentId": null, "color": "#eeeeee"}));
        assert!(database
            .cloud_sync()
            .apply_remote_batch("hash-cursor-2", &[divergent])
            .is_err());

        assert_eq!(database.group_list().unwrap()[0].color, "#111111");
        assert_eq!(
            database.cloud_sync().current_remote_cursor().unwrap(),
            Some("hash-cursor-1".to_string())
        );
    }

    #[test]
    fn equal_revision_upsert_vs_tombstone_rolls_back_separate_batch() {
        let (_dir, database) = test_database();
        let original = remote_upsert(
            SyncEntityKind::Group,
            "hash-delete-group",
            101,
            json!({"name": "Still Here", "parentId": null, "color": "#222222"}),
        );
        database
            .cloud_sync()
            .apply_remote_batch("delete-hash-cursor-1", std::slice::from_ref(&original))
            .unwrap();
        let mut tombstone = remote_delete(
            SyncEntityKind::Group,
            &original.entity_id,
            revision_millis(&original.revision).unwrap(),
        );
        tombstone.revision = original.revision;

        assert!(database
            .cloud_sync()
            .apply_remote_batch("delete-hash-cursor-2", &[tombstone])
            .is_err());
        assert_eq!(database.group_list().unwrap().len(), 1);
        assert_eq!(
            database.cloud_sync().current_remote_cursor().unwrap(),
            Some("delete-hash-cursor-1".to_string())
        );
    }

    #[test]
    fn remote_batch_is_atomic_when_a_payload_is_invalid() {
        let (_dir, database) = test_database();
        let valid_group = remote_upsert(
            SyncEntityKind::Group,
            "remote-group",
            10,
            json!({"name": "Remote", "parentId": null, "color": "#ffffff"}),
        );
        let invalid_snippet = remote_upsert(
            SyncEntityKind::CommandSnippet,
            "invalid-snippet",
            11,
            json!({"name": "Missing required fields"}),
        );

        database
            .cloud_sync()
            .apply_remote_batch("cursor-before", &[])
            .unwrap();
        assert!(database
            .cloud_sync()
            .apply_remote_batch("cursor-after", &[valid_group, invalid_snippet])
            .is_err());
        assert_eq!(
            database.cloud_sync().current_remote_cursor().unwrap(),
            Some("cursor-before".to_string())
        );
        assert!(database.group_list().unwrap().is_empty());
        assert!(database.snippet_list(None).unwrap().is_empty());
    }

    #[test]
    fn concurrent_server_creates_with_same_name_converge_across_two_devices() {
        let (_dir_a, device_a) = test_database();
        let (_dir_b, device_b) = test_database();
        let mut server_a = server("same-name");
        let mut server_b = server("same-name");
        device_a.server_add(&mut server_a).unwrap();
        device_b.server_add(&mut server_b).unwrap();
        let change_a = pending(&device_a).pop().unwrap();
        let change_b = pending(&device_b).pop().unwrap();
        let winner_id = winning_change(&change_a, &change_b).entity_id.clone();
        let loser_id = if winner_id == change_a.entity_id {
            change_b.entity_id.clone()
        } else {
            change_a.entity_id.clone()
        };

        let report_a = device_a
            .cloud_sync()
            .apply_remote_batch("create-cross-a", &[change_b])
            .unwrap();
        let report_b = device_b
            .cloud_sync()
            .apply_remote_batch("create-cross-b", &[change_a])
            .unwrap();

        for database in [&device_a, &device_b] {
            let servers = database.server_list(None, None).unwrap();
            assert_eq!(servers.len(), 2);
            assert_eq!(
                servers
                    .iter()
                    .find(|server| server.id == winner_id)
                    .unwrap()
                    .name,
                "same-name"
            );
            assert_eq!(
                servers
                    .iter()
                    .find(|server| server.id == loser_id)
                    .unwrap()
                    .name,
                deterministic_conflict_name("same-name", &loser_id)
            );
        }
        assert!(!report_a.conflicts.is_empty());
        assert!(!report_b.conflicts.is_empty());
    }

    #[test]
    fn concurrent_group_creates_with_same_name_converge_across_two_devices() {
        let (_dir_a, device_a) = test_database();
        let (_dir_b, device_b) = test_database();
        let mut group_a = group("Same Group");
        let mut group_b = group("Same Group");
        device_a.group_add(&mut group_a).unwrap();
        device_b.group_add(&mut group_b).unwrap();
        let change_a = pending(&device_a).pop().unwrap();
        let change_b = pending(&device_b).pop().unwrap();
        let winner_id = winning_change(&change_a, &change_b).entity_id.clone();
        let loser_id = if winner_id == change_a.entity_id {
            change_b.entity_id.clone()
        } else {
            change_a.entity_id.clone()
        };

        device_a
            .cloud_sync()
            .apply_remote_batch("group-cross-a", &[change_b])
            .unwrap();
        device_b
            .cloud_sync()
            .apply_remote_batch("group-cross-b", &[change_a])
            .unwrap();

        for database in [&device_a, &device_b] {
            let groups = database.group_list().unwrap();
            assert_eq!(groups.len(), 2);
            assert_eq!(
                groups
                    .iter()
                    .find(|group| group.id == winner_id)
                    .unwrap()
                    .name,
                "Same Group"
            );
            assert_eq!(
                groups
                    .iter()
                    .find(|group| group.id == loser_id)
                    .unwrap()
                    .name,
                deterministic_conflict_name("Same Group", &loser_id)
            );
        }
    }

    #[test]
    fn same_name_group_and_server_logs_converge_after_opposite_out_of_order_replays() {
        let (_source_dir_a, source_a) = test_database();
        let (_source_dir_b, source_b) = test_database();

        let mut group_a = group("Shared Group");
        group_a.color = "#111111".to_string();
        source_a.group_add(&mut group_a).unwrap();
        let mut server_a = server("shared-server");
        server_a.host = "a.example.com".to_string();
        source_a.server_add(&mut server_a).unwrap();

        let mut group_b = group("Shared Group");
        group_b.color = "#222222".to_string();
        source_b.group_add(&mut group_b).unwrap();
        let mut server_b = server("shared-server");
        server_b.host = "b.example.com".to_string();
        source_b.server_add(&mut server_b).unwrap();

        let changes_a = pending(&source_a);
        let changes_b = pending(&source_b);
        let group_change_a = changes_a
            .iter()
            .find(|change| change.entity_kind == SyncEntityKind::Group)
            .unwrap();
        let server_change_a = changes_a
            .iter()
            .find(|change| change.entity_kind == SyncEntityKind::Server)
            .unwrap();
        let group_change_b = changes_b
            .iter()
            .find(|change| change.entity_kind == SyncEntityKind::Group)
            .unwrap();
        let server_change_b = changes_b
            .iter()
            .find(|change| change.entity_kind == SyncEntityKind::Server)
            .unwrap();

        let expected_group_id = winning_change(group_change_a, group_change_b)
            .entity_id
            .clone();
        let expected_group_loser_id = if expected_group_id == group_change_a.entity_id {
            group_change_b.entity_id.clone()
        } else {
            group_change_a.entity_id.clone()
        };
        let expected_server_id = winning_change(server_change_a, server_change_b)
            .entity_id
            .clone();
        let expected_server_loser_id = if expected_server_id == server_change_a.entity_id {
            server_change_b.entity_id.clone()
        } else {
            server_change_a.entity_id.clone()
        };

        let (_replica_dir_a, replica_a) = test_database();
        let (_replica_dir_b, replica_b) = test_database();
        let order_a = [
            server_change_b,
            group_change_a,
            server_change_a,
            group_change_b,
        ];
        let order_b = [
            group_change_b,
            server_change_a,
            group_change_a,
            server_change_b,
        ];

        apply_changes_in_arrival_order(&replica_a, "out-of-order-a", &order_a);
        apply_changes_in_arrival_order(&replica_b, "out-of-order-b", &order_b);
        apply_changes_in_arrival_order(&replica_a, "replay-a", &order_b);
        apply_changes_in_arrival_order(&replica_b, "replay-b", &order_a);

        for replica in [&replica_a, &replica_b] {
            let groups = replica.group_list().unwrap();
            assert_eq!(groups.len(), 2);
            assert_eq!(
                groups
                    .iter()
                    .find(|group| group.id == expected_group_id)
                    .unwrap()
                    .name,
                "Shared Group"
            );
            assert_eq!(
                groups
                    .iter()
                    .find(|group| group.id == expected_group_loser_id)
                    .unwrap()
                    .name,
                deterministic_conflict_name("Shared Group", &expected_group_loser_id)
            );

            let servers = replica.server_list(None, None).unwrap();
            assert_eq!(servers.len(), 2);
            assert_eq!(
                servers
                    .iter()
                    .find(|server| server.id == expected_server_id)
                    .unwrap()
                    .name,
                "shared-server"
            );
            assert_eq!(
                servers
                    .iter()
                    .find(|server| server.id == expected_server_loser_id)
                    .unwrap()
                    .name,
                deterministic_conflict_name("shared-server", &expected_server_loser_id)
            );
            assert!(pending(replica).is_empty());
        }
    }

    #[test]
    fn same_name_group_conflict_preserves_stable_ids_and_group_relationships() {
        let loser = remote_upsert(
            SyncEntityKind::Group,
            "group-loser",
            100,
            json!({"name": "Shared Group", "parentId": null, "color": "#111111"}),
        );
        let child = remote_upsert(
            SyncEntityKind::Group,
            "child-group",
            110,
            json!({"name": "Child Group", "parentId": "group-loser", "color": "#333333"}),
        );
        let member = remote_upsert(
            SyncEntityKind::Server,
            "group-member",
            120,
            json!({
                "name": "group-member",
                "host": "member.example.com",
                "port": 22,
                "username": "root",
                "authType": "password",
                "groupId": "group-loser",
                "tags": [],
                "createdAt": 1,
                "updatedAt": 1,
                "jumpHostId": null,
                "postLoginCommand": null,
                "agentForwarding": false
            }),
        );
        let winner = remote_upsert(
            SyncEntityKind::Group,
            "group-winner",
            200,
            json!({"name": "Shared Group", "parentId": null, "color": "#222222"}),
        );

        let (_dir_a, replica_a) = test_database();
        let (_dir_b, replica_b) = test_database();
        let order_a = [&loser, &child, &member, &winner];
        let order_b = [&winner, &loser, &member, &child];
        apply_changes_in_arrival_order(&replica_a, "group-rel-a", &order_a);
        apply_changes_in_arrival_order(&replica_b, "group-rel-b", &order_b);
        apply_changes_in_arrival_order(&replica_a, "group-rel-replay-a", &order_b);
        apply_changes_in_arrival_order(&replica_b, "group-rel-replay-b", &order_a);

        let mut loser_names = Vec::new();
        for replica in [&replica_a, &replica_b] {
            let groups = replica.group_list().unwrap();
            let loser_group = groups
                .iter()
                .find(|group| group.id == "group-loser")
                .unwrap();
            let winner_group = groups
                .iter()
                .find(|group| group.id == "group-winner")
                .unwrap();
            let child_group = groups
                .iter()
                .find(|group| group.id == "child-group")
                .unwrap();

            assert_eq!(winner_group.name, "Shared Group");
            assert_ne!(loser_group.name, "Shared Group");
            assert!(loser_group.name.contains("group-loser"));
            assert_eq!(child_group.parent_id.as_deref(), Some("group-loser"));
            assert_eq!(
                replica
                    .server_get("group-member")
                    .unwrap()
                    .unwrap()
                    .group_id
                    .as_deref(),
                Some("group-loser")
            );
            loser_names.push(loser_group.name.clone());
        }
        assert_eq!(loser_names[0], loser_names[1]);
    }

    #[test]
    fn same_name_server_conflict_preserves_local_relationships_and_moves_credential_key() {
        let (_dir, database) = test_database();

        let mut jump_host = server("jump-host");
        database.server_add(&mut jump_host).unwrap();

        let mut loser = server("Shared Server");
        loser.jump_host_id = Some(jump_host.id.clone());
        database.server_add(&mut loser).unwrap();
        let loser_change = pending(&database)
            .into_iter()
            .find(|change| change.entity_id == loser.id)
            .unwrap();
        let loser_millis = revision_millis(&loser_change.revision).unwrap();

        let mut tunnel = TunnelConfig {
            id: String::new(),
            server_id: loser.id.clone(),
            tunnel_type: TunnelType::Local,
            local_host: "127.0.0.1".to_string(),
            local_port: 8080,
            remote_host: Some("internal.example.com".to_string()),
            remote_port: Some(80),
            auto_start: false,
            enabled: true,
        };
        database.tunnel_config_add(&mut tunnel).unwrap();

        let mut recording = Recording {
            id: String::new(),
            session_id: "session-loser".to_string(),
            server_id: loser.id.clone(),
            started_at: 10,
            ended_at: Some(20),
            file_path: "/device-only/recording.cast".to_string(),
            sync_status: SyncStatus::Local,
        };
        database.recording_add(&mut recording).unwrap();
        database
            .credential_save(
                &loser.name,
                "password",
                "device-only-secret",
                Some("device-only-passphrase"),
                Some("/device-only/id_ed25519"),
            )
            .unwrap();

        let winner = remote_upsert(
            SyncEntityKind::Server,
            "server-winner",
            loser_millis + 1,
            json!({
                "name": "Shared Server",
                "host": "winner.example.com",
                "port": 22,
                "username": "root",
                "authType": "password",
                "groupId": null,
                "tags": [],
                "createdAt": 1,
                "updatedAt": 1,
                "jumpHostId": null,
                "postLoginCommand": null,
                "agentForwarding": false
            }),
        );

        database
            .cloud_sync()
            .apply_remote_batch("server-relations", std::slice::from_ref(&winner))
            .unwrap();
        let replay = database
            .cloud_sync()
            .apply_remote_batch("server-relations-replay", &[winner])
            .unwrap();
        assert_eq!(replay.applied, 0);
        assert_eq!(replay.ignored, 1);

        let winner = database.server_get("server-winner").unwrap().unwrap();
        assert_eq!(winner.name, "Shared Server");

        let loser = database.server_get(&loser.id).unwrap().unwrap();
        let expected_conflict_name = deterministic_conflict_name("Shared Server", &loser.id);
        assert_eq!(loser.name, expected_conflict_name);
        assert_eq!(loser.jump_host_id.as_deref(), Some(jump_host.id.as_str()));

        let tunnels = database.tunnel_config_list(&loser.id).unwrap();
        assert_eq!(tunnels.len(), 1);
        assert_eq!(tunnels[0].id, tunnel.id);
        let stored_recording = database.recording_get(&recording.id).unwrap().unwrap();
        assert_eq!(stored_recording.server_id, loser.id);

        assert!(database.credential_get("Shared Server").unwrap().is_none());
        let credential = database
            .credential_get(&expected_conflict_name)
            .unwrap()
            .unwrap();
        assert_eq!(credential.credential, "device-only-secret");
        assert_eq!(
            credential.passphrase.as_deref(),
            Some("device-only-passphrase")
        );
        assert_eq!(
            credential.key_path.as_deref(),
            Some("/device-only/id_ed25519")
        );

        let loser_outbox = pending(&database)
            .into_iter()
            .find(|change| change.entity_id == loser.id)
            .unwrap();
        assert_eq!(
            loser_outbox
                .payload
                .as_ref()
                .and_then(|payload| payload.get("name"))
                .and_then(Value::as_str),
            Some(expected_conflict_name.as_str())
        );
    }

    #[test]
    fn occupied_credential_conflict_name_rolls_back_the_remote_batch() {
        let (_dir, database) = test_database();
        let mut loser = server("Shared Server");
        database.server_add(&mut loser).unwrap();
        let loser_change = pending(&database).pop().unwrap();
        let loser_millis = revision_millis(&loser_change.revision).unwrap();
        let conflict_name = deterministic_conflict_name(&loser.name, &loser.id);

        database
            .credential_save(&loser.name, "password", "loser-secret", None, None)
            .unwrap();
        database
            .credential_save(&conflict_name, "password", "occupied-secret", None, None)
            .unwrap();
        database
            .cloud_sync()
            .apply_remote_batch("credential-collision-before", &[])
            .unwrap();

        let winner = remote_upsert(
            SyncEntityKind::Server,
            "server-winner",
            loser_millis + 1,
            json!({
                "name": "Shared Server",
                "host": "winner.example.com",
                "port": 22,
                "username": "root",
                "authType": "password",
                "groupId": null,
                "tags": [],
                "createdAt": 1,
                "updatedAt": 1,
                "jumpHostId": null,
                "postLoginCommand": null,
                "agentForwarding": false
            }),
        );

        assert!(database
            .cloud_sync()
            .apply_remote_batch("credential-collision-after", &[winner])
            .is_err());
        assert_eq!(
            database
                .cloud_sync()
                .current_remote_cursor()
                .unwrap()
                .as_deref(),
            Some("credential-collision-before")
        );
        assert_eq!(
            database.server_get(&loser.id).unwrap().unwrap().name,
            "Shared Server"
        );
        assert!(database.server_get("server-winner").unwrap().is_none());
        assert_eq!(
            database
                .credential_get("Shared Server")
                .unwrap()
                .unwrap()
                .credential,
            "loser-secret"
        );
        assert_eq!(
            database
                .credential_get(&conflict_name)
                .unwrap()
                .unwrap()
                .credential,
            "occupied-secret"
        );
        assert_eq!(
            pending(&database)[0]
                .payload
                .as_ref()
                .and_then(|payload| payload.get("name"))
                .and_then(Value::as_str),
            Some("Shared Server")
        );
    }

    #[test]
    fn existing_incoming_server_loser_moves_credential_from_its_previous_name() {
        let (_dir, database) = test_database();
        let loser = remote_upsert(
            SyncEntityKind::Server,
            "server-loser",
            100,
            json!({
                "name": "Previous Name",
                "host": "loser.example.com",
                "port": 22,
                "username": "root",
                "authType": "password",
                "groupId": null,
                "tags": [],
                "createdAt": 1,
                "updatedAt": 1,
                "jumpHostId": null,
                "postLoginCommand": null,
                "agentForwarding": false
            }),
        );
        let winner = remote_upsert(
            SyncEntityKind::Server,
            "server-winner",
            300,
            json!({
                "name": "Shared Server",
                "host": "winner.example.com",
                "port": 22,
                "username": "root",
                "authType": "password",
                "groupId": null,
                "tags": [],
                "createdAt": 1,
                "updatedAt": 1,
                "jumpHostId": null,
                "postLoginCommand": null,
                "agentForwarding": false
            }),
        );
        database
            .cloud_sync()
            .apply_remote_batch("existing-loser-seed", &[loser.clone(), winner])
            .unwrap();
        database
            .credential_save(
                "Previous Name",
                "password",
                "device-only-secret",
                None,
                None,
            )
            .unwrap();

        let mut rename = loser;
        rename.change_id = Uuid::new_v4().to_string();
        rename.revision = format!("{:020}:remote-device", 200);
        rename.payload.as_mut().unwrap()["name"] = json!("Shared Server");
        database
            .cloud_sync()
            .apply_remote_batch("existing-loser-rename", &[rename.clone()])
            .unwrap();
        database
            .cloud_sync()
            .apply_remote_batch("existing-loser-replay", &[rename])
            .unwrap();

        let conflict_name = deterministic_conflict_name("Shared Server", "server-loser");
        assert_eq!(
            database.server_get("server-loser").unwrap().unwrap().name,
            conflict_name
        );
        assert!(database.credential_get("Previous Name").unwrap().is_none());
        assert_eq!(
            database
                .credential_get(&conflict_name)
                .unwrap()
                .unwrap()
                .credential,
            "device-only-secret"
        );
    }

    #[test]
    fn delete_and_update_races_follow_revision_order_on_every_replica() {
        let (_source_dir, source) = test_database();
        let mut delete_wins = server("delete-wins");
        let mut update_wins = server("update-wins");
        source.server_add(&mut delete_wins).unwrap();
        source.server_add(&mut update_wins).unwrap();
        let seeds = pending(&source);

        let seed_delete_wins = seeds
            .iter()
            .find(|change| change.entity_id == delete_wins.id)
            .unwrap();
        let seed_update_wins = seeds
            .iter()
            .find(|change| change.entity_id == update_wins.id)
            .unwrap();

        let delete_wins_base = revision_millis(&seed_delete_wins.revision).unwrap();
        let mut delete_wins_payload = seed_delete_wins.payload.clone().unwrap();
        delete_wins_payload["host"] = json!("updated-before-delete.example.com");
        let older_update = remote_upsert(
            SyncEntityKind::Server,
            &delete_wins.id,
            delete_wins_base + 1,
            delete_wins_payload,
        );
        let newer_delete = remote_delete(
            SyncEntityKind::Server,
            &delete_wins.id,
            delete_wins_base + 2,
        );

        let update_wins_base = revision_millis(&seed_update_wins.revision).unwrap();
        let older_delete = remote_delete(
            SyncEntityKind::Server,
            &update_wins.id,
            update_wins_base + 1,
        );
        let mut update_wins_payload = seed_update_wins.payload.clone().unwrap();
        update_wins_payload["host"] = json!("restored-by-update.example.com");
        let newer_update = remote_upsert(
            SyncEntityKind::Server,
            &update_wins.id,
            update_wins_base + 2,
            update_wins_payload,
        );

        let (_replica_dir_a, replica_a) = test_database();
        let (_replica_dir_b, replica_b) = test_database();
        replica_a
            .cloud_sync()
            .apply_remote_batch("race-seed-a", &seeds)
            .unwrap();
        replica_b
            .cloud_sync()
            .apply_remote_batch("race-seed-b", &seeds)
            .unwrap();

        let order_a = [&newer_delete, &older_update, &newer_update, &older_delete];
        let order_b = [&older_delete, &newer_update, &older_update, &newer_delete];
        apply_changes_in_arrival_order(&replica_a, "race-a", &order_a);
        apply_changes_in_arrival_order(&replica_b, "race-b", &order_b);

        for replica in [&replica_a, &replica_b] {
            assert!(replica.server_get(&delete_wins.id).unwrap().is_none());
            let restored = replica.server_get(&update_wins.id).unwrap().unwrap();
            assert_eq!(restored.host, "restored-by-update.example.com");
            assert!(pending(replica).is_empty());
        }
    }

    #[test]
    fn concurrent_server_renames_to_same_name_converge_across_two_devices() {
        let (_origin_dir, origin) = test_database();
        let mut alpha = server("alpha");
        let mut beta = server("beta");
        origin.server_add(&mut alpha).unwrap();
        origin.server_add(&mut beta).unwrap();
        let seed = pending(&origin);

        let (_dir_a, device_a) = test_database();
        let (_dir_b, device_b) = test_database();
        device_a
            .cloud_sync()
            .apply_remote_batch("rename-seed-a", &seed)
            .unwrap();
        device_b
            .cloud_sync()
            .apply_remote_batch("rename-seed-b", &seed)
            .unwrap();

        let mut alpha_on_a = device_a.server_get(&alpha.id).unwrap().unwrap();
        alpha_on_a.name = "shared-rename".to_string();
        device_a.server_update(&alpha_on_a).unwrap();
        let change_a = pending(&device_a).pop().unwrap();

        let mut beta_on_b = device_b.server_get(&beta.id).unwrap().unwrap();
        beta_on_b.name = "shared-rename".to_string();
        device_b.server_update(&beta_on_b).unwrap();
        let change_b = pending(&device_b).pop().unwrap();
        let winner_id = winning_change(&change_a, &change_b).entity_id.clone();
        let loser_id = if winner_id == change_a.entity_id {
            change_b.entity_id.clone()
        } else {
            change_a.entity_id.clone()
        };

        let report_a = device_a
            .cloud_sync()
            .apply_remote_batch("rename-cross-a", &[change_b])
            .unwrap();
        let report_b = device_b
            .cloud_sync()
            .apply_remote_batch("rename-cross-b", &[change_a])
            .unwrap();

        for database in [&device_a, &device_b] {
            let servers = database.server_list(None, None).unwrap();
            assert_eq!(servers.len(), 2);
            assert_eq!(
                servers
                    .iter()
                    .find(|server| server.id == winner_id)
                    .unwrap()
                    .name,
                "shared-rename"
            );
            assert_eq!(
                servers
                    .iter()
                    .find(|server| server.id == loser_id)
                    .unwrap()
                    .name,
                deterministic_conflict_name("shared-rename", &loser_id)
            );
        }
        assert!(!report_a.conflicts.is_empty());
        assert!(!report_b.conflicts.is_empty());
    }

    #[test]
    fn remote_conflicts_use_revision_order_and_preserve_local_credentials() {
        let (_dir, database) = test_database();
        let mut server = server("conflicted");
        database.server_add(&mut server).unwrap();

        let local = pending(&database).pop().unwrap();
        let local_millis = revision_millis(&local.revision).unwrap();
        let mut older_payload = local.payload.clone().unwrap();
        older_payload["host"] = json!("stale.example.com");
        let older = remote_upsert(
            SyncEntityKind::Server,
            &server.id,
            local_millis - 1,
            older_payload,
        );

        let local_wins = database
            .cloud_sync()
            .apply_remote_batch("conflict-older", &[older])
            .unwrap();
        assert_eq!(local_wins.applied, 0);
        assert_eq!(local_wins.ignored, 1);
        assert_eq!(local_wins.conflicts.len(), 1);
        assert_eq!(
            local_wins.conflicts[0].resolution,
            ConflictResolution::LocalKept
        );
        assert_eq!(
            database.server_get(&server.id).unwrap().unwrap().host,
            server.host
        );

        let mut newer_payload = local.payload.unwrap();
        newer_payload["host"] = json!("remote.example.com");
        let newer = remote_upsert(
            SyncEntityKind::Server,
            &server.id,
            local_millis + 10,
            newer_payload,
        );
        let remote_wins = database
            .cloud_sync()
            .apply_remote_batch("conflict-newer", &[newer])
            .unwrap();
        assert_eq!(remote_wins.applied, 1);
        assert_eq!(remote_wins.conflicts.len(), 1);
        assert_eq!(
            remote_wins.conflicts[0].resolution,
            ConflictResolution::RemoteApplied
        );

        let stored = database.server_get(&server.id).unwrap().unwrap();
        assert_eq!(stored.host, "remote.example.com");
        assert_eq!(
            stored.credential_id.as_deref(),
            Some("device-local-credential")
        );
        assert!(pending(&database).is_empty());

        let tombstone = remote_delete(SyncEntityKind::Server, &server.id, local_millis + 20);
        let deleted = database
            .cloud_sync()
            .apply_remote_batch("conflict-delete", std::slice::from_ref(&tombstone))
            .unwrap();
        assert_eq!(deleted.applied, 1);
        assert!(database.server_get(&server.id).unwrap().is_none());

        let duplicate = database
            .cloud_sync()
            .apply_remote_batch("conflict-delete", &[tombstone])
            .unwrap();
        assert_eq!(duplicate.applied, 0);
        assert_eq!(duplicate.ignored, 1);
    }

    #[test]
    fn far_future_revision_rolls_back_domain_cursor_and_revision_clock() {
        let (_dir, database) = test_database();
        database
            .cloud_sync()
            .apply_remote_batch("future-baseline-cursor", &[])
            .unwrap();
        let clock_before = {
            let conn = database.conn.lock().unwrap();
            metadata_get(&conn, LAST_REVISION_MILLIS_KEY).unwrap()
        };

        let valid = remote_upsert(
            SyncEntityKind::Group,
            "valid-before-future",
            1,
            json!({"name": "Valid", "parentId": null, "color": "#ffffff"}),
        );
        let far_future_millis =
            Utc::now().timestamp_millis() + MAX_REMOTE_CLOCK_SKEW_MILLIS + 60_000;
        let future = remote_upsert(
            SyncEntityKind::Group,
            "far-future",
            far_future_millis,
            json!({"name": "Future", "parentId": null, "color": "#ffffff"}),
        );

        let error = database
            .cloud_sync()
            .apply_remote_batch("future-rejected-cursor", &[valid, future])
            .unwrap_err();
        assert!(error.to_string().contains("implausibly far in the future"));
        assert!(database.group_list().unwrap().is_empty());
        assert!(pending(&database).is_empty());
        assert_eq!(
            database.cloud_sync().current_remote_cursor().unwrap(),
            Some("future-baseline-cursor".to_string())
        );
        let clock_after = {
            let conn = database.conn.lock().unwrap();
            metadata_get(&conn, LAST_REVISION_MILLIS_KEY).unwrap()
        };
        assert_eq!(clock_after, clock_before);
    }

    #[test]
    fn local_revision_clock_uses_final_i64_value_then_rolls_back_exhausted_write() {
        let (_dir, database) = test_database();
        {
            let conn = database.conn.lock().unwrap();
            metadata_set(&conn, LAST_REVISION_MILLIS_KEY, &(i64::MAX - 1).to_string()).unwrap();
        }

        let mut final_group = group("Final revision");
        database.group_add(&mut final_group).unwrap();
        let final_batch = pending(&database);
        assert_eq!(final_batch.len(), 1);
        assert_eq!(revision_millis(&final_batch[0].revision).unwrap(), i64::MAX);

        let mut exhausted_group = group("Exhausted revision");
        let error = database.group_add(&mut exhausted_group).unwrap_err();
        assert!(error.to_string().contains("revision clock is exhausted"));

        let groups = database.group_list().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, final_group.id);
        let remaining_batch = pending(&database);
        assert_eq!(remaining_batch.len(), 1);
        assert_eq!(remaining_batch[0].entity_id, final_group.id);
        let stored_clock = {
            let conn = database.conn.lock().unwrap();
            metadata_get(&conn, LAST_REVISION_MILLIS_KEY).unwrap()
        };
        assert_eq!(stored_clock, Some(i64::MAX.to_string()));
    }

    #[test]
    fn remote_future_revisions_and_local_clock_saturation_are_rejected() {
        let (_dir, database) = test_database();
        let too_far_future = Utc::now().timestamp_millis() + MAX_REMOTE_CLOCK_SKEW_MILLIS + 1;
        let future = remote_upsert(
            SyncEntityKind::Group,
            "future-group",
            too_far_future,
            json!({"name": "Future", "parentId": null, "color": "#ffffff"}),
        );
        assert!(database
            .cloud_sync()
            .apply_remote_batch("future-cursor", &[future])
            .is_err());
        assert!(database.group_list().unwrap().is_empty());
        assert_eq!(database.cloud_sync().current_remote_cursor().unwrap(), None);

        let mut invalid_device = remote_upsert(
            SyncEntityKind::Group,
            "bad-device-group",
            1,
            json!({"name": "Bad Device", "parentId": null, "color": "#ffffff"}),
        );
        invalid_device.revision = format!("{:020}:bad device", 1);
        assert!(database
            .cloud_sync()
            .apply_remote_batch("bad-device-cursor", &[invalid_device])
            .is_err());

        {
            let conn = database.conn.lock().unwrap();
            metadata_set(&conn, LAST_REVISION_MILLIS_KEY, &i64::MAX.to_string()).unwrap();
        }
        let mut exhausted = group("Exhausted");
        assert!(database.group_add(&mut exhausted).is_err());
        assert!(database.group_list().unwrap().is_empty());
        assert!(pending(&database).is_empty());
    }

    #[test]
    fn prior_sync_state_is_backfilled_with_canonical_content_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prior-sync.db");
        create_legacy_database(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE sync_metadata (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                CREATE TABLE sync_entity_state (
                    entity_kind TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    revision TEXT NOT NULL,
                    is_tombstone INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (entity_kind, entity_id)
                );
                INSERT INTO sync_metadata (key, value) VALUES
                    ('device_id', 'legacy-device'),
                    ('last_revision_millis', '1');
                INSERT INTO sync_entity_state
                    (entity_kind, entity_id, revision, is_tombstone)
                    VALUES ('server', 'legacy-server',
                            '00000000000000000001:legacy-device', 0);
                "#,
            )
            .unwrap();
        }

        let database = Database::new_at(&path).unwrap();
        let payload = {
            let conn = database.conn.lock().unwrap();
            let hash: String = conn
                .query_row(
                    r#"SELECT content_hash FROM sync_entity_state
                       WHERE entity_kind = 'server' AND entity_id = 'legacy-server'"#,
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(hash.len(), 64);
            current_domain_payload(&conn, SyncEntityKind::Server, "legacy-server")
                .unwrap()
                .unwrap()
        };
        let mut replay = remote_upsert(SyncEntityKind::Server, "legacy-server", 1, payload);
        replay.revision = "00000000000000000001:legacy-device".to_string();
        let report = database
            .cloud_sync()
            .apply_remote_batch("legacy-hash-cursor", &[replay])
            .unwrap();
        assert_eq!(report.applied, 0);
        assert_eq!(report.ignored, 1);
    }

    #[test]
    fn existing_rows_are_bootstrapped_once_and_acknowledgement_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        create_legacy_database(&path);

        let (device_id, change_ids) = {
            let database = Database::new_at(&path).unwrap();
            let batch = database.cloud_sync().export_pending_changes(100).unwrap();
            assert_eq!(batch.changes.len(), 3);
            let kinds = batch
                .changes
                .iter()
                .map(|change| change.entity_kind)
                .collect::<HashSet<_>>();
            assert_eq!(
                kinds,
                HashSet::from([
                    SyncEntityKind::Server,
                    SyncEntityKind::Group,
                    SyncEntityKind::CommandSnippet,
                ])
            );
            (
                batch.device_id,
                batch
                    .changes
                    .into_iter()
                    .map(|change| change.change_id)
                    .collect::<Vec<_>>(),
            )
        };

        {
            let database = Database::new_at(&path).unwrap();
            let batch = database.cloud_sync().export_pending_changes(100).unwrap();
            assert_eq!(batch.device_id, device_id);
            assert_eq!(batch.changes.len(), 3);
            assert_eq!(
                database
                    .cloud_sync()
                    .acknowledge_changes(&change_ids)
                    .unwrap(),
                3
            );
        }

        let database = Database::new_at(&path).unwrap();
        assert!(pending(&database).is_empty());
    }

    #[test]
    fn foreign_keys_are_enabled_and_server_delete_cascades_local_tunnels() {
        let (_dir, database) = test_database();
        let mut server = server("tunnel-host");
        database.server_add(&mut server).unwrap();
        let mut tunnel = TunnelConfig {
            id: String::new(),
            server_id: server.id.clone(),
            tunnel_type: TunnelType::Local,
            local_host: "127.0.0.1".to_string(),
            local_port: 8080,
            remote_host: Some("127.0.0.1".to_string()),
            remote_port: Some(80),
            auto_start: false,
            enabled: true,
        };
        database.tunnel_config_add(&mut tunnel).unwrap();
        assert_eq!(database.tunnel_config_list(&server.id).unwrap().len(), 1);

        database.server_delete(&server.id).unwrap();
        assert!(database.tunnel_config_list(&server.id).unwrap().is_empty());
    }

    #[test]
    fn local_group_delete_detaches_dependents_and_records_their_upserts() {
        let (_dir, database) = test_database();
        let mut parent = group("Parent");
        database.group_add(&mut parent).unwrap();
        let mut child = group("Child");
        child.parent_id = Some(parent.id.clone());
        database.group_add(&mut child).unwrap();
        let mut member = server("member");
        member.group_id = Some(parent.id.clone());
        database.server_add(&mut member).unwrap();
        acknowledge_all(&database);

        database.group_delete(&parent.id).unwrap();

        let groups = database.group_list().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, child.id);
        assert_eq!(groups[0].parent_id, None);
        assert_eq!(
            database.server_get(&member.id).unwrap().unwrap().group_id,
            None
        );

        let changes = pending(&database);
        assert_eq!(changes.len(), 3);
        let child_change = changes
            .iter()
            .find(|change| change.entity_id == child.id)
            .unwrap();
        assert!(!child_change.deleted);
        assert_eq!(
            child_change.payload.as_ref().unwrap()["parentId"],
            Value::Null
        );
        let member_change = changes
            .iter()
            .find(|change| change.entity_id == member.id)
            .unwrap();
        assert!(!member_change.deleted);
        assert_eq!(
            member_change.payload.as_ref().unwrap()["groupId"],
            Value::Null
        );
        let tombstone = changes
            .iter()
            .find(|change| change.entity_id == parent.id)
            .unwrap();
        assert!(tombstone.deleted);
        assert!(child_change.revision < tombstone.revision);
        assert!(member_change.revision < tombstone.revision);
    }

    #[test]
    fn local_server_delete_detaches_jump_hosts_cascades_tunnels_and_keeps_recordings() {
        let (_dir, database) = test_database();
        let mut jump_host = server("jump");
        database.server_add(&mut jump_host).unwrap();
        let mut dependent = server("dependent");
        dependent.jump_host_id = Some(jump_host.id.clone());
        database.server_add(&mut dependent).unwrap();
        let mut tunnel = TunnelConfig {
            id: String::new(),
            server_id: jump_host.id.clone(),
            tunnel_type: TunnelType::Local,
            local_host: "127.0.0.1".to_string(),
            local_port: 8080,
            remote_host: Some("127.0.0.1".to_string()),
            remote_port: Some(80),
            auto_start: false,
            enabled: true,
        };
        database.tunnel_config_add(&mut tunnel).unwrap();
        let mut recording = Recording {
            id: String::new(),
            session_id: "session-jump".to_string(),
            server_id: jump_host.id.clone(),
            started_at: 10,
            ended_at: Some(20),
            file_path: "/device-only/jump.cast".to_string(),
            sync_status: SyncStatus::Local,
        };
        database.recording_add(&mut recording).unwrap();
        acknowledge_all(&database);

        database.server_delete(&jump_host.id).unwrap();

        assert_eq!(
            database
                .server_get(&dependent.id)
                .unwrap()
                .unwrap()
                .jump_host_id,
            None
        );
        assert!(database
            .tunnel_config_list(&jump_host.id)
            .unwrap()
            .is_empty());
        assert_eq!(
            database
                .recording_get(&recording.id)
                .unwrap()
                .unwrap()
                .server_id,
            jump_host.id
        );

        let changes = pending(&database);
        assert_eq!(changes.len(), 2);
        let dependent_change = changes
            .iter()
            .find(|change| change.entity_id == dependent.id)
            .unwrap();
        assert!(!dependent_change.deleted);
        assert_eq!(
            dependent_change.payload.as_ref().unwrap()["jumpHostId"],
            Value::Null
        );
        let tombstone = changes
            .iter()
            .find(|change| change.entity_id == jump_host.id)
            .unwrap();
        assert!(tombstone.deleted);
        assert!(dependent_change.revision < tombstone.revision);
    }

    #[test]
    fn remote_deletes_detach_dependents_without_echoing_changes() {
        let (_dir, database) = test_database();
        let mut parent = group("Remote Parent");
        database.group_add(&mut parent).unwrap();
        let mut child = group("Remote Child");
        child.parent_id = Some(parent.id.clone());
        database.group_add(&mut child).unwrap();
        let mut member = server("remote-member");
        member.group_id = Some(parent.id.clone());
        database.server_add(&mut member).unwrap();
        let mut jump_host = server("remote-jump");
        database.server_add(&mut jump_host).unwrap();
        let mut dependent = server("remote-dependent");
        dependent.jump_host_id = Some(jump_host.id.clone());
        database.server_add(&mut dependent).unwrap();
        let mut tunnel = TunnelConfig {
            id: String::new(),
            server_id: jump_host.id.clone(),
            tunnel_type: TunnelType::Local,
            local_host: "127.0.0.1".to_string(),
            local_port: 9080,
            remote_host: Some("127.0.0.1".to_string()),
            remote_port: Some(90),
            auto_start: false,
            enabled: true,
        };
        database.tunnel_config_add(&mut tunnel).unwrap();
        let mut recording = Recording {
            id: String::new(),
            session_id: "session-remote-jump".to_string(),
            server_id: jump_host.id.clone(),
            started_at: 10,
            ended_at: Some(20),
            file_path: "/device-only/remote-jump.cast".to_string(),
            sync_status: SyncStatus::Local,
        };
        database.recording_add(&mut recording).unwrap();

        let base_revision = pending(&database)
            .iter()
            .map(|change| revision_millis(&change.revision).unwrap())
            .max()
            .unwrap();
        acknowledge_all(&database);
        database
            .cloud_sync()
            .apply_remote_batch(
                "remote-related-deletes",
                &[
                    remote_delete(SyncEntityKind::Group, &parent.id, base_revision + 1),
                    remote_delete(SyncEntityKind::Server, &jump_host.id, base_revision + 2),
                ],
            )
            .unwrap();

        assert_eq!(database.group_list().unwrap()[0].parent_id, None);
        assert_eq!(
            database.server_get(&member.id).unwrap().unwrap().group_id,
            None
        );
        assert_eq!(
            database
                .server_get(&dependent.id)
                .unwrap()
                .unwrap()
                .jump_host_id,
            None
        );
        assert!(database
            .tunnel_config_list(&jump_host.id)
            .unwrap()
            .is_empty());
        assert_eq!(
            database
                .recording_get(&recording.id)
                .unwrap()
                .unwrap()
                .server_id,
            jump_host.id
        );
        assert!(pending(&database).is_empty());
    }

    #[test]
    fn relationship_upserts_cannot_restore_references_to_tombstoned_entities() {
        let (_origin_dir, origin) = test_database();
        let mut parent = group("Ordered Parent");
        origin.group_add(&mut parent).unwrap();
        let mut child = group("Ordered Child");
        child.parent_id = Some(parent.id.clone());
        origin.group_add(&mut child).unwrap();
        let mut member = server("ordered-member");
        member.group_id = Some(parent.id.clone());
        origin.server_add(&mut member).unwrap();
        let mut jump_host = server("ordered-jump");
        origin.server_add(&mut jump_host).unwrap();
        let mut dependent = server("ordered-dependent");
        dependent.jump_host_id = Some(jump_host.id.clone());
        origin.server_add(&mut dependent).unwrap();
        let seeds = pending(&origin);
        let base_revision = seeds
            .iter()
            .map(|change| revision_millis(&change.revision).unwrap())
            .max()
            .unwrap();

        let child_update = remote_upsert(
            SyncEntityKind::Group,
            &child.id,
            base_revision + 2,
            seeds
                .iter()
                .find(|change| change.entity_id == child.id)
                .unwrap()
                .payload
                .clone()
                .unwrap(),
        );
        let member_update = remote_upsert(
            SyncEntityKind::Server,
            &member.id,
            base_revision + 3,
            seeds
                .iter()
                .find(|change| change.entity_id == member.id)
                .unwrap()
                .payload
                .clone()
                .unwrap(),
        );
        let dependent_update = remote_upsert(
            SyncEntityKind::Server,
            &dependent.id,
            base_revision + 5,
            seeds
                .iter()
                .find(|change| change.entity_id == dependent.id)
                .unwrap()
                .payload
                .clone()
                .unwrap(),
        );
        let group_delete = remote_delete(SyncEntityKind::Group, &parent.id, base_revision + 1);
        let jump_delete = remote_delete(SyncEntityKind::Server, &jump_host.id, base_revision + 4);

        let (_dir_a, replica_a) = test_database();
        let (_dir_b, replica_b) = test_database();
        replica_a
            .cloud_sync()
            .apply_remote_batch("relation-seeds-a", &seeds)
            .unwrap();
        replica_b
            .cloud_sync()
            .apply_remote_batch("relation-seeds-b", &seeds)
            .unwrap();

        apply_changes_in_arrival_order(
            &replica_a,
            "relations-a",
            &[
                &child_update,
                &member_update,
                &dependent_update,
                &group_delete,
                &jump_delete,
            ],
        );
        apply_changes_in_arrival_order(
            &replica_b,
            "relations-b",
            &[
                &group_delete,
                &jump_delete,
                &child_update,
                &member_update,
                &dependent_update,
            ],
        );

        for replica in [&replica_a, &replica_b] {
            let child = replica
                .group_list()
                .unwrap()
                .into_iter()
                .find(|group| group.id == child.id)
                .unwrap();
            assert_eq!(child.parent_id, None);
            assert_eq!(
                replica.server_get(&member.id).unwrap().unwrap().group_id,
                None
            );
            assert_eq!(
                replica
                    .server_get(&dependent.id)
                    .unwrap()
                    .unwrap()
                    .jump_host_id,
                None
            );
            assert!(pending(replica).is_empty());
        }
    }

    #[test]
    fn remote_cursors_are_vault_scoped_without_changing_the_legacy_cursor() {
        let (_dir, database) = test_database();

        database
            .cloud_sync()
            .apply_remote_batch_for_vault("vault-a", "cursor-a", &[])
            .unwrap();
        database
            .cloud_sync()
            .apply_remote_batch_for_vault("vault-b", "cursor-b", &[])
            .unwrap();

        assert_eq!(
            database
                .cloud_sync()
                .current_remote_cursor_for_vault("vault-a")
                .unwrap(),
            Some("cursor-a".to_string())
        );
        assert_eq!(
            database
                .cloud_sync()
                .current_remote_cursor_for_vault("vault-b")
                .unwrap(),
            Some("cursor-b".to_string())
        );
        assert_eq!(database.cloud_sync().current_remote_cursor().unwrap(), None);

        database
            .cloud_sync()
            .apply_remote_batch("legacy-cursor", &[])
            .unwrap();
        assert_eq!(
            database.cloud_sync().current_remote_cursor().unwrap(),
            Some("legacy-cursor".to_string())
        );
        assert_eq!(
            database
                .cloud_sync()
                .current_remote_cursor_for_vault("vault-a")
                .unwrap(),
            Some("cursor-a".to_string())
        );
    }

    #[test]
    fn pending_upload_is_idempotent_and_acknowledged_atomically() {
        let (_dir, database) = test_database();
        for name in ["Upload One", "Upload Two", "Upload Three"] {
            let mut group = group(name);
            database.group_add(&mut group).unwrap();
        }
        let changes = pending(&database);
        let upload = PendingSyncUpload {
            vault_id: "vault-upload".to_string(),
            envelope_id: "envelope-1".to_string(),
            ciphertext: "encrypted-envelope-json".to_string(),
            change_ids: changes[..2]
                .iter()
                .map(|change| change.change_id.clone())
                .collect(),
        };

        database.cloud_sync().save_pending_upload(&upload).unwrap();
        database.cloud_sync().save_pending_upload(&upload).unwrap();
        assert_eq!(
            database
                .cloud_sync()
                .load_pending_upload(&upload.vault_id)
                .unwrap(),
            Some(upload.clone())
        );

        let mut conflicting = upload.clone();
        conflicting.envelope_id = "envelope-2".to_string();
        assert!(database
            .cloud_sync()
            .save_pending_upload(&conflicting)
            .is_err());
        assert!(database
            .cloud_sync()
            .acknowledge_pending_upload(&upload.vault_id, "envelope-wrong")
            .is_err());
        assert_eq!(pending(&database).len(), 3);

        assert_eq!(
            database
                .cloud_sync()
                .acknowledge_pending_upload(&upload.vault_id, &upload.envelope_id)
                .unwrap(),
            2
        );
        assert_eq!(
            database
                .cloud_sync()
                .load_pending_upload(&upload.vault_id)
                .unwrap(),
            None
        );
        assert_eq!(pending(&database).len(), 1);
        assert_eq!(
            database
                .cloud_sync()
                .acknowledge_pending_upload(&upload.vault_id, &upload.envelope_id)
                .unwrap(),
            0
        );
    }

    #[test]
    fn activating_a_vault_rebuilds_live_records_and_tombstones_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault-activation.db");
        let database = Database::new_at(&path).unwrap();
        let mut group = group("Vault Group");
        database.group_add(&mut group).unwrap();
        let mut live_server = server("vault-live");
        live_server.group_id = Some(group.id.clone());
        database.server_add(&mut live_server).unwrap();
        let mut deleted_server = server("vault-deleted");
        database.server_add(&mut deleted_server).unwrap();
        database.server_delete(&deleted_server.id).unwrap();
        let mut snippet = snippet("vault-snippet");
        database.snippet_add(&mut snippet).unwrap();
        acknowledge_all(&database);

        assert!(database.cloud_sync().activate_vault("vault-a").unwrap());
        assert_eq!(
            database.cloud_sync().active_vault_id().unwrap(),
            Some("vault-a".to_string())
        );
        let first_snapshot = pending(&database);
        assert_eq!(first_snapshot.len(), 4);
        assert!(first_snapshot
            .iter()
            .any(|change| change.entity_id == deleted_server.id && change.deleted));
        assert!(first_snapshot
            .iter()
            .any(|change| change.entity_id == live_server.id && !change.deleted));
        assert!(first_snapshot
            .iter()
            .any(|change| change.entity_id == group.id && !change.deleted));
        assert!(first_snapshot
            .iter()
            .any(|change| change.entity_id == snippet.id && !change.deleted));
        acknowledge_all(&database);

        assert!(!database.cloud_sync().activate_vault("vault-a").unwrap());
        assert!(pending(&database).is_empty());
        assert!(database.cloud_sync().activate_vault("vault-b").unwrap());
        let second_snapshot = pending(&database);
        assert_eq!(second_snapshot.len(), 4);
        assert!(second_snapshot
            .iter()
            .any(|change| change.entity_id == deleted_server.id && change.deleted));
        assert!(database.cloud_sync().activate_vault("bad vault").is_err());
        drop(database);

        let reopened = Database::new_at(&path).unwrap();
        assert_eq!(
            reopened.cloud_sync().active_vault_id().unwrap(),
            Some("vault-b".to_string())
        );
        assert_eq!(pending(&reopened).len(), 4);
    }

    fn create_legacy_database(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                username TEXT NOT NULL,
                auth_type TEXT NOT NULL,
                credential_id TEXT,
                group_id TEXT,
                tags TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                jump_host_id TEXT,
                post_login_command TEXT,
                agent_forwarding INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                parent_id TEXT,
                color TEXT NOT NULL DEFAULT '#808080'
            );
            CREATE TABLE command_snippets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                command TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            INSERT INTO groups (id, name, color)
                VALUES ('legacy-group', 'Legacy', '#808080');
            INSERT INTO servers
                (id, name, host, port, username, auth_type, credential_id, group_id,
                 tags, created_at, updated_at, agent_forwarding)
                VALUES ('legacy-server', 'Legacy Server', 'legacy.example.com', 22, 'root',
                        'password', 'local-secret', 'legacy-group', '[]', 1, 1, 0);
            INSERT INTO command_snippets
                (id, name, command, category, description, tags, created_at, updated_at)
                VALUES ('legacy-snippet', 'Legacy Snippet', 'uptime', '', '', '[]', 1, 1);
            "#,
        )
        .unwrap();
    }
}
