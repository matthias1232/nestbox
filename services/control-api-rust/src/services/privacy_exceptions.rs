//! DLP exceptions: create, list, delete, and the check the content filter asks.
//!
//! An exception is matched by the salted fingerprint, never by the value, so the
//! table cannot hold content. Whoever may grant one is a policy decision: an
//! administrator always, a sandbox owner only when the environment allows it.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::domain::auth::Principal;
use crate::domain::privacy_exception::PrivacyException;
use crate::error::ApiError;
use crate::services::ServiceContext;
use crate::store::TenantScope;

/// Body of `POST /privacy/exceptions`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NewExceptionRequest {
    pub incident_id: Uuid,
    /// `value` (this value only) or `detector` (any value of this detector).
    pub scope: String,
    /// true: only the incident's sandbox; false: the whole environment (admin only).
    pub sandbox_only: bool,
    /// `allow`, `replace` or `pseudonymize`.
    pub action: String,
    /// 15, 60, 1440, 10080, or `None` for no end.
    pub minutes: Option<i64>,
    #[serde(default)]
    pub reason: String,
}

fn not_implemented() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::NOT_IMPLEMENTED,
        crate::error::ErrorCode::INTERNAL,
        "Not implemented yet.",
    )
}

/// Exceptions that apply to one sandbox now: its own and the environment's.
pub async fn active_for(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Vec<PrivacyException>, ApiError> {
    let _ = (scope, sandbox_id);
    Ok(Vec::new())
}

/// Create an exception from one incident.
pub async fn create(
    _ctx: &ServiceContext,
    _principal: &Principal,
    _request: NewExceptionRequest,
) -> Result<PrivacyException, ApiError> {
    Err(not_implemented())
}

/// The exceptions a principal may see.
pub async fn list(
    _ctx: &ServiceContext,
    _principal: &Principal,
    _sandbox_id: Option<Uuid>,
) -> Result<Vec<PrivacyException>, ApiError> {
    Ok(Vec::new())
}

/// Remove one exception.
pub async fn delete(
    _ctx: &ServiceContext,
    _principal: &Principal,
    _id: Uuid,
) -> Result<(), ApiError> {
    Err(not_implemented())
}

/// A moment `minutes` from now, or none for "never".
pub(crate) fn expires_at(minutes: Option<i64>, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    minutes.map(|minutes| now + chrono::Duration::minutes(minutes))
}
