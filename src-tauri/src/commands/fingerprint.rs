//! Tauri commands for SSH fingerprint management
//!
//! These commands provide the frontend with access to fingerprint storage
//! for verifying SSH server host keys and managing trusted hosts.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tauri::State;

use crate::ssh::{
    FingerprintStore, FingerprintVerificationResult, HostKeyCheck, HostKeyRejection, SshClient,
    StoredFingerprint,
};

/// Request to get a fingerprint by host and port
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFingerprintRequest {
    pub host: String,
    pub port: u16,
}

/// Request to save a fingerprint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFingerprintRequest {
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub algorithm: String,
    pub server_name: Option<String>,
}

/// Request to delete a fingerprint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFingerprintRequest {
    pub host: String,
    pub port: u16,
}

/// Request to delete a fingerprint by ID
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFingerprintByIdRequest {
    pub id: String,
}

/// Request to verify a fingerprint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyFingerprintRequest {
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub algorithm: String,
}

/// Response for fingerprint verification
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyFingerprintResponse {
    pub status: String, // "trusted", "unknown", or "changed"
    pub fingerprint: Option<String>,
    pub algorithm: Option<String>,
    pub stored_fingerprint: Option<String>,
    pub stored_algorithm: Option<String>,
    pub stored_at: Option<i64>,
}

impl From<FingerprintVerificationResult> for VerifyFingerprintResponse {
    fn from(result: FingerprintVerificationResult) -> Self {
        match result {
            FingerprintVerificationResult::Trusted => VerifyFingerprintResponse {
                status: "trusted".to_string(),
                fingerprint: None,
                algorithm: None,
                stored_fingerprint: None,
                stored_algorithm: None,
                stored_at: None,
            },
            FingerprintVerificationResult::Unknown {
                fingerprint,
                algorithm,
            } => VerifyFingerprintResponse {
                status: "unknown".to_string(),
                fingerprint: Some(fingerprint),
                algorithm: Some(algorithm),
                stored_fingerprint: None,
                stored_algorithm: None,
                stored_at: None,
            },
            FingerprintVerificationResult::Changed {
                stored_fingerprint,
                new_fingerprint,
                stored_algorithm,
                new_algorithm,
                stored_at,
            } => VerifyFingerprintResponse {
                status: "changed".to_string(),
                fingerprint: Some(new_fingerprint),
                algorithm: Some(new_algorithm),
                stored_fingerprint: Some(stored_fingerprint),
                stored_algorithm: Some(stored_algorithm),
                stored_at: Some(stored_at),
            },
        }
    }
}

/// State wrapper for the fingerprint store
pub struct FingerprintState {
    pub store: Arc<FingerprintStore>,
}

impl FingerprintState {
    pub fn new() -> Result<Self, String> {
        let store = FingerprintStore::new()
            .map_err(|e| format!("Failed to initialize fingerprint store: {}", e))?;
        Ok(Self {
            store: Arc::new(store),
        })
    }

    pub fn new_at(path: impl AsRef<Path>) -> Result<Self, String> {
        let store = FingerprintStore::new_at(path)
            .map_err(|e| format!("Failed to initialize fingerprint store: {}", e))?;
        Ok(Self {
            store: Arc::new(store),
        })
    }
}

/// Get a fingerprint by host and port
#[tauri::command]
pub fn get_fingerprint(
    state: State<'_, FingerprintState>,
    request: GetFingerprintRequest,
) -> Option<StoredFingerprint> {
    state.store.get(&request.host, request.port)
}

/// Save a new fingerprint
#[tauri::command]
pub fn save_fingerprint(
    state: State<'_, FingerprintState>,
    request: SaveFingerprintRequest,
) -> Result<StoredFingerprint, String> {
    state
        .store
        .save(
            &request.host,
            request.port,
            &request.fingerprint,
            &request.algorithm,
            request.server_name.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// Delete a fingerprint by host and port
#[tauri::command]
pub fn delete_fingerprint(
    state: State<'_, FingerprintState>,
    request: DeleteFingerprintRequest,
) -> Result<bool, String> {
    state
        .store
        .delete(&request.host, request.port)
        .map_err(|e| e.to_string())
}

/// Delete a fingerprint by its ID
#[tauri::command]
pub fn delete_fingerprint_by_id(
    state: State<'_, FingerprintState>,
    request: DeleteFingerprintByIdRequest,
) -> Result<bool, String> {
    state
        .store
        .delete_by_id(&request.id)
        .map_err(|e| e.to_string())
}

/// List all stored fingerprints
#[tauri::command]
pub fn list_fingerprints(state: State<'_, FingerprintState>) -> Vec<StoredFingerprint> {
    state.store.list()
}

/// Verify a fingerprint against stored values
#[tauri::command]
pub fn verify_fingerprint(
    state: State<'_, FingerprintState>,
    request: VerifyFingerprintRequest,
) -> VerifyFingerprintResponse {
    let result = state.store.verify(
        &request.host,
        request.port,
        &request.fingerprint,
        &request.algorithm,
    );
    result.into()
}

/// Clear all stored fingerprints (for testing/reset purposes)
#[tauri::command]
pub fn clear_fingerprints(state: State<'_, FingerprintState>) -> Result<(), String> {
    state.store.clear().map_err(|e| e.to_string())
}

/// Request for a handshake-only host-key probe (no credentials are sent).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeHostKeyRequest {
    pub host: String,
    pub port: u16,
}

/// Result of a handshake-only host-key probe.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeHostKeyResponse {
    /// "known" (trusted), "unknown" (first contact), or "changed" (potential MITM)
    pub status: String,
    /// Presented key's SHA256 fingerprint (always present on success)
    pub fingerprint: Option<String>,
    /// Presented key's algorithm, e.g. "ssh-ed25519"
    pub key_type: Option<String>,
    /// Previously stored fingerprint ("changed" only)
    pub stored_fingerprint: Option<String>,
    /// Previously stored key type ("changed" only)
    pub stored_key_type: Option<String>,
    /// When the stored key was first trusted, Unix seconds ("changed" only)
    pub stored_at: Option<i64>,
}

/// Probe a server's host key by performing an SSH transport handshake only —
/// no authentication, so no credentials ever reach the wire. The frontend
/// calls this before `session_connect` to drive the TOFU approval dialogs;
/// the backend still enforces the same policy in `check_server_key`.
#[tauri::command]
pub async fn probe_host_key(
    state: State<'_, FingerprintState>,
    request: ProbeHostKeyRequest,
) -> Result<ProbeHostKeyResponse, String> {
    let (output_tx, _output_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    let mut client = SshClient::new(output_tx);
    client.set_host_key_check(HostKeyCheck::new(
        state.store.clone(),
        request.host.clone(),
        request.port,
    ));

    match client.connect_handshake(&request.host, request.port).await {
        Ok(key) => {
            // Trusted: refresh the last-verified timestamp (best effort).
            let _ = state.store.touch(&request.host, request.port);
            Ok(ProbeHostKeyResponse {
                status: "known".to_string(),
                fingerprint: Some(key.fingerprint),
                key_type: Some(key.algorithm),
                stored_fingerprint: None,
                stored_key_type: None,
                stored_at: None,
            })
        }
        Err(err) => {
            // The handshake was aborted by the host-key policy (or failed for
            // a network reason). Map the structured rejection to the response.
            match client.take_host_key_rejection().await {
                Some(HostKeyRejection::Unknown {
                    fingerprint,
                    algorithm,
                }) => Ok(ProbeHostKeyResponse {
                    status: "unknown".to_string(),
                    fingerprint: Some(fingerprint),
                    key_type: Some(algorithm),
                    stored_fingerprint: None,
                    stored_key_type: None,
                    stored_at: None,
                }),
                Some(HostKeyRejection::Changed {
                    stored_fingerprint,
                    stored_algorithm,
                    stored_at,
                    fingerprint,
                    algorithm,
                }) => Ok(ProbeHostKeyResponse {
                    status: "changed".to_string(),
                    fingerprint: Some(fingerprint),
                    key_type: Some(algorithm),
                    stored_fingerprint: Some(stored_fingerprint),
                    stored_key_type: Some(stored_algorithm),
                    stored_at: Some(stored_at),
                }),
                Some(rejection) => Err(rejection.error_message(&request.host, request.port)),
                None => Err(format!(
                    "Failed to reach {}:{}: {}",
                    request.host, request.port, err
                )),
            }
        }
    }
}
