//! SSH Host Key Fingerprint Storage and Verification
//!
//! This module provides secure storage and verification of SSH server host key fingerprints.
//! It stores fingerprints locally to detect potential MITM attacks when a server's key changes.

use anyhow::{Context, Result};
use chrono::Utc;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Represents a stored SSH host key fingerprint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredFingerprint {
    /// Unique identifier for the fingerprint record
    pub id: String,
    /// Hostname or IP address of the SSH server
    pub host: String,
    /// Port number of the SSH server
    pub port: u16,
    /// The fingerprint hash (SHA256 or MD5)
    pub fingerprint: String,
    /// The algorithm used for the host key (e.g., "ssh-rsa", "ssh-ed25519", "ecdsa-sha2-nistp256")
    pub algorithm: String,
    /// Unix timestamp when the fingerprint was first added
    pub added_at: i64,
    /// Unix timestamp when the fingerprint was last verified
    pub last_verified_at: i64,
    /// Optional friendly name for the server
    pub server_name: Option<String>,
}

/// Result of fingerprint verification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FingerprintVerificationResult {
    /// Fingerprint matches stored value - safe to connect
    Trusted,
    /// No stored fingerprint - new server, user should verify
    Unknown {
        fingerprint: String,
        algorithm: String,
    },
    /// Fingerprint changed from stored value - potential MITM attack!
    Changed {
        stored_fingerprint: String,
        new_fingerprint: String,
        stored_algorithm: String,
        new_algorithm: String,
        stored_at: i64,
    },
}

/// Pure TOFU (Trust On First Use) decision for a presented host key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyDecision {
    /// Stored fingerprint matches the presented key - allow the handshake.
    Accept,
    /// A different key was presented for a known host - potential MITM.
    Changed,
    /// No stored fingerprint for this host yet - first contact.
    Unknown,
}

/// Decide whether a presented host key fingerprint may be accepted (TOFU).
///
/// Pure function so the security-relevant decision is unit-testable without
/// any store or IO.
pub fn decide_host_key(stored: Option<&str>, presented: &str) -> HostKeyDecision {
    match stored {
        Some(stored) if stored == presented => HostKeyDecision::Accept,
        Some(_) => HostKeyDecision::Changed,
        None => HostKeyDecision::Unknown,
    }
}

/// Rejection details captured when the SSH handshake is aborted because the
/// presented host key is not trusted. The russh `check_server_key` callback
/// can only return a bool, so the typed reason is recorded in a shared slot
/// and converted into a structured error by the connect wrapper.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum HostKeyRejection {
    /// No stored fingerprint for this host: first contact, user approval is
    /// required before any credentials may be sent.
    Unknown {
        fingerprint: String,
        algorithm: String,
    },
    /// The presented key differs from the stored one - potential MITM.
    Changed {
        stored_fingerprint: String,
        stored_algorithm: String,
        stored_at: i64,
        fingerprint: String,
        algorithm: String,
    },
    /// No host-key verification context was configured - fail closed.
    Unconfigured,
}

impl HostKeyRejection {
    /// Machine-parseable error message. The first line carries the
    /// `HOST_KEY_UNKNOWN` / `HOST_KEY_CHANGED` marker; the following
    /// `key: value` lines carry the details so callers (GUI, MCP clients)
    /// can render precise warnings and parse the offending identity.
    pub fn error_message(&self, host: &str, port: u16) -> String {
        match self {
            HostKeyRejection::Unknown {
                fingerprint,
                algorithm,
            } => format!(
                "HOST_KEY_UNKNOWN: the host key for {host}:{port} is not trusted; \
                 connection refused before authentication.\n\
                 host: {host}\n\
                 port: {port}\n\
                 presented-fingerprint: {fingerprint}\n\
                 presented-key-type: {algorithm}"
            ),
            HostKeyRejection::Changed {
                stored_fingerprint,
                stored_algorithm,
                stored_at,
                fingerprint,
                algorithm,
            } => format!(
                "HOST_KEY_CHANGED: possible man-in-the-middle attack; the host key \
                 for {host}:{port} differs from the stored key. Connection refused \
                 before authentication.\n\
                 host: {host}\n\
                 port: {port}\n\
                 stored-fingerprint: {stored_fingerprint}\n\
                 stored-key-type: {stored_algorithm}\n\
                 presented-fingerprint: {fingerprint}\n\
                 presented-key-type: {algorithm}\n\
                 first-trusted: {stored_at}"
            ),
            HostKeyRejection::Unconfigured => format!(
                "HOST_KEY_UNVERIFIED: host key verification is not configured; \
                 refusing to connect to {host}:{port}."
            ),
        }
    }
}

/// Map a store verification result to the TOFU decision: `Ok(())` allows the
/// handshake, `Err(rejection)` denies it with the structured reason.
/// Pure function, unit-testable without a store.
pub fn evaluate_host_key(result: &FingerprintVerificationResult) -> Result<(), HostKeyRejection> {
    match result {
        FingerprintVerificationResult::Trusted => Ok(()),
        FingerprintVerificationResult::Unknown {
            fingerprint,
            algorithm,
        } => Err(HostKeyRejection::Unknown {
            fingerprint: fingerprint.clone(),
            algorithm: algorithm.clone(),
        }),
        FingerprintVerificationResult::Changed {
            stored_fingerprint,
            new_fingerprint,
            stored_algorithm,
            new_algorithm,
            stored_at,
        } => Err(HostKeyRejection::Changed {
            stored_fingerprint: stored_fingerprint.clone(),
            stored_algorithm: stored_algorithm.clone(),
            stored_at: *stored_at,
            fingerprint: new_fingerprint.clone(),
            algorithm: new_algorithm.clone(),
        }),
    }
}

/// Host-key verification context attached to one SSH connection attempt:
/// which store to consult and under which (host, port) identity. Cloned into
/// the russh client handler for `check_server_key`.
#[derive(Clone)]
pub struct HostKeyCheck {
    store: Arc<FingerprintStore>,
    host: String,
    port: u16,
}

impl HostKeyCheck {
    pub fn new(store: Arc<FingerprintStore>, host: impl Into<String>, port: u16) -> Self {
        Self {
            store,
            host: host.into(),
            port,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn refresh(&self) -> Result<()> {
        self.store.reload()
    }

    /// Look up the presented key in the store, keyed by this check's
    /// (host, port) identity.
    pub fn verify(&self, fingerprint: &str, algorithm: &str) -> FingerprintVerificationResult {
        self.store
            .verify(&self.host, self.port, fingerprint, algorithm)
    }
}

/// Fingerprint storage backend
/// Stores fingerprints in a JSON file in the application data directory
pub struct FingerprintStore {
    fingerprints: Mutex<HashMap<String, StoredFingerprint>>,
    file_path: PathBuf,
}

impl FingerprintStore {
    /// Create a new fingerprint store, loading existing fingerprints from disk
    pub fn new() -> Result<Self> {
        let file_path = Self::get_store_path()?;
        Self::new_at(file_path)
    }

    /// Create a fingerprint store at an explicit application-owned path.
    pub fn new_at(path: impl AsRef<Path>) -> Result<Self> {
        let file_path = path.as_ref().to_path_buf();
        debug!("[FingerprintStore] Using store path: {:?}", file_path);

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Load existing fingerprints if file exists
        let fingerprints = Self::read_snapshot(&file_path)?;

        Ok(Self {
            fingerprints: Mutex::new(fingerprints),
            file_path,
        })
    }

    /// Get the path to the fingerprint store file
    fn get_store_path() -> Result<PathBuf> {
        crate::platform::default_fingerprint_path()
    }

    /// Generate a unique key for host:port combination
    fn host_key(host: &str, port: u16) -> String {
        format!("{}:{}", host.to_lowercase(), port)
    }

    /// Verify a server's fingerprint against stored values
    /// Returns the verification result indicating if it's trusted, unknown, or changed
    pub fn verify(
        &self,
        host: &str,
        port: u16,
        fingerprint: &str,
        algorithm: &str,
    ) -> FingerprintVerificationResult {
        let key = Self::host_key(host, port);
        let fingerprints = self.fingerprints.lock().unwrap();

        match fingerprints.get(&key) {
            Some(stored) => match decide_host_key(Some(&stored.fingerprint), fingerprint) {
                HostKeyDecision::Accept => {
                    debug!("[FingerprintStore] Fingerprint matches for {}", key);
                    FingerprintVerificationResult::Trusted
                }
                HostKeyDecision::Changed => {
                    warn!(
                        "[FingerprintStore] FINGERPRINT CHANGED for {}! Stored: {}, New: {}",
                        key, stored.fingerprint, fingerprint
                    );
                    FingerprintVerificationResult::Changed {
                        stored_fingerprint: stored.fingerprint.clone(),
                        new_fingerprint: fingerprint.to_string(),
                        stored_algorithm: stored.algorithm.clone(),
                        new_algorithm: algorithm.to_string(),
                        stored_at: stored.added_at,
                    }
                }
                // A stored entry is present, so the decision cannot be Unknown.
                HostKeyDecision::Unknown => {
                    unreachable!("stored entry present but decision was Unknown")
                }
            },
            None => {
                debug!(
                    "[FingerprintStore] Unknown host {}, fingerprint: {}",
                    key, fingerprint
                );
                FingerprintVerificationResult::Unknown {
                    fingerprint: fingerprint.to_string(),
                    algorithm: algorithm.to_string(),
                }
            }
        }
    }

    /// Save a new fingerprint or update existing one
    pub fn save(
        &self,
        host: &str,
        port: u16,
        fingerprint: &str,
        algorithm: &str,
        server_name: Option<&str>,
    ) -> Result<StoredFingerprint> {
        let key = Self::host_key(host, port);
        let now = Utc::now().timestamp();

        let record = StoredFingerprint {
            id: uuid::Uuid::new_v4().to_string(),
            host: host.to_string(),
            port,
            fingerprint: fingerprint.to_string(),
            algorithm: algorithm.to_string(),
            added_at: now,
            last_verified_at: now,
            server_name: server_name.map(|s| s.to_string()),
        };

        self.update(|fingerprints| {
            fingerprints.insert(key.clone(), record.clone());
        })?;
        info!("[FingerprintStore] Saved fingerprint for {}", key);
        Ok(record)
    }

    /// Update the last verified timestamp for a host
    pub fn touch(&self, host: &str, port: u16) -> Result<()> {
        let key = Self::host_key(host, port);
        let now = Utc::now().timestamp();

        self.update(|fingerprints| {
            if let Some(record) = fingerprints.get_mut(&key) {
                record.last_verified_at = now;
            }
        })?;
        debug!("[FingerprintStore] Updated last_verified_at for {}", key);
        Ok(())
    }

    /// Delete a stored fingerprint
    pub fn delete(&self, host: &str, port: u16) -> Result<bool> {
        let key = Self::host_key(host, port);

        let removed = self.update(|fingerprints| fingerprints.remove(&key).is_some())?;

        if removed {
            info!("[FingerprintStore] Deleted fingerprint for {}", key);
        } else {
            debug!(
                "[FingerprintStore] No fingerprint found to delete for {}",
                key
            );
        }

        Ok(removed)
    }

    /// Delete a fingerprint by its ID
    pub fn delete_by_id(&self, id: &str) -> Result<bool> {
        let removed = self.update(|fingerprints| {
            let key_to_remove = fingerprints
                .iter()
                .find(|(_, v)| v.id == id)
                .map(|(k, _)| k.clone());

            if let Some(key) = key_to_remove {
                fingerprints.remove(&key);
                true
            } else {
                false
            }
        })?;

        if removed {
            info!("[FingerprintStore] Deleted fingerprint with id {}", id);
        }

        Ok(removed)
    }

    /// Get a specific fingerprint by host and port
    pub fn get(&self, host: &str, port: u16) -> Option<StoredFingerprint> {
        let key = Self::host_key(host, port);
        let fingerprints = self.fingerprints.lock().unwrap();
        fingerprints.get(&key).cloned()
    }

    /// Get a fingerprint by ID
    pub fn get_by_id(&self, id: &str) -> Option<StoredFingerprint> {
        let fingerprints = self.fingerprints.lock().unwrap();
        fingerprints.values().find(|v| v.id == id).cloned()
    }

    /// List all stored fingerprints
    pub fn list(&self) -> Vec<StoredFingerprint> {
        let fingerprints = self.fingerprints.lock().unwrap();
        let mut list: Vec<StoredFingerprint> = fingerprints.values().cloned().collect();
        // Sort by host:port for consistent ordering
        list.sort_by(|a, b| {
            let key_a = Self::host_key(&a.host, a.port);
            let key_b = Self::host_key(&b.host, b.port);
            key_a.cmp(&key_b)
        });
        list
    }

    fn read_snapshot(path: &Path) -> Result<HashMap<String, StoredFingerprint>> {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).with_context(|| {
                format!(
                    "Invalid SSH trust store {}; refusing to reset trusted keys",
                    path.display()
                )
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(error) => Err(error).context("Cannot read SSH trust store"),
        }
    }

    pub fn reload(&self) -> Result<()> {
        let mut current = self.fingerprints.lock().unwrap();
        *current = Self::read_snapshot(&self.file_path)?;
        Ok(())
    }

    /// Serialize GUI/daemon writers using a stable sibling lock, then merge
    /// with the latest on-disk state. Never trust an unpersisted approval.
    fn update<T>(
        &self,
        change: impl FnOnce(&mut HashMap<String, StoredFingerprint>) -> T,
    ) -> Result<T> {
        let mut current = self.fingerprints.lock().unwrap();
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(self.file_path.with_extension("json.lock"))?;
        lock.lock().context("Cannot lock SSH trust store")?;
        let mut next = Self::read_snapshot(&self.file_path)?;
        let result = change(&mut next);
        let temporary = self
            .file_path
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let persisted = (|| -> Result<()> {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            serde_json::to_writer_pretty(&mut file, &next)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &self.file_path)?;
            Ok(())
        })();
        if persisted.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        persisted.context("Cannot persist SSH trust store; approval was not applied")?;
        *current = next;
        // Dropping the file releases the inter-process lock on all paths.
        drop(lock);
        Ok(result)
    }

    /// Clear all stored fingerprints (useful for testing or reset)
    pub fn clear(&self) -> Result<()> {
        self.update(|fingerprints| fingerprints.clear())?;
        info!("[FingerprintStore] Cleared all fingerprints");
        Ok(())
    }
}

/// Helper function to format a fingerprint for display
/// Takes raw bytes and returns a human-readable SHA256 fingerprint
pub fn format_fingerprint(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose, Engine as _};
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();

    // Format as SHA256:base64
    format!("SHA256:{}", general_purpose::STANDARD.encode(result))
}

/// Extract fingerprint from a public key
pub fn extract_fingerprint_from_key(key: &russh::keys::PublicKeyOrCertificate) -> (String, String) {
    let key = key.public_key();
    (
        key.fingerprint(russh::keys::HashAlg::Sha256).to_string(),
        key.algorithm().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{tempdir, TempDir};

    // Returns (TempDir, FingerprintStore) — TempDir must be kept alive for the test duration
    fn test_store() -> (TempDir, FingerprintStore) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_fingerprints.json");

        let store = FingerprintStore::new_at(path).unwrap();
        (dir, store)
    }

    #[test]
    fn new_at_reloads_fingerprints_from_the_injected_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("fingerprints.json");
        let store = FingerprintStore::new_at(&path).unwrap();
        store
            .save("example.com", 22, "SHA256:abc", "ssh-ed25519", None)
            .unwrap();
        drop(store);

        let reopened = FingerprintStore::new_at(&path).unwrap();
        assert_eq!(
            reopened.get("example.com", 22).unwrap().fingerprint,
            "SHA256:abc"
        );
    }

    #[test]
    fn corrupt_store_is_not_silently_reset_or_trusted() {
        let (dir, store) = test_store();
        store
            .save("known", 22, "SHA256:old", "ssh-ed25519", None)
            .unwrap();
        let path = dir.path().join("test_fingerprints.json");
        fs::write(&path, "{broken").unwrap();
        assert!(FingerprintStore::new_at(&path).is_err());
        assert!(store.reload().is_err());
        assert!(store
            .save("new", 22, "SHA256:new", "ssh-ed25519", None)
            .is_err());
        assert!(store.get("new", 22).is_none());
        assert_eq!(fs::read_to_string(path).unwrap(), "{broken");
    }

    #[test]
    fn independent_writers_merge_and_revocations_reload() {
        let (dir, first) = test_store();
        let second = FingerprintStore::new_at(dir.path().join("test_fingerprints.json")).unwrap();
        first
            .save("one", 22, "SHA256:one", "ssh-ed25519", None)
            .unwrap();
        second
            .save("two", 22, "SHA256:two", "ssh-ed25519", None)
            .unwrap();
        first.reload().unwrap();
        assert_eq!(first.list().len(), 2);
        second.delete("one", 22).unwrap();
        first.reload().unwrap();
        assert!(first.get("one", 22).is_none());
        assert!(first.get("two", 22).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn persisted_store_is_private_and_has_no_temporary_files() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, store) = test_store();
        store
            .save("one", 22, "SHA256:one", "ssh-ed25519", None)
            .unwrap();
        assert_eq!(
            fs::metadata(&store.file_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2); // snapshot + stable lock
    }

    #[test]
    fn test_save_and_get() {
        let (_dir, store) = test_store();

        store
            .save(
                "example.com",
                22,
                "SHA256:abcdef123456",
                "ssh-ed25519",
                Some("My Server"),
            )
            .unwrap();

        let fp = store.get("example.com", 22).unwrap();
        assert_eq!(fp.host, "example.com");
        assert_eq!(fp.port, 22);
        assert_eq!(fp.fingerprint, "SHA256:abcdef123456");
        assert_eq!(fp.algorithm, "ssh-ed25519");
        assert_eq!(fp.server_name, Some("My Server".to_string()));
    }

    #[test]
    fn test_verify_trusted() {
        let (_dir, store) = test_store();

        store
            .save("example.com", 22, "SHA256:abc", "ssh-ed25519", None)
            .unwrap();

        let result = store.verify("example.com", 22, "SHA256:abc", "ssh-ed25519");
        assert!(matches!(result, FingerprintVerificationResult::Trusted));
    }

    #[test]
    fn test_verify_unknown() {
        let (_dir, store) = test_store();

        let result = store.verify("unknown.com", 22, "SHA256:xyz", "ssh-rsa");
        assert!(matches!(
            result,
            FingerprintVerificationResult::Unknown { .. }
        ));
    }

    #[test]
    fn test_verify_changed() {
        let (_dir, store) = test_store();

        store
            .save("example.com", 22, "SHA256:old", "ssh-ed25519", None)
            .unwrap();

        let result = store.verify("example.com", 22, "SHA256:new", "ssh-ed25519");
        assert!(matches!(
            result,
            FingerprintVerificationResult::Changed { .. }
        ));
    }

    #[test]
    fn test_delete() {
        let (_dir, store) = test_store();

        store
            .save("example.com", 22, "SHA256:abc", "ssh-ed25519", None)
            .unwrap();
        assert!(store.get("example.com", 22).is_some());

        store.delete("example.com", 22).unwrap();
        assert!(store.get("example.com", 22).is_none());
    }

    #[test]
    fn test_list() {
        let (_dir, store) = test_store();

        store
            .save("server1.com", 22, "SHA256:aaa", "ssh-ed25519", None)
            .unwrap();
        store
            .save("server2.com", 2222, "SHA256:bbb", "ssh-rsa", None)
            .unwrap();

        let list = store.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_case_insensitive_host() {
        let (_dir, store) = test_store();

        store
            .save("Example.COM", 22, "SHA256:abc", "ssh-ed25519", None)
            .unwrap();

        // Should find with different case
        let fp = store.get("example.com", 22);
        assert!(fp.is_some());
    }

    #[test]
    fn test_store_roundtrip_persists_verification_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("roundtrip.json");

        {
            let store = FingerprintStore::new_at(&path).unwrap();
            store
                .save("host.example.com", 2222, "SHA256:live", "ssh-ed25519", None)
                .unwrap();
        }

        // Reopen from disk: the key must still be trusted and a different key
        // must still be detected as changed (roundtrip preserves the TOFU pin).
        let reopened = FingerprintStore::new_at(&path).unwrap();
        assert!(matches!(
            reopened.verify("host.example.com", 2222, "SHA256:live", "ssh-ed25519"),
            FingerprintVerificationResult::Trusted
        ));
        assert!(matches!(
            reopened.verify("host.example.com", 2222, "SHA256:rotated", "ssh-ed25519"),
            FingerprintVerificationResult::Changed { .. }
        ));
    }

    // === TOFU decision logic (pure functions) ===

    #[test]
    fn decide_accepts_known_matching_key() {
        assert_eq!(
            decide_host_key(Some("SHA256:abc"), "SHA256:abc"),
            HostKeyDecision::Accept
        );
    }

    #[test]
    fn decide_rejects_changed_key_with_details() {
        assert_eq!(
            decide_host_key(Some("SHA256:old"), "SHA256:new"),
            HostKeyDecision::Changed
        );
    }

    #[test]
    fn decide_rejects_unknown_key() {
        assert_eq!(
            decide_host_key(None, "SHA256:new"),
            HostKeyDecision::Unknown
        );
    }

    #[test]
    fn evaluate_maps_trusted_to_allow() {
        let result = evaluate_host_key(&FingerprintVerificationResult::Trusted);
        assert!(result.is_ok());
    }

    #[test]
    fn evaluate_maps_unknown_to_unknown_rejection() {
        let result = evaluate_host_key(&FingerprintVerificationResult::Unknown {
            fingerprint: "SHA256:presented".into(),
            algorithm: "ssh-ed25519".into(),
        })
        .unwrap_err();
        match &result {
            HostKeyRejection::Unknown {
                fingerprint,
                algorithm,
            } => {
                assert_eq!(fingerprint, "SHA256:presented");
                assert_eq!(algorithm, "ssh-ed25519");
            }
            other => panic!("expected Unknown rejection, got {:?}", other),
        }
        // The error message must be machine-parseable with the marker and details.
        let message = result.error_message("srv.example.com", 22);
        assert!(message.starts_with("HOST_KEY_UNKNOWN"));
        assert!(message.contains("presented-fingerprint: SHA256:presented"));
        assert!(message.contains("presented-key-type: ssh-ed25519"));
    }

    #[test]
    fn evaluate_maps_changed_to_changed_rejection_with_details() {
        let result = evaluate_host_key(&FingerprintVerificationResult::Changed {
            stored_fingerprint: "SHA256:old".into(),
            new_fingerprint: "SHA256:new".into(),
            stored_algorithm: "ssh-rsa".into(),
            new_algorithm: "ssh-ed25519".into(),
            stored_at: 1_700_000_000,
        })
        .unwrap_err();
        match &result {
            HostKeyRejection::Changed {
                stored_fingerprint,
                fingerprint,
                ..
            } => {
                assert_eq!(stored_fingerprint, "SHA256:old");
                assert_eq!(fingerprint, "SHA256:new");
            }
            other => panic!("expected Changed rejection, got {:?}", other),
        }
        let message = result.error_message("srv.example.com", 22);
        assert!(message.starts_with("HOST_KEY_CHANGED"));
        assert!(message.contains("stored-fingerprint: SHA256:old"));
        assert!(message.contains("presented-fingerprint: SHA256:new"));
        assert!(message.contains("first-trusted: 1700000000"));
    }

    #[test]
    fn host_key_check_authorizes_known_key_and_records_rejections() {
        let (_dir, store) = test_store();
        store
            .save("srv.example.com", 22, "SHA256:pinned", "ssh-ed25519", None)
            .unwrap();
        let store = Arc::new(store);

        let known = HostKeyCheck::new(store.clone(), "srv.example.com", 22);
        // Known + matching: allow.
        assert!(evaluate_host_key(&known.verify("SHA256:pinned", "ssh-ed25519")).is_ok());
        // Changed: deny with details.
        let changed = evaluate_host_key(&known.verify("SHA256:rotated", "ssh-ed25519"));
        assert!(matches!(changed, Err(HostKeyRejection::Changed { .. })));
        // Unknown identity: deny.
        let unknown = HostKeyCheck::new(store, "fresh.example.com", 22);
        assert!(matches!(
            evaluate_host_key(&unknown.verify("SHA256:pinned", "ssh-ed25519")),
            Err(HostKeyRejection::Unknown { .. })
        ));
    }
}
