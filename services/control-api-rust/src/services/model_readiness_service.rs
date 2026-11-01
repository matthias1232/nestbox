//! Making the models a sandbox was granted ready before it is used (ADR 0056).
//!
//! A local model on a node is started if it is stopped, asked one tiny question,
//! and only then is the sandbox reported ready, so the first real request does
//! not wait for a model load on an expensive machine. Filled by package B22.

use uuid::Uuid;

use super::ServiceContext;
use crate::domain::auth::Principal;
use crate::error::ApiError;

/// Start and warm the local models of a sandbox when it starts.
pub async fn ensure_ready_for_sandbox(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
) -> Result<(), ApiError> {
    let _ = (ctx, environment_id, sandbox_id);
    Ok(())
}

/// Warm the granted local models of a sandbox on the node it moves to.
pub async fn prewarm_on_node(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
    node_id: Uuid,
) -> Result<(), ApiError> {
    let _ = (ctx, environment_id, sandbox_id, node_id);
    Ok(())
}

/// Warm one model on demand.
pub async fn warm(
    ctx: &ServiceContext,
    principal: &Principal,
    model_key: &str,
) -> Result<serde_json::Value, ApiError> {
    let _ = (ctx, principal, model_key);
    Err(ApiError::not_implemented("warming a model on demand"))
}

/// Keep the models marked `keep_warm` answering.
pub async fn keep_warm_tick(ctx: &ServiceContext) -> Result<(), ApiError> {
    let _ = ctx;
    Ok(())
}
