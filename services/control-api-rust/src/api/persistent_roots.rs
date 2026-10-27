//! Persistent roots and mounts.
//!
//! A handler parses and delegates: the rule that a host path is derived, that
//! one read-write mount exists per root, and that a root only mounts on its own
//! node all live in `services::persist_service`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::persist::{Mount, PersistentRoot};
use crate::error::ApiError;
use crate::services::persist_service::{self, MountRequest, NewRoot, RootRemoval, RootUsage};
use crate::state::AppState;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRootRequest {
    /// `agent_home` or `project`.
    pub kind: String,
    /// The node the directory lives on, by name. Omitted means the local node.
    #[serde(default)]
    pub node: Option<String>,
    /// Lowercase letters, digits, and hyphens; 1–63 characters.
    pub slug: String,
}

/// Create a root. The host path is derived from kind, owner, and slug, so there
/// is no field to ask for one.
pub async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<CreateRootRequest>,
) -> Result<(StatusCode, Json<PersistentRoot>), ApiError> {
    let root = persist_service::create(
        &state.services,
        &auth.principal,
        NewRoot {
            kind: body.kind,
            node: body.node,
            slug: body.slug,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(root)))
}

/// Every root with where it is mounted.
pub async fn list(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<Vec<PersistentRoot>>, ApiError> {
    let roots = persist_service::list(&state.services, &auth.principal).await?;
    Ok(Json(roots))
}

/// One root with its mounts.
pub async fn get(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<PersistentRoot>, ApiError> {
    let root = persist_service::get(&state.services, &auth.principal, id).await?;
    Ok(Json(root))
}

/// Query for a delete: `?purge=true` deletes the record instead of archiving it.
#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    #[serde(default)]
    pub purge: bool,
}

/// Archive a root (default) or purge its record. A mounted root is refused.
pub async fn delete(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Query(query): Query<DeleteQuery>,
) -> Result<Json<RootRemoval>, ApiError> {
    let outcome =
        persist_service::delete(&state.services, &auth.principal, id, query.purge).await?;
    Ok(Json(outcome))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MountBody {
    pub sandbox_id: Uuid,
    /// The path inside the container; absolute and without `..`.
    pub mount_path: String,
    #[serde(default)]
    pub read_only: bool,
}

/// Mount a root into a sandbox on the same node.
pub async fn mount(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<MountBody>,
) -> Result<(StatusCode, Json<Mount>), ApiError> {
    let mount = persist_service::mount(
        &state.services,
        &auth.principal,
        id,
        MountRequest {
            sandbox_id: body.sandbox_id,
            mount_path: body.mount_path,
            read_only: body.read_only,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(mount)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DismountBody {
    pub sandbox_id: Uuid,
}

/// Dismount a root from a sandbox.
pub async fn dismount(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<DismountBody>,
) -> Result<StatusCode, ApiError> {
    persist_service::dismount(&state.services, &auth.principal, id, body.sandbox_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The stored measurement of a root, or an honest "not measured".
pub async fn usage(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<RootUsage>, ApiError> {
    let usage = persist_service::usage(&state.services, &auth.principal, id).await?;
    Ok(Json(usage))
}
