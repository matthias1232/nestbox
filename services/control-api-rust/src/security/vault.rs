//! Encryption of stored secrets.
//!
//! AES-256-GCM with a random 96-bit nonce per value. The key is a 32-byte master key
//! read from a file in the secret directory (`vault_key`, 64 hex characters), which
//! the database never sees: a copy of the database is not a copy of the secrets. The
//! associated data binds a ciphertext to its environment and name, so a row copied
//! under another name or into another tenant does not decrypt.

use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;

#[derive(Debug, PartialEq, Eq)]
pub enum VaultError {
    /// No master key file, or it is not 64 hex characters.
    NoKey,
    /// The value could not be encrypted, or the ciphertext did not authenticate.
    Crypto,
}

impl VaultError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::NoKey => "The secret store has no usable master key (vault_key).",
            Self::Crypto => "The stored secret could not be decrypted.",
        }
    }
}

/// Read the master key.
pub fn master_key(secret_dir: &Path) -> Result<[u8; 32], VaultError> {
    let text = std::fs::read_to_string(secret_dir.join("vault_key")).map_err(|_| VaultError::NoKey)?;
    parse_key(text.trim())
}

pub fn parse_key(hex: &str) -> Result<[u8; 32], VaultError> {
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(VaultError::NoKey);
    }
    let mut key = [0u8; 32];
    for (index, slot) in key.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).map_err(|_| VaultError::NoKey)?;
    }
    Ok(key)
}

fn aad(environment: &str, name: &str) -> Vec<u8> {
    format!("nestbox-secret:{environment}:{name}").into_bytes()
}

/// Encrypt a value. Returns `(ciphertext, nonce)`.
pub fn seal(key: &[u8; 32], environment: &str, name: &str, value: &str) -> Result<(Vec<u8>, Vec<u8>), VaultError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::Crypto)?;
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: value.as_bytes(), aad: &aad(environment, name) })
        .map_err(|_| VaultError::Crypto)?;
    Ok((ciphertext, nonce.to_vec()))
}

/// Decrypt a value.
pub fn open(key: &[u8; 32], environment: &str, name: &str, ciphertext: &[u8], nonce: &[u8]) -> Result<String, VaultError> {
    if nonce.len() != 12 {
        return Err(VaultError::Crypto);
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::Crypto)?;
    let plain = cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: &aad(environment, name) })
        .map_err(|_| VaultError::Crypto)?;
    String::from_utf8(plain).map_err(|_| VaultError::Crypto)
}

/// Last four characters, for display.
pub fn last4(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    chars[chars.len().saturating_sub(4)..].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; 32] {
        parse_key(&"ab".repeat(32)).unwrap()
    }

    #[test]
    fn a_value_round_trips_and_the_ciphertext_is_not_the_value() {
        let (cipher, nonce) = seal(&key(), "env-1", "openai", "sk-secret-value").unwrap();
        assert!(!String::from_utf8_lossy(&cipher).contains("sk-secret"));
        assert_eq!(open(&key(), "env-1", "openai", &cipher, &nonce).unwrap(), "sk-secret-value");
    }

    #[test]
    fn every_seal_uses_a_new_nonce() {
        let a = seal(&key(), "e", "n", "v").unwrap();
        let b = seal(&key(), "e", "n", "v").unwrap();
        assert_ne!(a.1, b.1);
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn a_ciphertext_does_not_open_under_another_name_environment_or_key() {
        let (cipher, nonce) = seal(&key(), "env-1", "openai", "v").unwrap();
        assert!(open(&key(), "env-1", "other", &cipher, &nonce).is_err());
        assert!(open(&key(), "env-2", "openai", &cipher, &nonce).is_err());
        assert!(open(&parse_key(&"cd".repeat(32)).unwrap(), "env-1", "openai", &cipher, &nonce).is_err());
    }

    #[test]
    fn a_tampered_ciphertext_is_refused() {
        let (mut cipher, nonce) = seal(&key(), "e", "n", "value").unwrap();
        cipher[0] ^= 1;
        assert!(open(&key(), "e", "n", &cipher, &nonce).is_err());
    }

    #[test]
    fn a_master_key_must_be_64_hex_characters() {
        assert!(parse_key("short").is_err());
        assert!(parse_key(&"zz".repeat(32)).is_err());
        assert!(parse_key(&"0f".repeat(32)).is_ok());
    }

    #[test]
    fn only_the_last_four_characters_are_shown() {
        assert_eq!(last4("sk-abcdefgh1234"), "1234");
        assert_eq!(last4("ab"), "ab");
    }
}
