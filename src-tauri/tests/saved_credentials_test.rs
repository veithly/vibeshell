//! Opt-in upgrade acceptance against explicit local database snapshots.
//! No network access, credential output, or modification of either snapshot.
use anyhow::{anyhow, Context, Result};
use vibeshell_core::{session::SshCredential, Database};

#[test]
#[ignore = "requires explicit credential snapshots and the original local device key"]
fn saved_keys_survive_upgrade_and_decode_without_changes() -> Result<()> {
    let current = std::env::var("VIBESHELL_CREDENTIAL_SNAPSHOT")
        .context("Set VIBESHELL_CREDENTIAL_SNAPSHOT to a database backup")?;
    let legacy = std::env::var("VIBESHELL_LEGACY_CREDENTIAL_SNAPSHOT")
        .context("Set VIBESHELL_LEGACY_CREDENTIAL_SNAPSHOT to the legacy backup")?;
    let directory = tempfile::tempdir()?;
    let current_path = directory.path().join("current.db");
    let legacy_path = directory.path().join("legacy.db");
    std::fs::copy(current, &current_path)?;
    std::fs::copy(legacy, &legacy_path)?;
    let current = Database::new_at(current_path)?;
    let legacy = Database::new_at(legacy_path)?;
    let mut checked = 0;
    for server in current.server_list(None, None)? {
        let Some(saved) = current.credential_get(&server.name)? else {
            println!(
                "{}: no saved credential; authentication not verified",
                server.name
            );
            continue;
        };
        let original = legacy
            .credential_get(&server.name)?
            .ok_or_else(|| anyhow!("{}: original credential absent", server.name))?;
        // Do not use assert_eq: failure diagnostics must never print secrets.
        assert!(
            saved.credential == original.credential,
            "credential changed during upgrade"
        );
        assert!(
            saved.passphrase == original.passphrase,
            "passphrase changed during upgrade"
        );
        assert!(
            saved.key_path == original.key_path,
            "key path changed during upgrade"
        );
        match SshCredential::from_stored(saved)? {
            SshCredential::PrivateKey { key, passphrase } => {
                russh::keys::decode_secret_key(&key, passphrase.as_deref()).map_err(|_| {
                    anyhow!("{}: saved private key could not be decoded", server.name)
                })?;
                println!(
                    "{}: unchanged credential and passphrase; private key decoded",
                    server.name
                );
            }
            SshCredential::Password(_) => println!("{}: unchanged saved password", server.name),
        }
        checked += 1;
    }
    assert!(checked > 0, "no saved credentials were checked");
    println!("Verified {checked} unchanged saved credentials without network access");
    Ok(())
}
