//! Tauri commands for SSH fingerprint management
//!
//! These commands provide the frontend with access to fingerprint storage
//! for verifying SSH server host keys and managing trusted hosts.

use serde::{Deserialize, Serialize};
use tauri::State;
use std::sync::Arc;

use crate::ssh::{FingerprintStore, StoredFingerprint, FingerprintVerificationResult};

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
    pub status: String,  // "trusted", "unknown", or "changed"
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
            FingerprintVerificationResult::Unknown { fingerprint, algorithm } => VerifyFingerprintResponse {
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
    state.store.save(
        &request.host,
        request.port,
        &request.fingerprint,
        &request.algorithm,
        request.server_name.as_deref(),
    ).map_err(|e| e.to_string())
}

/// Delete a fingerprint by host and port
#[tauri::command]
pub fn delete_fingerprint(
    state: State<'_, FingerprintState>,
    request: DeleteFingerprintRequest,
) -> Result<bool, String> {
    state.store.delete(&request.host, request.port)
        .map_err(|e| e.to_string())
}

/// Delete a fingerprint by its ID
#[tauri::command]
pub fn delete_fingerprint_by_id(
    state: State<'_, FingerprintState>,
    request: DeleteFingerprintByIdRequest,
) -> Result<bool, String> {
    state.store.delete_by_id(&request.id)
        .map_err(|e| e.to_string())
}

/// List all stored fingerprints
#[tauri::command]
pub fn list_fingerprints(
    state: State<'_, FingerprintState>,
) -> Vec<StoredFingerprint> {
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

/// Update the last_verified_at timestamp for a host
#[tauri::command]
pub fn touch_fingerprint(
    state: State<'_, FingerprintState>,
    request: GetFingerprintRequest,
) -> Result<(), String> {
    state.store.touch(&request.host, request.port)
        .map_err(|e| e.to_string())
}

/// Clear all stored fingerprints (for testing/reset purposes)
#[tauri::command]
pub fn clear_fingerprints(
    state: State<'_, FingerprintState>,
) -> Result<(), String> {
    state.store.clear()
        .map_err(|e| e.to_string())
}
