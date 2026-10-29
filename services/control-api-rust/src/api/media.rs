//! Media rendering settings, the render record, and the MCP bridge.
//!
//! `GET/PUT /media/settings` is the environment default and allow-list.
//! `GET/PUT /sandboxes/{id}/media` is one sandbox's own choice.
//! `POST /internal/media/render` is the one route the sandbox-side MCP server
//! calls; it is not part of the browser API and is authenticated by a token that
//! only the appliance's media service holds.
//!
//! The bridge route is deliberately not reachable with a user session or an API
//! token: a caller that could present one would be able to render without the
//! attachment the control plane checks, which is the grant this feature is built
//! on.

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::api::principal::Auth;
use crate::domain::media::{MediaSettings, SandboxMediaSettings};
use crate::error::{ApiError, ErrorCode};
use crate::services::media_service::{self, MediaRender, MediaView, RenderRequest, SandboxMediaView};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<i64>,
}

/// The environment default and the providers it may point at.
pub async fn get_settings(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<Json<MediaView>, ApiError> {
    Ok(Json(media_service::settings(&state.services, &auth.principal).await?))
}

/// Replace the environment default. Admin only, checked in the service.
pub async fn put_settings(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<MediaSettings>,
) -> Result<Json<MediaView>, ApiError> {
    Ok(Json(media_service::update_settings(&state.services, &auth.principal, body).await?))
}

/// What one sandbox uses, and what it overrides.
pub async fn sandbox_settings(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<SandboxMediaView>, ApiError> {
    Ok(Json(media_service::sandbox(&state.services, &auth.principal, id).await?))
}

/// Set one sandbox's own choice. Every target must be allowed by the environment.
pub async fn put_sandbox_settings(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<SandboxMediaSettings>,
) -> Result<Json<SandboxMediaView>, ApiError> {
    Ok(Json(media_service::set_sandbox(&state.services, &auth.principal, id, body).await?))
}

/// Recent renders for a sandbox.
pub async fn renders(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<MediaRender>>, ApiError> {
    Ok(Json(
        media_service::renders(&state.services, &auth.principal, id, query.limit.unwrap_or(50)).await?,
    ))
}

/// Serve one rendered asset.
///
/// The stored asset name is compared, not concatenated, so a path parameter
/// cannot escape the media root.
pub async fn asset(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, name)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    let (path, mime) = media_service::asset(&state.services, &auth.principal, id, &name).await?;
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ApiError::not_found(ErrorCode::NOT_FOUND, "The asset is no longer on disk."))
        }
        Err(error) => return Err(ApiError::internal(format!("media read: {error}"))),
    };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
        ],
        Body::from(bytes),
    )
        .into_response())
}

/// The token the media MCP server presents.
///
/// Read from a file where a file is available: an environment variable appears
/// in `docker inspect`. Unset means the bridge refuses every call, which is the
/// safe direction for a missing secret.
fn bridge_token() -> Option<String> {
    if let Ok(path) = std::env::var("NESTBOX_MEDIA_BRIDGE_TOKEN_FILE") {
        if let Ok(value) = std::fs::read_to_string(&path) {
            let value = value.trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    std::env::var("NESTBOX_MEDIA_BRIDGE_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn authorised(headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = bridge_token() else {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::INTERNAL,
            "The media bridge has no token configured.",
        ));
    };
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("")
        .trim();
    let expected_hash = Sha256::digest(expected.as_bytes());
    let presented_hash = Sha256::digest(presented.as_bytes());
    if presented.is_empty() || expected_hash.ct_eq(&presented_hash).unwrap_u8() != 1 {
        return Err(ApiError::unauthenticated("The media bridge token is not valid."));
    }
    Ok(())
}

/// Render one asset for a sandbox, called by the media MCP server.
///
/// The body names the sandbox (its config carries the name); the model comes
/// from the settings, never from the body.
pub async fn bridge_render(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RenderRequest>,
) -> Result<Json<MediaRender>, ApiError> {
    authorised(&headers)?;
    let environment = media_service::bridge_environment(&state.services).await?;
    Ok(Json(media_service::render(&state.services, environment, &body).await?))
}

/// The bridge's identity, so the media service can show a clear error before a
/// tool call rather than failing on the first render.
#[derive(Debug, serde::Serialize, ToSchema)]
pub struct BridgeInfo {
    pub enabled: bool,
    pub kinds: Vec<String>,
}

pub async fn bridge_info(State(_state): State<AppState>, headers: HeaderMap) -> Result<Json<BridgeInfo>, ApiError> {
    authorised(&headers)?;
    Ok(Json(BridgeInfo {
        enabled: true,
        kinds: vec!["image".into(), "video".into(), "music".into()],
    }))
}


/// One rendered asset for the MCP bridge, so the agent receives the bytes and
/// not a link it cannot authenticate.
pub async fn bridge_asset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    authorised(&headers)?;
    let environment = media_service::bridge_environment(&state.services).await?;
    let (path, mime) = media_service::bridge_asset(&state.services, environment, id).await?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|error| ApiError::internal(format!("media read: {error}")))?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-store".to_string())],
        Body::from(bytes),
    )
        .into_response())
}
