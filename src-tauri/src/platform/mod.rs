mod app_data;
mod capabilities;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) use app_data::{
    copy_legacy_app_data, database_path, default_database_path, default_fingerprint_path,
    fingerprint_path,
};
pub use capabilities::{RuntimeCapabilities, RuntimePlatform};
