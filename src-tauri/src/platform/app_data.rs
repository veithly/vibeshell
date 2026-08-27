use anyhow::Result;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use anyhow::Context;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const APP_BUNDLE_IDENTIFIER: &str = "com.vibeshell.desktop";
pub(crate) const DATABASE_FILE_NAME: &str = "vibeshell.db";
pub(crate) const FINGERPRINT_FILE_NAME: &str = "ssh_fingerprints.json";

pub(crate) fn database_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(DATABASE_FILE_NAME)
}

pub(crate) fn fingerprint_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(FINGERPRINT_FILE_NAME)
}

/// Same value as the GUI's `app.path().app_data_dir()`, resolved without a
/// running Tauri runtime so headless entry points (CLI daemon, import) share
/// one data directory with the desktop application.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn default_app_data_dir() -> Result<PathBuf> {
    let base_dirs = directories::BaseDirs::new()
        .context("Could not determine platform base directories")?;
    Ok(base_dirs.data_dir().join(APP_BUNDLE_IDENTIFIER))
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn default_app_data_dir() -> Result<PathBuf> {
    anyhow::bail!(
        "The default {} data directory can only be resolved on desktop platforms",
        APP_BUNDLE_IDENTIFIER
    );
}

/// Default database location for entry points without an AppHandle. Creates the
/// directory and performs the same one-time legacy migration as GUI startup.
pub(crate) fn default_database_path() -> Result<PathBuf> {
    prepared_default_app_data_dir().map(|dir| database_path(&dir))
}

/// Default fingerprint-store location for entry points without an AppHandle.
pub(crate) fn default_fingerprint_path() -> Result<PathBuf> {
    prepared_default_app_data_dir().map(|dir| fingerprint_path(&dir))
}

fn prepared_default_app_data_dir() -> Result<PathBuf> {
    let dir = default_app_data_dir()?;
    fs::create_dir_all(&dir)?;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    copy_legacy_app_data(&dir)?;
    Ok(dir)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn copy_legacy_app_data(app_data_dir: &Path) -> Result<()> {
    use directories::ProjectDirs;

    let legacy_dir = ProjectDirs::from("com", "vibeshell", "VibeShell")
        .context("Could not determine legacy VibeShell data directory")?
        .data_dir()
        .to_path_buf();

    copy_if_missing(
        &legacy_dir.join(DATABASE_FILE_NAME),
        &database_path(app_data_dir),
    )?;
    copy_if_missing(
        &legacy_dir.join(FINGERPRINT_FILE_NAME),
        &fingerprint_path(app_data_dir),
    )?;
    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn copy_if_missing(source: &Path, destination: &Path) -> Result<bool> {
    if destination.exists() || !source.exists() {
        return Ok(false);
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create app data directory {}", parent.display()))?;
    }

    log::info!(
        "[VibeShell] Copying legacy app data from {} to {}",
        source.display(),
        destination.display()
    );
    let temporary = destination.with_extension("migration-copy");
    fs::copy(source, &temporary).with_context(|| {
        format!(
            "Failed to copy legacy app data from {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    fs::rename(&temporary, destination).with_context(|| {
        format!(
            "Failed to finalize legacy app data copy from {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(true)
}

#[cfg(all(test, not(any(target_os = "android", target_os = "ios"))))]
mod tests {
    use super::*;

    #[test]
    fn bundle_identifier_matches_tauri_configuration() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let raw = fs::read_to_string(manifest_dir.join("tauri.conf.json"))
            .expect("tauri.conf.json must exist next to the crate manifest");
        let config: serde_json::Value =
            serde_json::from_str(&raw).expect("tauri.conf.json must be valid JSON");

        assert_eq!(
            APP_BUNDLE_IDENTIFIER,
            config["identifier"].as_str().expect("identifier field"),
            "APP_BUNDLE_IDENTIFIER and the default data dir must track the Tauri bundle identifier"
        );
    }

    #[test]
    fn copies_legacy_file_without_removing_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("legacy").join(DATABASE_FILE_NAME);
        let destination = temp.path().join("current").join(DATABASE_FILE_NAME);
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"legacy database").unwrap();

        assert!(copy_if_missing(&source, &destination).unwrap());
        assert_eq!(fs::read(&destination).unwrap(), b"legacy database");
        assert_eq!(fs::read(&source).unwrap(), b"legacy database");
    }

    #[test]
    fn never_overwrites_current_app_data() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("legacy.json");
        let destination = temp.path().join("current.json");
        fs::write(&source, b"legacy").unwrap();
        fs::write(&destination, b"current").unwrap();

        assert!(!copy_if_missing(&source, &destination).unwrap());
        assert_eq!(fs::read(&destination).unwrap(), b"current");
        assert_eq!(fs::read(&source).unwrap(), b"legacy");
    }

    #[test]
    fn does_nothing_when_legacy_file_is_absent() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("missing.db");
        let destination = temp.path().join("current").join(DATABASE_FILE_NAME);

        assert!(!copy_if_missing(&source, &destination).unwrap());
        assert!(!destination.exists());
    }

    #[test]
    fn copy_failure_is_returned_without_creating_the_destination() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("legacy-directory");
        let destination = temp.path().join("current").join(DATABASE_FILE_NAME);
        fs::create_dir_all(&source).unwrap();

        assert!(copy_if_missing(&source, &destination).is_err());
        assert!(source.exists());
        assert!(!destination.exists());
    }
}
