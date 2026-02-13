use anyhow::{Result, anyhow};
use argon2::{Argon2, password_hash::SaltString};
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

        argon2.hash_password_into(
            password.as_bytes(),
            salt,
            &mut key_bytes,
        ).map_err(|e| anyhow!("Key derivation failed: {}", e))?;

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

        self.key.seal_in_place_append_tag(nonce, Aad::empty(), &mut ciphertext)
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
        self.key.open_in_place(nonce, Aad::empty(), &mut plaintext)
            .map_err(|_| anyhow!("Decryption failed - wrong password?"))?;

        // Remove auth tag
        plaintext.truncate(plaintext.len() - 16);
        Ok(plaintext)
    }
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
    fn test_wrong_password_fails() {
        let salt = Crypto::generate_salt();
        let crypto1 = Crypto::from_password("password1", &salt).unwrap();
        let crypto2 = Crypto::from_password("password2", &salt).unwrap();

        let ciphertext = crypto1.encrypt(b"secret").unwrap();
        let result = crypto2.decrypt(&ciphertext);

        assert!(result.is_err());
    }
}
