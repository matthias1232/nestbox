//! Turning a period of observation into rule drafts in one step (ADR 0053).
//!
//! Everything that was denied, or that only got through because rules were
//! suspended, becomes one allow draft per destination, so the operator reviews
//! a list instead of retyping hosts. Filled by package B10.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::ServiceContext;
use crate::domain::auth::Principal;
use crate::error::ApiError;

/// Draft one allow rule per destination that was denied or only observed.
pub async fn drafts_from_observations(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Option<Uuid>,
    since: Option<DateTime<Utc>>,
) -> Result<serde_json::Value, ApiError> {
    let _ = (ctx, principal, sandbox_id, since);
    Err(ApiError::not_implemented("bulk rule drafts"))
}
