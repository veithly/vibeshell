//! Authenticated wire codec for provider-facing cloud sync batches.

use std::error::Error;
use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use zeroize::Zeroize;

const VAULT_KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const AAD_DOMAIN: &str = "vibeshell.sync.pending-batch";

pub const SYNC_CRYPTO_WIRE_VERSION: u32 = 1;
pub const SYNC_CRYPTO_ALGORITHM: &str = "AES-256-GCM";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncCryptoError {
    InvalidVaultKey,
    RandomGeneration,
    Serialization,
    UnsupportedEnvelope,
    InvalidEnvelope,
    AuthenticationFailed,
}

impl fmt::Display for SyncCryptoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidVaultKey => "invalid vault key",
            Self::RandomGeneration => "secure random generation failed",
            Self::Serialization => "sync batch serialization failed",
            Self::UnsupportedEnvelope => "unsupported sync envelope",
            Self::InvalidEnvelope => "invalid sync envelope",
            Self::AuthenticationFailed => "sync batch authentication failed",
        };
        formatter.write_str(message)
    }
}

impl Error for SyncCryptoError {}

pub struct VaultKey {
    bytes: [u8; VAULT_KEY_LEN],
}

impl VaultKey {
    pub fn generate() -> Result<Self, SyncCryptoError> {
        let mut bytes = [0u8; VAULT_KEY_LEN];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| SyncCryptoError::RandomGeneration)?;
        Ok(Self { bytes })
    }

    pub fn import(encoded: &str) -> Result<Self, SyncCryptoError> {
        let mut decoded = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| SyncCryptoError::InvalidVaultKey)?;
        if decoded.len() != VAULT_KEY_LEN {
            decoded.zeroize();
            return Err(SyncCryptoError::InvalidVaultKey);
        }

        let mut bytes = [0u8; VAULT_KEY_LEN];
        bytes.copy_from_slice(&decoded);
        decoded.zeroize();
        Ok(Self { bytes })
    }

    pub fn export(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.bytes)
    }
}

impl fmt::Debug for VaultKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultKey([REDACTED])")
    }
}

impl Drop for VaultKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBatchContext {
    pub vault_id: String,
    pub batch_id: String,
    pub schema_version: u32,
    pub device_id: String,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedSyncBatch {
    pub wire_version: u32,
    pub algorithm: String,
    pub context: SyncBatchContext,
    pub nonce: String,
    pub ciphertext: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthenticatedMetadata<'a> {
    domain: &'static str,
    wire_version: u32,
    algorithm: &'a str,
    context: &'a SyncBatchContext,
}

pub fn encrypt_batch<T: Serialize>(
    key: &VaultKey,
    context: SyncBatchContext,
    batch: &T,
) -> Result<EncryptedSyncBatch, SyncCryptoError> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| SyncCryptoError::RandomGeneration)?;

    let mut ciphertext = serde_json::to_vec(batch).map_err(|_| SyncCryptoError::Serialization)?;
    let aad = authenticated_data(SYNC_CRYPTO_WIRE_VERSION, SYNC_CRYPTO_ALGORITHM, &context)?;
    encryption_key(key)?
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(aad),
            &mut ciphertext,
        )
        .map_err(|_| SyncCryptoError::AuthenticationFailed)?;

    Ok(EncryptedSyncBatch {
        wire_version: SYNC_CRYPTO_WIRE_VERSION,
        algorithm: SYNC_CRYPTO_ALGORITHM.to_string(),
        context,
        nonce: URL_SAFE_NO_PAD.encode(nonce_bytes),
        ciphertext: URL_SAFE_NO_PAD.encode(ciphertext),
    })
}

pub fn decrypt_batch<T: DeserializeOwned>(
    key: &VaultKey,
    envelope: &EncryptedSyncBatch,
) -> Result<T, SyncCryptoError> {
    if envelope.wire_version != SYNC_CRYPTO_WIRE_VERSION
        || envelope.algorithm != SYNC_CRYPTO_ALGORITHM
    {
        return Err(SyncCryptoError::UnsupportedEnvelope);
    }

    let nonce_bytes: [u8; NONCE_LEN] = URL_SAFE_NO_PAD
        .decode(&envelope.nonce)
        .map_err(|_| SyncCryptoError::InvalidEnvelope)?
        .try_into()
        .map_err(|_| SyncCryptoError::InvalidEnvelope)?;
    let mut ciphertext = URL_SAFE_NO_PAD
        .decode(&envelope.ciphertext)
        .map_err(|_| SyncCryptoError::InvalidEnvelope)?;
    if ciphertext.len() < AES_256_GCM.tag_len() {
        return Err(SyncCryptoError::InvalidEnvelope);
    }

    let aad = authenticated_data(
        envelope.wire_version,
        &envelope.algorithm,
        &envelope.context,
    )?;
    let plaintext = encryption_key(key)?
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(aad),
            &mut ciphertext,
        )
        .map_err(|_| SyncCryptoError::AuthenticationFailed)?;

    serde_json::from_slice(plaintext).map_err(|_| SyncCryptoError::Serialization)
}

fn encryption_key(key: &VaultKey) -> Result<LessSafeKey, SyncCryptoError> {
    UnboundKey::new(&AES_256_GCM, &key.bytes)
        .map(LessSafeKey::new)
        .map_err(|_| SyncCryptoError::InvalidVaultKey)
}

fn authenticated_data(
    wire_version: u32,
    algorithm: &str,
    context: &SyncBatchContext,
) -> Result<Vec<u8>, SyncCryptoError> {
    serde_json::to_vec(&AuthenticatedMetadata {
        domain: AAD_DOMAIN,
        wire_version,
        algorithm,
        context,
    })
    .map_err(|_| SyncCryptoError::Serialization)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::super::sync::{PendingSyncBatch, SyncChange, SyncEntityKind};
    use super::*;

    fn context() -> SyncBatchContext {
        SyncBatchContext {
            vault_id: "vault-6c54".to_string(),
            batch_id: "batch-84bc".to_string(),
            schema_version: 1,
            device_id: "device-a".to_string(),
            cursor: Some("opaque-cursor-17".to_string()),
        }
    }

    fn pending_batch() -> PendingSyncBatch {
        PendingSyncBatch {
            device_id: "device-a".to_string(),
            changes: vec![
                SyncChange {
                    schema_version: 1,
                    change_id: "change-server".to_string(),
                    entity_kind: SyncEntityKind::Server,
                    entity_id: "server-1".to_string(),
                    revision: "00000000000000000001:device-a".to_string(),
                    deleted: false,
                    payload: Some(json!({
                        "host": "secret.example.internal",
                        "username": "private-user",
                        "tags": ["production", "finance"]
                    })),
                },
                SyncChange {
                    schema_version: 1,
                    change_id: "change-snippet".to_string(),
                    entity_kind: SyncEntityKind::CommandSnippet,
                    entity_id: "snippet-1".to_string(),
                    revision: "00000000000000000002:device-a".to_string(),
                    deleted: false,
                    payload: Some(json!({
                        "name": "Private deploy",
                        "command": "deploy --token top-secret-value"
                    })),
                },
            ],
        }
    }

    #[test]
    fn generated_vault_keys_are_256_bit_and_exportable() {
        let first = VaultKey::generate().unwrap();
        let second = VaultKey::generate().unwrap();

        let exported = first.export();
        assert_eq!(URL_SAFE_NO_PAD.decode(&exported).unwrap().len(), 32);
        assert_ne!(exported, second.export());

        let imported = VaultKey::import(&exported).unwrap();
        assert_eq!(imported.export(), exported);
        assert_eq!(format!("{imported:?}"), "VaultKey([REDACTED])");
    }

    #[test]
    fn vault_key_import_rejects_malformed_or_wrong_length_material() {
        assert_eq!(
            VaultKey::import("not base64!").unwrap_err(),
            SyncCryptoError::InvalidVaultKey
        );
        assert_eq!(
            VaultKey::import(&URL_SAFE_NO_PAD.encode([0u8; 31])).unwrap_err(),
            SyncCryptoError::InvalidVaultKey
        );
    }

    #[test]
    fn pending_sync_batch_round_trips_through_the_wire_envelope() {
        let key = VaultKey::generate().unwrap();
        let imported_key = VaultKey::import(&key.export()).unwrap();
        let batch = pending_batch();
        let envelope = encrypt_batch(&key, context(), &batch).unwrap();

        let wire = serde_json::to_string(&envelope).unwrap();
        let received: EncryptedSyncBatch = serde_json::from_str(&wire).unwrap();
        let decrypted: PendingSyncBatch = decrypt_batch(&imported_key, &received).unwrap();

        assert_eq!(decrypted, batch);
    }

    #[test]
    fn every_encryption_uses_a_fresh_random_nonce() {
        let key = VaultKey::generate().unwrap();
        let batch = pending_batch();
        let mut nonces = HashSet::new();

        for _ in 0..32 {
            let envelope = encrypt_batch(&key, context(), &batch).unwrap();
            assert!(nonces.insert(envelope.nonce));
        }
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let key = VaultKey::generate().unwrap();
        let mut envelope = encrypt_batch(&key, context(), &pending_batch()).unwrap();
        let mut ciphertext = URL_SAFE_NO_PAD.decode(&envelope.ciphertext).unwrap();
        ciphertext[0] ^= 0x80;
        envelope.ciphertext = URL_SAFE_NO_PAD.encode(ciphertext);

        assert_eq!(
            decrypt_batch::<PendingSyncBatch>(&key, &envelope).unwrap_err(),
            SyncCryptoError::AuthenticationFailed
        );
    }

    #[test]
    fn every_context_field_is_authenticated() {
        let key = VaultKey::generate().unwrap();
        let envelope = encrypt_batch(&key, context(), &pending_batch()).unwrap();

        let mut tampered = Vec::new();
        let mut changed = envelope.clone();
        changed.context.vault_id.push_str("-other");
        tampered.push(changed);
        let mut changed = envelope.clone();
        changed.context.batch_id.push_str("-other");
        tampered.push(changed);
        let mut changed = envelope.clone();
        changed.context.schema_version += 1;
        tampered.push(changed);
        let mut changed = envelope.clone();
        changed.context.device_id.push_str("-other");
        tampered.push(changed);
        let mut changed = envelope;
        changed.context.cursor = Some("different-cursor".to_string());
        tampered.push(changed);

        for envelope in tampered {
            assert_eq!(
                decrypt_batch::<PendingSyncBatch>(&key, &envelope).unwrap_err(),
                SyncCryptoError::AuthenticationFailed
            );
        }
    }

    #[test]
    fn a_different_vault_key_cannot_decrypt_the_batch() {
        let encrypting_key = VaultKey::generate().unwrap();
        let wrong_key = VaultKey::generate().unwrap();
        let envelope = encrypt_batch(&encrypting_key, context(), &pending_batch()).unwrap();

        assert_eq!(
            decrypt_batch::<PendingSyncBatch>(&wrong_key, &envelope).unwrap_err(),
            SyncCryptoError::AuthenticationFailed
        );
    }

    #[test]
    fn wire_envelope_does_not_expose_domain_payload_plaintext() {
        let key = VaultKey::generate().unwrap();
        let envelope = encrypt_batch(&key, context(), &pending_batch()).unwrap();
        let wire = serde_json::to_string(&envelope).unwrap();

        for secret in [
            "secret.example.internal",
            "private-user",
            "production",
            "finance",
            "Private deploy",
            "deploy --token top-secret-value",
        ] {
            assert!(!wire.contains(secret), "wire exposed {secret:?}");
        }
        for field in ["\"host\"", "\"username\"", "\"tags\"", "\"command\""] {
            assert!(!wire.contains(field), "wire exposed field {field}");
        }
    }
}
