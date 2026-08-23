pub mod crypto;
pub mod database;
pub mod models;
pub mod sync;
pub mod sync_crypto;

pub use crypto::Crypto;
pub use database::{Database, Group};
pub use models::*;
pub use sync::{
    CloudSyncStorage, ConflictResolution, PendingSyncBatch, PendingSyncUpload, RemoteApplyReport,
    SyncChange, SyncConflict, SyncEntityKind, SYNC_CHANGE_SCHEMA_VERSION,
};
pub use sync_crypto::{
    decrypt_batch, encrypt_batch, EncryptedSyncBatch, SyncBatchContext, SyncCryptoError, VaultKey,
    SYNC_CRYPTO_ALGORITHM, SYNC_CRYPTO_WIRE_VERSION,
};

// Re-export new model types
pub use models::{
    CommandHistoryEntry, CommandSnippet, Recording, TunnelConfig, TunnelInfo, TunnelStatus,
    TunnelType,
};
