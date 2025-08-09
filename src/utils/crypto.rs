use aes_gcm::{aead::Aead, Aes256Gcm, Key, KeyInit, Nonce};
use anyhow::{anyhow, Result};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::{Digest, Sha256};

pub const MAGIC_SALT: &[u8] = b".Kita_Ikuyo.^_^.";

/// SHA-256 with salt
pub fn sha256_with_salt(data: &[u8], salt: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::default();
    hasher.update(data);
    hasher.update(salt);
    hasher.finalize().to_vec()
}

/// Cryptographic context for secure communication between client and server
pub struct CryptoContext {
    cipher: Aes256Gcm,
}

impl CryptoContext {
    /// Derives a session key from authentication token and client ID using HKDF-SHA256
    pub fn derive_session_key(token: &str, client_id: &str) -> Result<Vec<u8>> {
        let hk = Hkdf::<Sha256>::new(None, token.as_bytes());
        let mut okm = [0u8; 32]; // 256-bit key
        hk.expand(client_id.as_bytes(), &mut okm)
            .map_err(|_| anyhow!("Failed to derive session key"))?;
        Ok(okm.to_vec())
    }

    /// Creates a new cryptographic context with the given session key
    pub fn new(session_key: &[u8]) -> Result<Self> {
        if session_key.len() != 32 {
            return Err(anyhow!("Session key must be 32 bytes"));
        }

        let key = Key::<Aes256Gcm>::from_slice(session_key);
        let cipher = Aes256Gcm::new(key);

        Ok(CryptoContext { cipher })
    }

    /// Encrypts data using AES-256-GCM with a random nonce
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, data)
            .map_err(|_| anyhow!("Encryption failed"))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// Decrypts data using AES-256-GCM, extracting nonce from the beginning
    pub fn decrypt(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        if encrypted_data.len() < 12 {
            return Err(anyhow!("Invalid encrypted data: too short"));
        }

        let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow!("Decryption failed"))?;

        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_roundtrip() {
        let token = "ciallo";
        let client_id = "0058454c-ba2f-40de-8390-c1bcfc65754f";

        let session_key = CryptoContext::derive_session_key(token, client_id).unwrap();
        let crypto = CryptoContext::new(&session_key).unwrap();

        let original_data = b"Hello, world!";
        let encrypted = crypto.encrypt(original_data).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();

        assert_eq!(original_data, decrypted.as_slice());
    }
}
