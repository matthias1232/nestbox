//! Permission evaluation.
//!
//! One function, so the refusal message and the audit signal are identical
//! everywhere.

use crate::domain::auth::{Permission, Principal};
use crate::error::ApiError;

/// Check a permission, returning the standard refusal when it is missing.
///
/// The error names the permission, so an operator who is refused learns what
/// they lack instead of seeing "forbidden".
pub fn require(principal: &Principal, permission: Permission) -> Result<(), ApiError> {
    if principal.can(permission) {
        Ok(())
    } else {
        Err(ApiError::forbidden(permission.code()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::auth::{CredentialSource, Role};
    use uuid::Uuid;

    fn principal(role: Role) -> Principal {
        Principal {
            user_id: Uuid::nil(),
            environment_id: Uuid::nil(),
            role,
            display_name: "t".into(),
            via: CredentialSource::Session,
        }
    }

    #[test]
    fn a_viewer_is_refused_a_write_and_told_which_permission() {
        let error = require(&principal(Role::Viewer), Permission::SandboxWrite).unwrap_err();
        assert_eq!(error.status, axum::http::StatusCode::FORBIDDEN);
        assert!(error.details.to_string().contains("sandbox.write"));
    }

    #[test]
    fn an_admin_is_allowed_everything() {
        for permission in [
            Permission::SandboxWrite,
            Permission::SandboxExec,
            Permission::PolicyGlobal,
            Permission::GatewayWrite,
        ] {
            assert!(require(&principal(Role::Admin), permission).is_ok());
        }
    }
}
