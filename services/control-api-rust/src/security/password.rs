//! Password hashing with Argon2id.
//!
//! Argon2id rather than a fast hash because the threat is an offline attack on a
//! stolen database, and speed is exactly the wrong property there. The memory
//! cost is why this runs on a blocking thread rather than an async worker: 19
//! MiB of allocation would otherwise stall the runtime.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

/// Argon2 parameters.
///
/// Chosen to take roughly 50–100 ms on a small server: slow enough to make
/// offline guessing expensive, fast enough that a login is not visibly slow.
fn hasher() -> Argon2<'static> {
    // 19 MiB, 2 iterations, 1 lane. The OWASP-recommended baseline.
    //
    // The parameters are constants, so a failure here would be a programming
    // error rather than runtime input. Falling back to Argon2's own defaults
    // keeps the process serving instead of panicking on a login.
    match Params::new(19_456, 2, 1, None) {
        Ok(params) => Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
        Err(_) => Argon2::default(),
    }
}

/// Hash a password for storage.
///
/// Runs on a blocking thread because Argon2 is CPU- and memory-bound.
pub async fn hash_password(password: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        hasher()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

/// Verify a password against a stored hash.
///
/// Returns `false` for a malformed hash rather than an error, so a corrupted row
/// cannot be distinguished from a wrong password by the caller.
pub async fn verify_password(password: String, stored: String) -> bool {
    tokio::task::spawn_blocking(move || {
        let Ok(parsed) = PasswordHash::new(&stored) else {
            return false;
        };
        hasher()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
    .await
    .unwrap_or(false)
}

/// Whether a stored value is a real hash.
///
/// The seed row uses the sentinel `!bootstrap`, which must never verify. Checking
/// the shape explicitly means a sentinel can never accidentally match.
pub fn is_usable_hash(stored: &str) -> bool {
    stored.starts_with("$argon2")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_password_round_trips() {
        let hash = hash_password("correct horse battery staple".into())
            .await
            .expect("hash");
        assert!(verify_password("correct horse battery staple".into(), hash.clone()).await);
        assert!(!verify_password("wrong".into(), hash).await);
    }

    #[tokio::test]
    async fn the_same_password_hashes_differently_each_time() {
        // A per-hash salt means two identical passwords do not share a hash,
        // so the database does not reveal that two users share a password.
        let a = hash_password("same".into()).await.expect("hash a");
        let b = hash_password("same".into()).await.expect("hash b");
        assert_ne!(a, b);
    }

    #[test]
    fn the_bootstrap_sentinel_is_not_a_usable_hash() {
        assert!(!is_usable_hash("!bootstrap"));
        assert!(is_usable_hash("$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA"));
    }

    #[tokio::test]
    async fn a_malformed_hash_does_not_verify() {
        assert!(!verify_password("x".into(), "not-a-hash".into()).await);
        assert!(!verify_password("x".into(), "!bootstrap".into()).await);
    }
}
