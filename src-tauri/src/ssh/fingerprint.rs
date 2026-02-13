//! SSH Host Key Fingerprint Storage and Verification
//!
//! This module provides secure storage and verification of SSH server host key fingerprints.
//! It stores fingerprints locally to detect potential MITM attacks when a server's key changes.

use anyhow::{Result, anyhow};
use chrono::Utc;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use log::{info, warn, debug};

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
        debug!("[FingerprintStore] Using store path: {:?}", file_path);

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Load existing fingerprints if file exists
        let fingerprints = if file_path.exists() {
            let content = fs::read_to_string(&file_path)?;
            let fingerprints: HashMap<String, StoredFingerprint> =
                serde_json::from_str(&content).unwrap_or_default();
            info!("[FingerprintStore] Loaded {} stored fingerprints", fingerprints.len());
            fingerprints
        } else {
            info!("[FingerprintStore] No existing fingerprint store, starting fresh");
            HashMap::new()
        };

        Ok(Self {
            fingerprints: Mutex::new(fingerprints),
            file_path,
        })
    }

    /// Get the path to the fingerprint store file
    fn get_store_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "vibeshell", "VibeShell")
            .ok_or_else(|| anyhow!("Could not determine project directories"))?;
        Ok(proj_dirs.data_dir().join("ssh_fingerprints.json"))
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
            Some(stored) => {
                if stored.fingerprint == fingerprint {
                    debug!("[FingerprintStore] Fingerprint matches for {}", key);
                    FingerprintVerificationResult::Trusted
                } else {
                    warn!("[FingerprintStore] FINGERPRINT CHANGED for {}! Stored: {}, New: {}",
                          key, stored.fingerprint, fingerprint);
                    FingerprintVerificationResult::Changed {
                        stored_fingerprint: stored.fingerprint.clone(),
                        new_fingerprint: fingerprint.to_string(),
                        stored_algorithm: stored.algorithm.clone(),
                        new_algorithm: algorithm.to_string(),
                        stored_at: stored.added_at,
                    }
                }
            }
            None => {
                debug!("[FingerprintStore] Unknown host {}, fingerprint: {}", key, fingerprint);
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

        {
            let mut fingerprints = self.fingerprints.lock().unwrap();
            fingerprints.insert(key.clone(), record.clone());
        }

        self.persist()?;
        info!("[FingerprintStore] Saved fingerprint for {}", key);
        Ok(record)
    }

    /// Update the last verified timestamp for a host
    pub fn touch(&self, host: &str, port: u16) -> Result<()> {
        let key = Self::host_key(host, port);
        let now = Utc::now().timestamp();

        {
            let mut fingerprints = self.fingerprints.lock().unwrap();
            if let Some(record) = fingerprints.get_mut(&key) {
                record.last_verified_at = now;
            }
        }

        self.persist()?;
        debug!("[FingerprintStore] Updated last_verified_at for {}", key);
        Ok(())
    }

    /// Delete a stored fingerprint
    pub fn delete(&self, host: &str, port: u16) -> Result<bool> {
        let key = Self::host_key(host, port);

        let removed = {
            let mut fingerprints = self.fingerprints.lock().unwrap();
            fingerprints.remove(&key).is_some()
        };

        if removed {
            self.persist()?;
            info!("[FingerprintStore] Deleted fingerprint for {}", key);
        } else {
            debug!("[FingerprintStore] No fingerprint found to delete for {}", key);
        }

        Ok(removed)
    }

    /// Delete a fingerprint by its ID
    pub fn delete_by_id(&self, id: &str) -> Result<bool> {
        let removed = {
            let mut fingerprints = self.fingerprints.lock().unwrap();
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
        };

        if removed {
            self.persist()?;
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

    /// Persist fingerprints to disk
    fn persist(&self) -> Result<()> {
        let fingerprints = self.fingerprints.lock().unwrap();
        let content = serde_json::to_string_pretty(&*fingerprints)?;
        fs::write(&self.file_path, content)?;
        debug!("[FingerprintStore] Persisted {} fingerprints to disk", fingerprints.len());
        Ok(())
    }

    /// Clear all stored fingerprints (useful for testing or reset)
    pub fn clear(&self) -> Result<()> {
        {
            let mut fingerprints = self.fingerprints.lock().unwrap();
            fingerprints.clear();
        }
        self.persist()?;
        info!("[FingerprintStore] Cleared all fingerprints");
        Ok(())
    }
}

/// Helper function to format a fingerprint for display
/// Takes raw bytes and returns a human-readable SHA256 fingerprint
pub fn format_fingerprint(bytes: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    use base64::{Engine as _, engine::general_purpose};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();

    // Format as SHA256:base64
    format!("SHA256:{}", general_purpose::STANDARD.encode(result))
}

/// Extract fingerprint from a public key
pub fn extract_fingerprint_from_key(key: &russh_keys::key::PublicKey) -> (String, String) {
    use sha2::{Sha256, Digest};
    use base64::{Engine as _, engine::general_purpose};
    use russh_keys::PublicKeyBase64;

    // Get the algorithm name
    let algorithm = key.name().to_string();

    // Serialize the key to bytes for hashing
    let key_bytes = key.public_key_bytes();

    // Calculate SHA256 hash
    let mut hasher = Sha256::new();
    hasher.update(&key_bytes);
    let result = hasher.finalize();

    // Format as SHA256:base64 (like OpenSSH does)
    let encoded = general_purpose::STANDARD.encode(result);
    let fingerprint = format!("SHA256:{}", encoded.trim_end_matches('='));

    (fingerprint, algorithm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_store() -> FingerprintStore {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_fingerprints.json");

        FingerprintStore {
            fingerprints: Mutex::new(HashMap::new()),
            file_path: path,
        }
    }

    #[test]
    fn test_save_and_get() {
        let store = test_store();

        store.save(
            "example.com",
            22,
            "SHA256:abcdef123456",
            "ssh-ed25519",
            Some("My Server"),
        ).unwrap();

        let fp = store.get("example.com", 22).unwrap();
        assert_eq!(fp.host, "example.com");
        assert_eq!(fp.port, 22);
        assert_eq!(fp.fingerprint, "SHA256:abcdef123456");
        assert_eq!(fp.algorithm, "ssh-ed25519");
        assert_eq!(fp.server_name, Some("My Server".to_string()));
    }

    #[test]
    fn test_verify_trusted() {
        let store = test_store();

        store.save("example.com", 22, "SHA256:abc", "ssh-ed25519", None).unwrap();

        let result = store.verify("example.com", 22, "SHA256:abc", "ssh-ed25519");
        assert!(matches!(result, FingerprintVerificationResult::Trusted));
    }

    #[test]
    fn test_verify_unknown() {
        let store = test_store();

        let result = store.verify("unknown.com", 22, "SHA256:xyz", "ssh-rsa");
        assert!(matches!(result, FingerprintVerificationResult::Unknown { .. }));
    }

    #[test]
    fn test_verify_changed() {
        let store = test_store();

        store.save("example.com", 22, "SHA256:old", "ssh-ed25519", None).unwrap();

        let result = store.verify("example.com", 22, "SHA256:new", "ssh-ed25519");
        assert!(matches!(result, FingerprintVerificationResult::Changed { .. }));
    }

    #[test]
    fn test_delete() {
        let store = test_store();

        store.save("example.com", 22, "SHA256:abc", "ssh-ed25519", None).unwrap();
        assert!(store.get("example.com", 22).is_some());

        store.delete("example.com", 22).unwrap();
        assert!(store.get("example.com", 22).is_none());
    }

    #[test]
    fn test_list() {
        let store = test_store();

        store.save("server1.com", 22, "SHA256:aaa", "ssh-ed25519", None).unwrap();
        store.save("server2.com", 2222, "SHA256:bbb", "ssh-rsa", None).unwrap();

        let list = store.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_case_insensitive_host() {
        let store = test_store();

        store.save("Example.COM", 22, "SHA256:abc", "ssh-ed25519", None).unwrap();

        // Should find with different case
        let fp = store.get("example.com", 22);
        assert!(fp.is_some());
    }
}
