//! What is installed inside a sandbox.
//!
//! The snapshot comes from the fixed script in the runner image; a refresh asks
//! the runner to take a new one and stores it. Reading is a viewer action,
//! refreshing runs a script in the container and therefore needs write
//! permission.
//!
//! A sandbox with no snapshot is a 200 with `inventory: null`, not a 404: "not
//! taken yet" is the state of every sandbox in its first minute, and a 404 would
//! make the console show an error for a normal situation.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::error::ApiError;
use crate::services::inventory_service::{self, InventoryView};
use crate::state::AppState;

/// The last inventory snapshot, with its age.
pub async fn get(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<InventoryView>, ApiError> {
    let view = inventory_service::load(&state.services, &auth.principal, id).await?;
    Ok(Json(view))
}

/// Take a new inventory snapshot.
pub async fn refresh(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<InventoryView>, ApiError> {
    let view = inventory_service::refresh(&state.services, &auth.principal, id).await?;
    Ok(Json(view))
}
