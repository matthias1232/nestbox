//! Extra ways into a sandbox's SSH door: operator keys and temporary passwords.
//!
//! Pure rules only: what a valid public key is, how a password is hashed, and
//! when a credential is still good. The same hash function is used by the runner
//! (in Python) to check a login, so the formula is written down once, here.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

/// The longest a temporary credential may live. A password is a convenience for a
/// client without a key, not a standing way in.
pub const MAX_CREDENTIAL_MINUTES: i64 = 24 * 60;
pub const DEFAULT_CREDENTIAL_MINUTES: i64 = 60;
pub const MAX_USER_KEYS_PER_SANDBOX: i64 = 20;
pub const MAX_ACTIVE_CREDENTIALS_PER_SANDBOX: i64 = 5;

/// A public key an operator added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UserKey {
    pub id: Uuid,
    pub label: String,
    pub public_key: String,
    pub fingerprint: String,
    pub created_at: DateTime<Utc>,
}

/// A temporary login, as listed. The password is never part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TempCredential {
    pub id: Uuid,
    pub username: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    /// `active`, `expired` or `revoked`, decided at read time.
    pub state: String,
}

/// What creating a credential returns: the only time the password is visible.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IssuedCredential {
    pub id: Uuid,
    pub username: String,
    pub password: String,
    pub expires_at: DateTime<Utc>,
}

impl TempCredential {
    pub fn state_at(expires_at: DateTime<Utc>, revoked_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> &'static str {
        if revoked_at.is_some() {
            "revoked"
        } else if expires_at <= now {
            "expired"
        } else {
            "active"
        }
    }
}

/// `sha256(salt || password)` as lowercase hex. The runner computes the same.
pub fn hash_password(salt: &str, password: &str) -> String {
    let digest = Sha256::digest(format!("{salt}{password}").as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A requested lifetime, clamped to what is allowed.
pub fn clamp_minutes(requested: Option<i64>) -> i64 {
    requested.unwrap_or(DEFAULT_CREDENTIAL_MINUTES).clamp(1, MAX_CREDENTIAL_MINUTES)
}

pub fn expiry(now: DateTime<Utc>, minutes: i64) -> DateTime<Utc> {
    now + Duration::minutes(clamp_minutes(Some(minutes)))
}

/// Parse one OpenSSH public key line: the normalised line and its fingerprint.
///
/// A private key pasted by mistake is refused by name, because the likeliest
/// mistake is the wrong file and the consequence of storing it is a leak.
pub fn parse_public_key(text: &str) -> Result<(String, String), String> {
    let line = text.trim();
    if line.contains("PRIVATE KEY") {
        return Err("That is a private key. Paste the public key (the .pub file), never the private one.".into());
    }
    if line.lines().count() != 1 {
        return Err("Paste exactly one public key line.".into());
    }
    let key = ssh_key::PublicKey::from_openssh(line)
        .map_err(|_| "That is not an OpenSSH public key (it starts with ssh-ed25519, ssh-rsa or ecdsa-sha2-...).".to_string())?;
    let normalised = key.to_openssh().map_err(|error| error.to_string())?;
    let fingerprint = key.fingerprint(ssh_key::HashAlg::Sha256).to_string();
    Ok((normalised, fingerprint))
}

/// A login name for a temporary credential: not guessable, not a real account.
pub fn username_from(random: u32) -> String {
    format!("tmp-{random:08x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_salted_sha256_hex() {
        // sha256("abcdef"), computed independently.
        assert_eq!(hash_password("abc", "def"), "bef57ec7f53a6d40beb640a780a639c83bc29ac8a9816f1fc6c5c6dcd93c4721");
        assert_eq!(hash_password("a", "bc"), hash_password("ab", "c"), "salt and password are simply concatenated");
        assert_ne!(hash_password("s1", "pw"), hash_password("s2", "pw"));
    }

    #[test]
    fn a_lifetime_is_clamped() {
        assert_eq!(clamp_minutes(None), DEFAULT_CREDENTIAL_MINUTES);
        assert_eq!(clamp_minutes(Some(0)), 1);
        assert_eq!(clamp_minutes(Some(10_000_000)), MAX_CREDENTIAL_MINUTES);
    }

    #[test]
    fn a_credential_is_active_expired_or_revoked() {
        let now = Utc::now();
        assert_eq!(TempCredential::state_at(now + Duration::minutes(5), None, now), "active");
        assert_eq!(TempCredential::state_at(now - Duration::minutes(5), None, now), "expired");
        assert_eq!(TempCredential::state_at(now + Duration::minutes(5), Some(now), now), "revoked");
    }

    #[test]
    fn a_private_key_is_refused_by_name() {
        let error = parse_public_key("-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----").unwrap_err();
        assert!(error.contains("private key"));
    }

    #[test]
    fn junk_and_multiple_lines_are_refused() {
        assert!(parse_public_key("hello world").is_err());
        assert!(parse_public_key("a\nb").is_err());
    }

    #[test]
    fn a_real_public_key_parses_with_a_fingerprint() {
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIECZIlOh/xPXL7cP+DXG0WkxDRpM3r9xm/gaOay5jTiY me@laptop";
        let (line, fingerprint) = parse_public_key(key).expect("valid");
        assert!(line.starts_with("ssh-ed25519 "));
        assert!(fingerprint.starts_with("SHA256:"));
    }

    #[test]
    fn a_username_is_not_a_real_account() {
        assert_eq!(username_from(0xdeadbeef), "tmp-deadbeef");
    }
}
