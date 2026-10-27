//! Resolving a secret locator to its value.
//!
//! The database stores a locator such as `file:///secrets/omniroute_api_key`
//! and never the value (ADR 0005). This module is the one place that turns a
//! locator into a string, and it is deliberately narrow:
//!
//! * only the `file://` scheme is implemented, because that is the only backend
//!   the appliance has. `vault://` and `k8s://` are refused with an explanation
//!   rather than half-implemented.
//! * a path must resolve inside the configured secret directory. A locator that
//!   escapes it is refused, so a bug in the console cannot turn into a file
//!   disclosure.
//! * the value is never logged and never returned by an HTTP handler. It is
//!   passed straight into a container environment or an outbound request.

use std::path::{Path, PathBuf};

use crate::error::{ApiError, ErrorCode};

/// Why a locator could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
    /// The scheme is not implemented.
    UnsupportedScheme,
    /// The locator is malformed.
    Invalid,
    /// The path leaves the secret directory.
    OutsideSecretDir,
    /// The file is missing or unreadable.
    Unreadable,
    /// The file exists but is world-readable, so it may have leaked.
    TooPermissive,
}

impl SecretError {
    /// A message that names the problem without naming the secret.
    pub fn message(self) -> &'static str {
        match self {
            Self::UnsupportedScheme => {
                "Only file:// secret locators are supported in this phase."
            }
            Self::Invalid => "The secret locator is not a valid URI.",
            Self::OutsideSecretDir => "The secret locator must resolve inside the secret directory.",
            Self::Unreadable => "The secret file does not exist or cannot be read.",
            Self::TooPermissive => "The secret file must not be readable by group or world.",
        }
    }

    pub fn to_api(self) -> ApiError {
        ApiError::new(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::SECRET_UNRESOLVABLE,
            self.message(),
        )
    }
}

/// Read the value a locator points at.
///
/// Synchronous on purpose: the caller is either building a request payload or
/// validating a profile at write time, and both already run off the reactor in
/// practice. A blocking file read of a few bytes does not deserve a thread
/// spawn, and making it async would hide that it is I/O under a lock-free
/// façade.
pub fn resolve(locator: &str, secret_dir: &Path) -> Result<String, SecretError> {
    let rest = locator
        .strip_prefix("file://")
        .ok_or(SecretError::UnsupportedScheme)?;

    if rest.is_empty() {
        return Err(SecretError::Invalid);
    }

    let candidate = PathBuf::from(rest);
    if !candidate.is_absolute() {
        return Err(SecretError::Invalid);
    }

    // Canonicalise both sides so `..` and symlinks cannot escape the directory.
    let root = secret_dir
        .canonicalize()
        .map_err(|_| SecretError::Unreadable)?;
    let resolved = candidate
        .canonicalize()
        .map_err(|_| SecretError::Unreadable)?;
    if !resolved.starts_with(&root) {
        return Err(SecretError::OutsideSecretDir);
    }

    check_permissions(&resolved)?;

    let value = std::fs::read_to_string(&resolved).map_err(|_| SecretError::Unreadable)?;
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(SecretError::Unreadable);
    }
    Ok(trimmed)
}

/// Refuse a secret file that group or world can read.
///
/// This is a guard, not a guarantee: the file may have been readable earlier.
/// It still catches the common mistake of copying a key in with default
/// permissions, which is the moment the window is widest.
fn check_permissions(path: &Path) -> Result<(), SecretError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)
            .map_err(|_| SecretError::Unreadable)?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(SecretError::TooPermissive);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nestbox-secret-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_file_locator_inside_the_directory_is_read() {
        let dir = temp_dir("ok");
        let file = dir.join("key");
        std::fs::write(&file, "s3cret\n").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).expect("chmod");
        }
        let locator = format!("file://{}", file.display());
        assert_eq!(resolve(&locator, &dir).expect("resolve"), "s3cret");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unsupported_scheme_is_named() {
        let dir = temp_dir("scheme");
        assert_eq!(
            resolve("vault://team/omniroute", &dir),
            Err(SecretError::UnsupportedScheme)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_outside_the_directory_is_refused() {
        let dir = temp_dir("escape");
        let locator = "file:///etc/passwd".to_string();
        assert_eq!(
            resolve(&locator, &dir),
            Err(SecretError::OutsideSecretDir)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_world_readable_file_is_refused() {
        let dir = temp_dir("perms");
        let file = dir.join("loose");
        std::fs::write(&file, "value").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).expect("chmod");
            let locator = format!("file://{}", file.display());
            assert_eq!(
                resolve(&locator, &dir),
                Err(SecretError::TooPermissive)
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
