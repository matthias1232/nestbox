//! Endpunkt für den Pack-Katalog.

use axum::Json;

use crate::domain::pack::{Pack, catalog};

#[utoipa::path(
    get,
    path = "/v1/packs",
    tag = "sandboxes",
    summary = "Freigegebene Agent-Packs",
    responses((status = 200, body = Vec<Pack>))
)]
pub async fn list() -> Json<Vec<Pack>> {
    Json(catalog())
}
