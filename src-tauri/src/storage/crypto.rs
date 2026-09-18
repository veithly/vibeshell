use anyhow::{anyhow, Context, Result};
use argon2::{password_hash::SaltString, Argon2};
use rand::rngs::OsRng;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

pub struct Crypto {
    key: LessSafeKey,
}

impl Crypto {
    /// Create crypto instance from master password
    pub fn from_password(password: &str, salt: &[u8]) -> Result<Self> {
        let argon2 = Argon2::default();
        let mut key_bytes = [0u8; KEY_LEN];

        argon2
            .hash_password_into(password.as_bytes(), salt, &mut key_bytes)
            .map_err(|e| anyhow!("Key derivation failed: {}", e))?;

        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
            .map_err(|_| anyhow!("Failed to create encryption key"))?;

        Ok(Self {
            key: LessSafeKey::new(unbound_key),
        })
    }

    /// Generate a new random salt
    pub fn generate_salt() -> Vec<u8> {
        let salt = SaltString::generate(&mut OsRng);
        salt.as_str().as_bytes().to_vec()
    }

    /// Encrypt data
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| anyhow!("Failed to generate nonce"))?;

        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut ciphertext = plaintext.to_vec();

        self.key
            .seal_in_place_append_tag(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| anyhow!("Encryption failed"))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);
        Ok(result)
    }

    /// Decrypt data
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < NONCE_LEN {
            return Err(anyhow!("Ciphertext too short"));
        }

        let (nonce_bytes, encrypted) = ciphertext.split_at(NONCE_LEN);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes.try_into().unwrap());

        let mut plaintext = encrypted.to_vec();
        self.key
            .open_in_place(nonce, Aad::empty(), &mut plaintext)
            .map_err(|_| anyhow!("Decryption failed - wrong password?"))?;

        // Remove auth tag
        plaintext.truncate(plaintext.len() - 16);
        Ok(plaintext)
    }

    /// Encrypt to a base64 string suitable for storage in a TEXT column.
    pub fn encrypt_base64(&self, plaintext: &[u8]) -> Result<String> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        Ok(STANDARD.encode(self.encrypt(plaintext)?))
    }

    /// Decrypt a value produced by [`Crypto::encrypt_base64`].
    pub fn decrypt_base64(&self, encoded: &str) -> Result<Vec<u8>> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let ciphertext = STANDARD
            .decode(encoded)
            .map_err(|e| anyhow!("Stored value is not valid base64: {}", e))?;
        self.decrypt(&ciphertext)
    }

    /// Process-wide crypto instance backed by a device-local key file (mode
    /// 0600 on unix). The key never leaves the device. Used for at-rest
    /// encryption of stored SSH credentials in `storage::database`; `dbconn`
    /// keeps its own independently generated key files.
    pub fn device() -> Result<&'static Self> {
        use std::sync::OnceLock;
        static DEVICE: OnceLock<Crypto> = OnceLock::new();
        if let Some(crypto) = DEVICE.get() {
            return Ok(crypto);
        }
        // Unit tests must not read, create or chmod the user's device keys.
        // File-backed behavior is covered separately with explicit tempdirs.
        #[cfg(test)]
        let crypto = Self::from_password("unit-test-device", &Self::generate_salt())?;
        #[cfg(not(test))]
        let crypto = Self::from_device_key_files("ssh_credentials")?;
        let _ = DEVICE.set(crypto);
        DEVICE
            .get()
            .ok_or_else(|| anyhow!("Device crypto failed to initialize"))
    }

    /// Shared loader; SSH and database passwords retain independent legacy keys.
    pub(crate) fn from_device_key_files(prefix: &str) -> Result<Self> {
        let dirs = directories::ProjectDirs::from("com", "vibeshell", "VibeShell")
            .ok_or_else(|| anyhow!("Could not determine the device key directory"))?;
        Self::from_key_files(dirs.data_dir(), prefix)
    }

    fn from_key_files(app_dir: &std::path::Path, prefix: &str) -> Result<Self> {
        use std::fs::OpenOptions;
        use std::io::{ErrorKind, Write};

        std::fs::create_dir_all(app_dir)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        // Serialize both first-time threads and GUI/daemon processes. The lock
        // is released on every return path; never replace an unreadable key.
        let lock = options.open(app_dir.join(format!("{prefix}.lock")))?;
        lock.lock().context("Cannot lock device key files")?;
        let key_path = app_dir.join(format!("{prefix}.key"));
        let salt_path = app_dir.join(format!("{prefix}.salt"));
        let (key_hex, salt) = match (
            std::fs::read_to_string(&key_path),
            std::fs::read(&salt_path),
        ) {
            (Ok(key), Ok(salt)) => (key, salt),
            (Err(key_error), Err(salt_error))
                if key_error.kind() == ErrorKind::NotFound
                    && salt_error.kind() == ErrorKind::NotFound =>
            {
                let mut bytes = [0u8; KEY_LEN];
                SystemRandom::new()
                    .fill(&mut bytes)
                    .map_err(|_| anyhow!("Failed to generate device key"))?;
                let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
                let salt = Self::generate_salt();
                for (path, content) in [(&key_path, hex.as_bytes()), (&salt_path, salt.as_slice())]
                {
                    let mut create = OpenOptions::new();
                    create.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        create.mode(0o600);
                    }
                    let mut file = create.open(path)?;
                    file.write_all(content)?;
                    file.sync_all()?;
                }
                (hex, salt)
            }
            (Err(error), _) | (_, Err(error)) => {
                return Err(error).context("Device key files are missing or unreadable; refusing to replace existing key material");
            }
        };
        let key_hex = key_hex.trim();
        if key_hex.len() != KEY_LEN * 2
            || !key_hex.bytes().all(|byte| byte.is_ascii_hexdigit())
            || salt.len() < 8
        {
            return Err(anyhow!("Invalid device key files; restore them from backup instead of generating a new key"));
        }
        set_private(&key_path)?;
        set_private(&salt_path)?;
        Self::from_password(key_hex, &salt)
    }
}

#[cfg(unix)]
fn set_private(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let salt = Crypto::generate_salt();
        let crypto = Crypto::from_password("test_password", &salt).unwrap();

        let plaintext = b"Hello, World!";
        let ciphertext = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&ciphertext).unwrap();

        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn device_key_initializers_agree_and_keep_private_files() {
        let directory = tempfile::tempdir().unwrap();
        let encrypted = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        Crypto::from_key_files(directory.path(), "test")
                            .unwrap()
                            .encrypt(b"saved password")
                            .unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        let loaded = Crypto::from_key_files(directory.path(), "test").unwrap();
        for ciphertext in encrypted {
            assert_eq!(loaded.decrypt(&ciphertext).unwrap(), b"saved password");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in ["test.key", "test.salt"] {
                assert_eq!(
                    std::fs::metadata(directory.path().join(name))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
        }
    }

    #[test]
    fn corrupt_or_incomplete_key_files_are_never_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let key_path = directory.path().join("test.key");
        let salt_path = directory.path().join("test.salt");
        std::fs::write(&key_path, "corrupt-key").unwrap();
        assert!(Crypto::from_key_files(directory.path(), "test").is_err());
        assert_eq!(std::fs::read_to_string(&key_path).unwrap(), "corrupt-key");
        assert!(!salt_path.exists());
        std::fs::write(&salt_path, b"legacy-salt").unwrap();
        assert!(Crypto::from_key_files(directory.path(), "test").is_err());
        assert_eq!(std::fs::read_to_string(&key_path).unwrap(), "corrupt-key");
        assert_eq!(std::fs::read(&salt_path).unwrap(), b"legacy-salt");
    }

    #[test]
    fn legacy_device_key_material_remains_compatible() {
        let directory = tempfile::tempdir().unwrap();
        let key = "ab".repeat(32);
        let salt = Crypto::generate_salt();
        let original = Crypto::from_password(&key, &salt)
            .unwrap()
            .encrypt(b"existing secret")
            .unwrap();
        std::fs::write(directory.path().join("dbconn.key"), &key).unwrap();
        std::fs::write(directory.path().join("dbconn.salt"), &salt).unwrap();
        let loaded = Crypto::from_key_files(directory.path(), "dbconn").unwrap();
        assert_eq!(loaded.decrypt(&original).unwrap(), b"existing secret");
    }

    #[test]
    fn test_wrong_password_fails() {
        let salt = Crypto::generate_salt();
        let crypto1 = Crypto::from_password("password1", &salt).unwrap();
        let crypto2 = Crypto::from_password("password2", &salt).unwrap();

        let ciphertext = crypto1.encrypt(b"secret").unwrap();
        let result = crypto2.decrypt(&ciphertext);

        assert!(result.is_err());
    }
}
