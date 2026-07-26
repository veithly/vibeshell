use std::sync::{Arc, Mutex, RwLock};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::sync_crypto::{
    decrypt_batch, encrypt_batch, EncryptedSyncBatch, SyncBatchContext, VaultKey,
};
use crate::storage::{Database, PendingSyncBatch, PendingSyncUpload, SYNC_CHANGE_SCHEMA_VERSION};

mod portable_file;
mod providers;

pub use portable_file::{CloudSyncFileOperation, CloudSyncFileReport};
pub use providers::{MultiSyncTransport, SyncProviderConfig, SyncProviderKind};

const PAIRING_PREFIX: &str = "vibeshell-sync-v2.";
const PAIRING_VERSION: u32 = 2;
const MAX_OUTBOX_BATCH: usize = 256;
const MAX_SYNC_ROUNDS: usize = 64;
const MAX_REMOTE_CHANGES_PER_PAGE: usize = 10_000;
const MAX_PAIRING_CODE_BYTES: usize = 16 * 1024;
const MAX_PROVIDER_CIPHERTEXT_BYTES: usize = 768 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncEnvelope {
    pub envelope_id: String,
    pub ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncExchangeRequest {
    pub cursor: Option<String>,
    pub envelope: Option<SyncEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncExchangeResponse {
    pub cursor: String,
    pub envelopes: Vec<SyncEnvelope>,
    pub has_more: bool,
}

#[async_trait]
pub trait SyncTransport: Send + Sync {
    async fn initialize(
        &self,
        config: &SyncProviderConfig,
        _vault_id: &str,
    ) -> Result<SyncProviderConfig> {
        Ok(config.clone())
    }

    async fn exchange(
        &self,
        config: &SyncProviderConfig,
        vault_id: &str,
        request: &SyncExchangeRequest,
    ) -> Result<SyncExchangeResponse>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncPairingInfo {
    pub provider: SyncProviderKind,
    pub endpoint: String,
    pub vault_id: String,
    pub pairing_code: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncStatus {
    pub unlocked: bool,
    pub syncing: bool,
    pub provider: Option<SyncProviderKind>,
    pub endpoint: Option<String>,
    pub vault_id: Option<String>,
    pub pending_changes: usize,
    pub conflicts: usize,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncReport {
    pub uploaded: usize,
    pub downloaded: usize,
    pub applied: usize,
    pub ignored: usize,
    pub conflicts: usize,
    pub pending_changes: usize,
    pub cursor: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairingBundle {
    version: u32,
    provider: SyncProviderConfig,
    vault_id: String,
    vault_key: String,
}

#[derive(Clone)]
struct UnlockedVault {
    provider: SyncProviderConfig,
    vault_id: String,
    key: Arc<VaultKey>,
}

#[derive(Default)]
struct RuntimeStatus {
    syncing: bool,
    last_success_at: Option<i64>,
    last_error: Option<String>,
    conflicts: usize,
}

struct PreparedUpload {
    envelope: SyncEnvelope,
}

pub struct CloudSyncManager {
    database: Arc<Database>,
    transport: Arc<dyn SyncTransport>,
    vault: RwLock<Option<UnlockedVault>>,
    runtime: Mutex<RuntimeStatus>,
    sync_lock: tokio::sync::Mutex<()>,
}

impl CloudSyncManager {
    pub fn new(database: Arc<Database>) -> Result<Self> {
        Ok(Self::with_transport(
            database,
            Arc::new(MultiSyncTransport::new()?),
        ))
    }

    pub fn with_transport(database: Arc<Database>, transport: Arc<dyn SyncTransport>) -> Self {
        Self {
            database,
            transport,
            vault: RwLock::new(None),
            runtime: Mutex::new(RuntimeStatus::default()),
            sync_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub async fn create_github_gist_vault(
        &self,
        gist_id: Option<String>,
        token: String,
    ) -> Result<CloudSyncPairingInfo> {
        self.create_vault_with_provider(SyncProviderConfig::GithubGist { gist_id, token })
            .await
    }

    pub async fn create_webdav_vault(
        &self,
        endpoint: String,
        username: String,
        password: String,
    ) -> Result<CloudSyncPairingInfo> {
        self.create_vault_with_provider(SyncProviderConfig::WebDav {
            endpoint: endpoint.trim().trim_end_matches('/').to_string(),
            username,
            password,
        })
        .await
    }

    async fn create_vault_with_provider(
        &self,
        provider: SyncProviderConfig,
    ) -> Result<CloudSyncPairingInfo> {
        let _guard = self.sync_lock.lock().await;
        provider.validate()?;
        let vault_id = Uuid::new_v4().to_string();
        let provider = self.transport.initialize(&provider, &vault_id).await?;
        let key = VaultKey::generate().map_err(|error| anyhow!(error.to_string()))?;
        let bundle = PairingBundle {
            version: PAIRING_VERSION,
            provider: provider.clone(),
            vault_id: vault_id.clone(),
            vault_key: key.export(),
        };
        let pairing_code = encode_pairing_bundle(&bundle)?;
        self.database
            .cloud_sync()
            .activate_vault(&bundle.vault_id)?;
        self.set_unlocked(bundle, key);
        Ok(CloudSyncPairingInfo {
            provider: provider.kind(),
            endpoint: provider.target(),
            vault_id,
            pairing_code,
        })
    }

    pub async fn join_vault(&self, pairing_code: &str) -> Result<CloudSyncPairingInfo> {
        let _guard = self.sync_lock.lock().await;
        let bundle = decode_pairing_bundle(pairing_code)?;
        bundle.provider.validate()?;
        if matches!(
            bundle.provider,
            SyncProviderConfig::GithubGist { gist_id: None, .. }
        ) {
            bail!("GitHub Gist pairing code does not contain a Gist ID");
        }
        validate_opaque_id("vault ID", &bundle.vault_id)?;
        let key =
            VaultKey::import(&bundle.vault_key).map_err(|error| anyhow!(error.to_string()))?;
        self.database
            .cloud_sync()
            .activate_vault(&bundle.vault_id)?;
        let info = CloudSyncPairingInfo {
            provider: bundle.provider.kind(),
            endpoint: bundle.provider.target(),
            vault_id: bundle.vault_id.clone(),
            pairing_code: encode_pairing_bundle(&bundle)?,
        };
        self.set_unlocked(bundle, key);
        Ok(info)
    }

    pub async fn lock(&self) {
        let _guard = self.sync_lock.lock().await;
        *self.vault.write().expect("cloud sync vault lock poisoned") = None;
        let mut runtime = self
            .runtime
            .lock()
            .expect("cloud sync status lock poisoned");
        runtime.syncing = false;
        runtime.last_error = None;
        runtime.conflicts = 0;
    }

    pub fn export_to_file(&self, path: &std::path::Path) -> Result<CloudSyncFileReport> {
        portable_file::export_to_path(&self.database, path)
    }

    pub fn import_from_file(&self, path: &std::path::Path) -> Result<CloudSyncFileReport> {
        portable_file::import_from_path(&self.database, path)
    }

    pub fn status(&self) -> CloudSyncStatus {
        let vault = self
            .vault
            .read()
            .expect("cloud sync vault lock poisoned")
            .clone();
        let runtime = self
            .runtime
            .lock()
            .expect("cloud sync status lock poisoned");
        let (pending_changes, pending_error) =
            match self.database.cloud_sync().pending_change_count() {
                Ok(count) => (count, None),
                Err(error) => (
                    0,
                    Some(format!(
                        "Failed to read pending cloud sync changes: {error}"
                    )),
                ),
            };
        CloudSyncStatus {
            unlocked: vault.is_some(),
            syncing: runtime.syncing,
            provider: vault.as_ref().map(|value| value.provider.kind()),
            endpoint: vault.as_ref().map(|value| value.provider.target()),
            vault_id: vault.as_ref().map(|value| value.vault_id.clone()),
            pending_changes,
            conflicts: runtime.conflicts,
            last_success_at: runtime.last_success_at,
            last_error: runtime.last_error.clone().or(pending_error),
        }
    }

    pub async fn sync_now(&self) -> Result<CloudSyncReport> {
        let _guard = self.sync_lock.lock().await;
        let vault = self
            .vault
            .read()
            .expect("cloud sync vault lock poisoned")
            .clone()
            .ok_or_else(|| anyhow!("Cloud sync is locked; enter a pairing code first"))?;
        {
            let mut runtime = self
                .runtime
                .lock()
                .expect("cloud sync status lock poisoned");
            runtime.syncing = true;
            runtime.last_error = None;
        }

        let result = self.run_sync(&vault).await;
        let mut runtime = self
            .runtime
            .lock()
            .expect("cloud sync status lock poisoned");
        runtime.syncing = false;
        match &result {
            Ok(report) => {
                runtime.last_success_at = Some(Utc::now().timestamp());
                runtime.last_error = None;
                runtime.conflicts = report.conflicts;
            }
            Err(error) => runtime.last_error = Some(error.to_string()),
        }
        result
    }

    fn set_unlocked(&self, bundle: PairingBundle, key: VaultKey) {
        *self.vault.write().expect("cloud sync vault lock poisoned") = Some(UnlockedVault {
            provider: bundle.provider,
            vault_id: bundle.vault_id,
            key: Arc::new(key),
        });
        let mut runtime = self
            .runtime
            .lock()
            .expect("cloud sync status lock poisoned");
        runtime.last_success_at = None;
        runtime.last_error = None;
        runtime.conflicts = 0;
    }

    async fn run_sync(&self, vault: &UnlockedVault) -> Result<CloudSyncReport> {
        let mut report = CloudSyncReport::default();
        let mut cursor = self
            .database
            .cloud_sync()
            .current_remote_cursor_for_vault(&vault.vault_id)?;

        for _ in 0..MAX_SYNC_ROUNDS {
            let upload = prepare_upload(&self.database, vault, cursor.clone())?;
            let envelope = upload.as_ref().map(|upload| upload.envelope.clone());

            let response = self
                .transport
                .exchange(
                    &vault.provider,
                    &vault.vault_id,
                    &SyncExchangeRequest {
                        cursor: cursor.clone(),
                        envelope,
                    },
                )
                .await?;

            let mut remote_changes = Vec::new();
            for provider_envelope in &response.envelopes {
                let encrypted: EncryptedSyncBatch =
                    serde_json::from_str(&provider_envelope.ciphertext)
                        .context("Provider envelope is not a valid encrypted sync batch")?;
                if encrypted.context.batch_id != provider_envelope.envelope_id {
                    bail!("Provider envelope ID does not match its authenticated batch ID");
                }
                if encrypted.context.vault_id != vault.vault_id {
                    bail!("Provider returned an encrypted batch for another vault");
                }
                if encrypted.context.schema_version != SYNC_CHANGE_SCHEMA_VERSION {
                    bail!("Provider returned an unsupported sync schema version");
                }
                let batch: PendingSyncBatch = decrypt_batch(&vault.key, &encrypted)
                    .map_err(|error| anyhow!(error.to_string()))?;
                if batch.device_id != encrypted.context.device_id {
                    bail!("Encrypted sync batch device ID mismatch");
                }
                remote_changes.extend(batch.changes);
                if remote_changes.len() > MAX_REMOTE_CHANGES_PER_PAGE {
                    bail!("Cloud sync provider page contains too many changes");
                }
            }

            let applied = self.database.cloud_sync().apply_remote_batch_for_vault(
                &vault.vault_id,
                &response.cursor,
                &remote_changes,
            )?;
            cursor = Some(response.cursor);
            report.downloaded += remote_changes.len();
            report.applied += applied.applied;
            report.ignored += applied.ignored;
            report.conflicts += applied.conflicts.len();

            if let Some(upload) = upload {
                report.uploaded += self
                    .database
                    .cloud_sync()
                    .acknowledge_pending_upload(&vault.vault_id, &upload.envelope.envelope_id)?;
            }

            let more_local = !self
                .database
                .cloud_sync()
                .export_pending_changes(1)?
                .changes
                .is_empty();
            if !response.has_more && !more_local {
                report.pending_changes = 0;
                report.cursor = cursor;
                return Ok(report);
            }
        }

        bail!("Cloud sync did not drain within the round limit")
    }
}

fn prepare_upload(
    database: &Database,
    vault: &UnlockedVault,
    cursor: Option<String>,
) -> Result<Option<PreparedUpload>> {
    if let Some(upload) = database.cloud_sync().load_pending_upload(&vault.vault_id)? {
        return Ok(Some(PreparedUpload {
            envelope: SyncEnvelope {
                envelope_id: upload.envelope_id,
                ciphertext: upload.ciphertext,
            },
        }));
    }

    let mut limit = MAX_OUTBOX_BATCH;
    loop {
        let pending = database.cloud_sync().export_pending_changes(limit)?;
        if pending.changes.is_empty() {
            return Ok(None);
        }
        let envelope = build_sync_envelope(vault, cursor.clone(), &pending)?;
        if envelope.ciphertext.len() <= MAX_PROVIDER_CIPHERTEXT_BYTES {
            let change_ids = pending
                .changes
                .into_iter()
                .map(|change| change.change_id)
                .collect::<Vec<_>>();
            database
                .cloud_sync()
                .save_pending_upload(&PendingSyncUpload {
                    vault_id: vault.vault_id.clone(),
                    envelope_id: envelope.envelope_id.clone(),
                    ciphertext: envelope.ciphertext.clone(),
                    change_ids: change_ids.clone(),
                })?;
            return Ok(Some(PreparedUpload { envelope }));
        }
        if pending.changes.len() == 1 {
            bail!(
                "Sync change {} exceeds the provider envelope size limit",
                pending.changes[0].change_id
            );
        }
        limit = (pending.changes.len() / 2).max(1);
    }
}

fn build_sync_envelope(
    vault: &UnlockedVault,
    cursor: Option<String>,
    pending: &PendingSyncBatch,
) -> Result<SyncEnvelope> {
    let batch_id = Uuid::new_v4().to_string();
    let encrypted = encrypt_batch(
        &vault.key,
        SyncBatchContext {
            vault_id: vault.vault_id.clone(),
            schema_version: SYNC_CHANGE_SCHEMA_VERSION,
            device_id: pending.device_id.clone(),
            batch_id: batch_id.clone(),
            cursor,
        },
        pending,
    )
    .map_err(|error| anyhow!(error.to_string()))?;
    Ok(SyncEnvelope {
        envelope_id: batch_id,
        ciphertext: serde_json::to_string(&encrypted)?,
    })
}

fn encode_pairing_bundle(bundle: &PairingBundle) -> Result<String> {
    Ok(format!(
        "{PAIRING_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(bundle)?)
    ))
}

fn decode_pairing_bundle(code: &str) -> Result<PairingBundle> {
    if code.len() > MAX_PAIRING_CODE_BYTES {
        bail!("Cloud sync pairing code exceeds the size limit");
    }
    let encoded = code
        .trim()
        .strip_prefix(PAIRING_PREFIX)
        .ok_or_else(|| anyhow!("Invalid VibeShell cloud sync pairing code"))?;
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded)
        .context("Invalid VibeShell cloud sync pairing code")?;
    let bundle: PairingBundle =
        serde_json::from_slice(&decoded).context("Invalid cloud sync pairing payload")?;
    if bundle.version != PAIRING_VERSION {
        bail!("Unsupported cloud sync pairing version");
    }
    Ok(bundle)
}

fn validate_opaque_id(label: &str, value: &str) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicBool, Ordering},
    };

    use crate::storage::{AuthType, CommandSnippet, Server};

    use super::*;

    #[derive(Default)]
    struct MemoryTransport {
        envelopes: tokio::sync::Mutex<Vec<SyncEnvelope>>,
    }

    #[derive(Default)]
    struct BlockingTransport {
        inner: MemoryTransport,
        block_once: AtomicBool,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    impl BlockingTransport {
        fn block_first_exchange() -> Self {
            Self {
                block_once: AtomicBool::new(true),
                ..Self::default()
            }
        }
    }

    #[async_trait]
    impl SyncTransport for MemoryTransport {
        async fn exchange(
            &self,
            _config: &SyncProviderConfig,
            _vault_id: &str,
            request: &SyncExchangeRequest,
        ) -> Result<SyncExchangeResponse> {
            let cursor = request.cursor.as_deref().unwrap_or("0").parse::<usize>()?;
            let mut envelopes = self.envelopes.lock().await;
            if let Some(upload) = &request.envelope {
                let existing = envelopes
                    .iter()
                    .find(|value| value.envelope_id == upload.envelope_id);
                match existing {
                    Some(existing) if existing != upload => bail!("envelope conflict"),
                    Some(_) => {}
                    None => envelopes.push(upload.clone()),
                }
            }
            let returned = envelopes.iter().skip(cursor).cloned().collect::<Vec<_>>();
            Ok(SyncExchangeResponse {
                cursor: envelopes.len().to_string(),
                envelopes: returned,
                has_more: false,
            })
        }
    }

    #[async_trait]
    impl SyncTransport for BlockingTransport {
        async fn exchange(
            &self,
            config: &SyncProviderConfig,
            vault_id: &str,
            request: &SyncExchangeRequest,
        ) -> Result<SyncExchangeResponse> {
            if self.block_once.swap(false, Ordering::SeqCst) {
                self.entered.notify_one();
                self.release.notified().await;
            }
            self.inner.exchange(config, vault_id, request).await
        }
    }

    fn database() -> (tempfile::TempDir, Arc<Database>) {
        let directory = tempfile::tempdir().unwrap();
        let database = Arc::new(Database::new_at(directory.path().join("app.db")).unwrap());
        (directory, database)
    }

    async fn create_test_vault(manager: &CloudSyncManager) -> CloudSyncPairingInfo {
        manager
            .create_github_gist_vault(Some("aabbcc".to_string()), "test-token".to_string())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn pairing_codes_round_trip_and_reject_invalid_provider_config() {
        let (_dir, database) = database();
        let manager =
            CloudSyncManager::with_transport(database, Arc::new(MemoryTransport::default()));
        let created = manager
            .create_github_gist_vault(Some("aabbcc".to_string()), "test-token".to_string())
            .await
            .unwrap();
        manager.lock().await;
        let joined = manager.join_vault(&created.pairing_code).await.unwrap();

        assert_eq!(joined.endpoint, "https://gist.github.com/aabbcc");
        assert_eq!(joined.vault_id, created.vault_id);
        assert!(manager.status().unlocked);
        assert!(manager
            .create_github_gist_vault(Some("not-a-gist".to_string()), "test-token".to_string())
            .await
            .is_err());
        assert!(manager
            .create_github_gist_vault(Some("aabbcc".to_string()), String::new())
            .await
            .is_err());
        assert!(manager
            .join_vault(&"x".repeat(MAX_PAIRING_CODE_BYTES + 1))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn encrypted_transport_syncs_servers_without_exposing_payload_or_credentials() {
        let transport = Arc::new(MemoryTransport::default());
        let (_source_dir, source) = database();
        let (_target_dir, target) = database();
        let source_manager = CloudSyncManager::with_transport(source.clone(), transport.clone());
        let target_manager = CloudSyncManager::with_transport(target.clone(), transport.clone());
        let pairing = create_test_vault(&source_manager).await;
        target_manager
            .join_vault(&pairing.pairing_code)
            .await
            .unwrap();

        let mut server = Server {
            id: String::new(),
            name: "production-edge".to_string(),
            host: "secret-host.internal".to_string(),
            port: 22,
            username: "deploy".to_string(),
            auth_type: AuthType::Password,
            credential_id: Some("must-stay-local".to_string()),
            group_id: None,
            tags: vec!["private".to_string()],
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: Some("secret-command".to_string()),
            agent_forwarding: false,
        };
        source.server_add(&mut server).unwrap();

        source_manager.sync_now().await.unwrap();
        target_manager.sync_now().await.unwrap();

        let stored = target.server_get(&server.id).unwrap().unwrap();
        assert_eq!(stored.host, "secret-host.internal");
        assert!(stored.credential_id.is_none());
        let provider_json = serde_json::to_string(&*transport.envelopes.lock().await).unwrap();
        for secret in [
            "production-edge",
            "secret-host.internal",
            "deploy",
            "private",
            "secret-command",
            "must-stay-local",
        ] {
            assert!(!provider_json.contains(secret), "provider leaked {secret}");
        }
    }

    #[tokio::test]
    async fn cumulative_outbox_payloads_are_split_under_the_provider_limit() {
        let transport = Arc::new(MemoryTransport::default());
        let (_dir, database) = database();
        let manager = CloudSyncManager::with_transport(database.clone(), transport.clone());
        create_test_vault(&manager).await;

        for index in 0..40 {
            let mut snippet = CommandSnippet {
                id: String::new(),
                name: format!("large-{index}"),
                command: "x".repeat(20_000),
                category: "test".to_string(),
                description: String::new(),
                tags: Vec::new(),
                created_at: 0,
                updated_at: 0,
            };
            database.snippet_add(&mut snippet).unwrap();
        }

        let report = manager.sync_now().await.unwrap();

        assert_eq!(report.uploaded, 40);
        let envelopes = transport.envelopes.lock().await;
        assert!(envelopes.len() > 1);
        assert!(envelopes
            .iter()
            .all(|envelope| envelope.ciphertext.len() <= MAX_PROVIDER_CIPHERTEXT_BYTES));
    }

    #[tokio::test]
    async fn status_reports_the_full_outbox_count_beyond_one_upload_batch() {
        let transport = Arc::new(MemoryTransport::default());
        let (_dir, database) = database();
        let manager = CloudSyncManager::with_transport(database.clone(), transport);
        create_test_vault(&manager).await;

        for index in 0..(MAX_OUTBOX_BATCH + 5) {
            let mut snippet = CommandSnippet {
                id: String::new(),
                name: format!("pending-{index}"),
                command: "true".to_string(),
                category: "test".to_string(),
                description: String::new(),
                tags: Vec::new(),
                created_at: 0,
                updated_at: 0,
            };
            database.snippet_add(&mut snippet).unwrap();
        }

        assert_eq!(manager.status().pending_changes, MAX_OUTBOX_BATCH + 5);
    }

    #[tokio::test]
    async fn vault_switch_waits_for_an_in_flight_sync_before_rebuilding_the_outbox() {
        let transport = Arc::new(BlockingTransport::block_first_exchange());
        let (_dir, database) = database();
        let manager = Arc::new(CloudSyncManager::with_transport(
            database.clone(),
            transport.clone(),
        ));
        let vault_a = create_test_vault(&manager).await;
        let mut server = Server {
            id: String::new(),
            name: "serialized-vault-switch".to_string(),
            host: "serialized.internal".to_string(),
            port: 22,
            username: "deploy".to_string(),
            auth_type: AuthType::Password,
            credential_id: None,
            group_id: None,
            tags: Vec::new(),
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: None,
            agent_forwarding: false,
        };
        database.server_add(&mut server).unwrap();

        let syncing_manager = manager.clone();
        let sync_task = tokio::spawn(async move { syncing_manager.sync_now().await });
        transport.entered.notified().await;

        let switching_manager = manager.clone();
        let switch_task = tokio::spawn(async move {
            switching_manager
                .create_github_gist_vault(Some("ddeeff".to_string()), "test-token".to_string())
                .await
        });
        tokio::task::yield_now().await;
        assert!(!switch_task.is_finished());
        assert_eq!(
            database.cloud_sync().active_vault_id().unwrap().as_deref(),
            Some(vault_a.vault_id.as_str())
        );

        transport.release.notify_one();
        sync_task.await.unwrap().unwrap();
        let vault_b = switch_task.await.unwrap().unwrap();
        assert_eq!(
            database.cloud_sync().active_vault_id().unwrap().as_deref(),
            Some(vault_b.vault_id.as_str())
        );
        assert_eq!(
            database
                .cloud_sync()
                .export_pending_changes(MAX_OUTBOX_BATCH)
                .unwrap()
                .changes
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn a_single_oversized_change_returns_an_actionable_error() {
        let transport = Arc::new(MemoryTransport::default());
        let (_dir, database) = database();
        let manager = CloudSyncManager::with_transport(database.clone(), transport);
        create_test_vault(&manager).await;
        let mut snippet = CommandSnippet {
            id: String::new(),
            name: "oversized".to_string(),
            command: "x".repeat(MAX_PROVIDER_CIPHERTEXT_BYTES),
            category: "test".to_string(),
            description: String::new(),
            tags: Vec::new(),
            created_at: 0,
            updated_at: 0,
        };
        database.snippet_add(&mut snippet).unwrap();
        let vault = manager
            .vault
            .read()
            .expect("vault lock should not be poisoned")
            .clone()
            .unwrap();

        let error = prepare_upload(&database, &vault, None)
            .err()
            .expect("oversized change should fail");

        assert!(error
            .to_string()
            .contains("exceeds the provider envelope size limit"));
    }

    #[test]
    fn pairing_payload_does_not_accept_unknown_fields() {
        let mut value = serde_json::to_value(PairingBundle {
            version: PAIRING_VERSION,
            provider: SyncProviderConfig::GithubGist {
                gist_id: Some("aabbcc".to_string()),
                token: "test-token".to_string(),
            },
            vault_id: "vault".to_string(),
            vault_key: URL_SAFE_NO_PAD.encode([7_u8; 32]),
        })
        .unwrap();
        value["unexpected"] = serde_json::json!(true);
        let code = format!(
            "{PAIRING_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap())
        );
        assert!(decode_pairing_bundle(&code).is_err());
    }

    #[test]
    fn memory_transport_type_is_object_safe() {
        let transports: HashMap<&str, Arc<dyn SyncTransport>> = HashMap::from([(
            "memory",
            Arc::new(MemoryTransport::default()) as Arc<dyn SyncTransport>,
        )]);
        assert!(transports.contains_key("memory"));
    }
}
