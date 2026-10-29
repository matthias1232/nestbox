//! Media rendering: settings, the effective model, and the provider call.
//!
//! # Why the control plane performs the render
//!
//! The MCP server inside the sandbox does not hold a provider credential and
//! does not know which model the environment selected. It asks this service,
//! which resolves the choice, checks the allow-list and the attachment, and only
//! then makes the outbound call. Two consequences are deliberate:
//!
//! 1. A sandbox cannot name a model it was not given. The model arrives from
//!    the settings tables, never from the tool arguments.
//! 2. A credential never enters the sandbox. The provider secret is resolved
//!    here, used for one request, and dropped.
//!
//! The prompt is stored on the render row because it is the record of what was
//! asked; it is bounded and never contains a credential.

use std::path::PathBuf;

use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::ToSchema;
use uuid::Uuid;

use super::access_control::require;
use super::{secret_service, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::media::{
    effective, refusal, MediaKind, MediaSettings, MediaTarget, SandboxMediaSettings,
};
use crate::domain::provider::ProviderProfile;
use crate::error::{ApiError, ErrorCode};
use crate::store::postgres::media as store;
use crate::store::postgres::providers as provider_store;
use crate::store::TenantScope;

/// The largest prompt a render accepts. Longer input is a 422, not a truncated
/// prompt, because truncation would silently change what was asked.
const MAX_PROMPT: usize = 4000;

/// One provider a sandbox owner may choose, as the console shows it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MediaProvider {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    /// The model the profile itself names, when it names one. A hint: the
    /// media model is a separate field because an image model is rarely the
    /// profile's text model.
    pub model: Option<String>,
    pub allowed: bool,
}

/// The settings screen: the environment default, and what may be chosen.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MediaView {
    pub settings: MediaSettings,
    pub providers: Vec<MediaProvider>,
}

/// What one sandbox would use, per kind, and whether it is inherited.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SandboxMediaView {
    pub sandbox_id: Uuid,
    pub effective: EffectiveMedia,
    pub override_settings: SandboxMediaSettings,
    pub providers: Vec<MediaProvider>,
    pub inherited: InheritedMedia,
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct EffectiveMedia {
    pub image: Option<MediaTarget>,
    pub video: Option<MediaTarget>,
    pub music: Option<MediaTarget>,
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct InheritedMedia {
    pub image: bool,
    pub video: bool,
    pub music: bool,
}

/// A render as the console lists it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MediaRender {
    pub id: Uuid,
    pub sandbox_id: Uuid,
    pub kind: MediaKind,
    pub provider_name: Option<String>,
    pub model: String,
    pub prompt: String,
    pub status: String,
    pub asset_url: Option<String>,
    pub mime_type: Option<String>,
    pub bytes: Option<i64>,
    pub error: Option<String>,
    pub duration_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// What the MCP bridge sends. The sandbox is named by its config, the model is
/// never in this body.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RenderRequest {
    pub sandbox: String,
    pub kind: String,
    pub prompt: String,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub duration_seconds: Option<u32>,
}

/// Where rendered assets live. A deployment sets `NESTBOX_MEDIA_DIR`; the
/// default is the same path the compose file mounts.
fn media_dir() -> PathBuf {
    std::env::var("NESTBOX_MEDIA_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/nestbox/media"))
}

fn provider_views(profiles: &[ProviderProfile], settings: &MediaSettings) -> Vec<MediaProvider> {
    profiles
        .iter()
        .map(|profile| MediaProvider {
            id: profile.id,
            name: profile.name.clone(),
            kind: profile.kind.as_str().to_string(),
            model: profile.model.clone(),
            allowed: settings.permits(profile.id, true),
        })
        .collect()
}

/// Read the environment default and the providers it may point at.
pub async fn settings(ctx: &ServiceContext, principal: &Principal) -> Result<MediaView, ApiError> {
    require(principal, Permission::MediaRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(ApiError::from)?;
    let settings = store::settings(&mut scope).await.map_err(ApiError::from)?;
    let profiles = provider_store::list(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(MediaView { providers: provider_views(&profiles, &settings), settings })
}

/// Replace the environment default. Admin only: this is what every sandbox
/// inherits and what the allow-list restricts.
pub async fn update_settings(
    ctx: &ServiceContext,
    principal: &Principal,
    settings: MediaSettings,
) -> Result<MediaView, ApiError> {
    require(principal, Permission::MediaConfigure)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(ApiError::from)?;
    let profiles = provider_store::list(&mut scope).await.map_err(ApiError::from)?;
    let known: Vec<Uuid> = profiles.iter().map(|profile| profile.id).collect();
    for target in [&settings.image, &settings.video, &settings.music].into_iter().flatten() {
        if !known.contains(&target.provider_id) {
            return Err(ApiError::validation(
                "The selected provider does not exist in this environment.",
            ));
        }
    }
    for provider_id in &settings.allowed_provider_ids {
        if !known.contains(provider_id) {
            return Err(ApiError::validation(
                "The allow-list names a provider that does not exist in this environment.",
            ));
        }
    }
    store::save_settings(&mut scope, &settings, Some(principal.user_id))
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "media.settings",
            subject_type: Some("environment"),
            subject_id: Some(principal.environment_id),
            detail: &json!({ "enabled": settings.enabled, "allowed": settings.allowed_provider_ids.len() }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    settings(ctx, principal).await
}

/// What this sandbox would use, and what it overrides.
pub async fn sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<SandboxMediaView, ApiError> {
    require(principal, Permission::MediaRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(ApiError::from)?;
    let global = store::settings(&mut scope).await.map_err(ApiError::from)?;
    let profiles = provider_store::list(&mut scope).await.map_err(ApiError::from)?;
    let override_settings = store::sandbox_settings(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?
        .unwrap_or_default();
    scope.commit().await.map_err(ApiError::from)?;
    Ok(SandboxMediaView {
        sandbox_id,
        effective: EffectiveMedia {
            image: effective(&global, Some(&override_settings), MediaKind::Image),
            video: effective(&global, Some(&override_settings), MediaKind::Video),
            music: effective(&global, Some(&override_settings), MediaKind::Music),
        },
        inherited: InheritedMedia {
            image: override_settings.image.is_none(),
            video: override_settings.video.is_none(),
            music: override_settings.music.is_none(),
        },
        override_settings,
        providers: provider_views(&profiles, &global),
    })
}

/// Set a sandbox's own choice.
///
/// `SandboxWrite`, not `MediaConfigure`: an operator may narrow what their
/// sandbox uses, but every target must be one the environment allows. Choosing
/// "inherit" for a kind is written as `None`, never as a copy of the default,
/// so a later change of the default reaches the sandbox.
pub async fn set_sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    settings: SandboxMediaSettings,
) -> Result<SandboxMediaView, ApiError> {
    require(principal, Permission::SandboxWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(ApiError::from)?;
    let global = store::settings(&mut scope).await.map_err(ApiError::from)?;
    let profiles = provider_store::list(&mut scope).await.map_err(ApiError::from)?;
    for (kind, target) in [
        (MediaKind::Image, settings.image.as_ref()),
        (MediaKind::Video, settings.video.as_ref()),
        (MediaKind::Music, settings.music.as_ref()),
    ] {
        if let Some(target) = target {
            let exists = profiles.iter().any(|profile| profile.id == target.provider_id);
            if let Some(reason) = refusal(&global, kind, target, exists) {
                return Err(ApiError::validation(reason));
            }
        }
    }
    store::save_sandbox_settings(&mut scope, sandbox_id, &settings, Some(principal.user_id))
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "media.sandbox",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            detail: &json!({ "image": settings.image, "video": settings.video, "music": settings.music }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    sandbox(ctx, principal, sandbox_id).await
}

/// The recent renders for a sandbox.
pub async fn renders(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    limit: i64,
) -> Result<Vec<MediaRender>, ApiError> {
    require(principal, Permission::MediaRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(ApiError::from)?;
    let rows = store::list_renders(&mut scope, sandbox_id, limit).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(rows.into_iter().map(render_view).collect())
}

fn render_view(row: store::RenderRow) -> MediaRender {
    MediaRender {
        asset_url: row.asset_name.as_ref().map(|name| format!("/api/v1/media/renders/{}/asset/{name}", row.id)),
        id: row.id,
        sandbox_id: row.sandbox_id,
        kind: row.kind,
        provider_name: row.provider_name,
        model: row.model,
        prompt: row.prompt,
        status: row.status,
        mime_type: row.mime_type,
        bytes: row.bytes,
        error: row.error,
        duration_ms: row.duration_ms,
        created_at: row.created_at,
        finished_at: row.finished_at,
    }
}

/// Where one rendered asset is, checked through the tenant scope.
pub async fn asset(
    ctx: &ServiceContext,
    principal: &Principal,
    render_id: Uuid,
    name: &str,
) -> Result<(PathBuf, String), ApiError> {
    require(principal, Permission::MediaRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(ApiError::from)?;
    let row = store::get_render(&mut scope, render_id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    let Some(row) = row else {
        return Err(ApiError::not_found(ErrorCode::NOT_FOUND, "No such render."));
    };
    let Some(asset_name) = row.asset_name else {
        return Err(ApiError::conflict(ErrorCode::CONFLICT, "This render has no asset."));
    };
    // The stored name is the only name that may be served: the path parameter
    // is compared, not concatenated, so `../` cannot escape the media root.
    if asset_name != name {
        return Err(ApiError::not_found(ErrorCode::NOT_FOUND, "No such asset."));
    }
    let directory = media_dir().join(principal.environment_id.to_string());
    Ok((directory.join(&asset_name), row.mime_type.unwrap_or_else(|| "application/octet-stream".into())))
}

/// Resolve a sandbox by name inside the caller's environment, for the MCP
/// bridge. The bridge names the sandbox; it never names an environment.
async fn sandbox_by_name(
    ctx: &ServiceContext,
    environment_id: Uuid,
    name: &str,
) -> Result<Uuid, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id).await.map_err(ApiError::from)?;
    let id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM sandboxes WHERE name = $1 AND environment_id = $2")
        .bind(name)
        .bind(environment_id)
        .fetch_optional(scope.connection())
        .await
        .map_err(|error| ApiError::from(crate::error::StoreError::Database(error)))?;
    scope.commit().await.map_err(ApiError::from)?;
    id.ok_or_else(|| ApiError::not_found(ErrorCode::NOT_FOUND, "No such sandbox in this environment."))
}

/// Render one asset for a sandbox, from the MCP bridge.
///
/// `environment_id` comes from the bridge token's own scope (the appliance has
/// one environment per token in Phase 1), never from the request body.
pub async fn render(
    ctx: &ServiceContext,
    environment_id: Uuid,
    request: &RenderRequest,
) -> Result<MediaRender, ApiError> {
    let kind = MediaKind::parse(&request.kind)
        .ok_or_else(|| ApiError::validation("kind must be image, video or music."))?;
    let prompt = request.prompt.trim();
    if prompt.is_empty() || prompt.chars().count() > MAX_PROMPT {
        return Err(ApiError::validation(format!(
            "A render prompt must be between 1 and {MAX_PROMPT} characters."
        )));
    }
    let sandbox_id = sandbox_by_name(ctx, environment_id, &request.sandbox).await?;

    let mut scope = TenantScope::begin(&ctx.pool, environment_id).await.map_err(ApiError::from)?;
    let global = store::settings(&mut scope).await.map_err(ApiError::from)?;
    let override_settings = store::sandbox_settings(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?
        .unwrap_or_default();
    let profiles = provider_store::list(&mut scope).await.map_err(ApiError::from)?;
    // The attachment is the grant: without it, an agent that learned the tool
    // name still cannot render. It is removed with the same one action that
    // removes the tool from the agent's config.
    let attached: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sandbox_mcp_servers \
         WHERE sandbox_id = $1 AND server_key = 'nestbox-media' AND enabled)",
    )
    .bind(sandbox_id)
    .fetch_one(scope.connection())
    .await
    .map_err(|error| ApiError::from(crate::error::StoreError::Database(error)))?;
    if !attached {
        return Err(ApiError::new(
            axum_status_forbidden(),
            ErrorCode::FORBIDDEN,
            "Media rendering is not attached to this sandbox.",
        ));
    }
    let target = effective(&global, Some(&override_settings), kind).ok_or_else(|| {
        ApiError::validation(format!(
            "No {} model is configured for this sandbox or its environment.",
            kind.as_str()
        ))
    })?;
    let profile = profiles
        .iter()
        .find(|profile| profile.id == target.provider_id)
        .ok_or_else(|| ApiError::validation("The selected media provider no longer exists."))?;
    if let Some(reason) = refusal(&global, kind, &target, true) {
        return Err(ApiError::new(axum_status_forbidden(), ErrorCode::FORBIDDEN, reason));
    }
    let render_id = store::insert_render(
        &mut scope,
        sandbox_id,
        kind,
        profile.id,
        &profile.name,
        &target.model,
        prompt,
        None,
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let started = std::time::Instant::now();
    let outcome = call_provider(ctx, environment_id, profile, kind, &target, prompt, request).await;
    let elapsed = started.elapsed().as_millis().min(i32::MAX as u128) as i32;
    let mut scope = TenantScope::begin(&ctx.pool, environment_id).await.map_err(ApiError::from)?;
    match outcome {
        Ok((bytes, mime)) => {
            let extension = extension_for(&mime, kind);
            let asset_name = format!("{render_id}.{extension}");
            let directory = media_dir().join(environment_id.to_string());
            if let Err(error) = tokio::fs::create_dir_all(&directory).await {
                store::finish_render(&mut scope, render_id, None, Some("The asset directory is not writable."), elapsed)
                    .await
                    .map_err(ApiError::from)?;
                scope.commit().await.map_err(ApiError::from)?;
                return Err(ApiError::internal(format!("media directory: {error}")));
            }
            if let Err(error) = tokio::fs::write(directory.join(&asset_name), &bytes).await {
                store::finish_render(&mut scope, render_id, None, Some("The asset could not be stored."), elapsed)
                    .await
                    .map_err(ApiError::from)?;
                scope.commit().await.map_err(ApiError::from)?;
                return Err(ApiError::internal(format!("media write: {error}")));
            }
            store::finish_render(
                &mut scope,
                render_id,
                Some((&asset_name, &mime, bytes.len() as i64)),
                None,
                elapsed,
            )
            .await
            .map_err(ApiError::from)?;
        }
        Err(message) => {
            store::finish_render(&mut scope, render_id, None, Some(&message), elapsed)
                .await
                .map_err(ApiError::from)?;
        }
    }
    scope.commit().await.map_err(ApiError::from)?;
    let mut scope = TenantScope::begin(&ctx.pool, environment_id).await.map_err(ApiError::from)?;
    let row = store::get_render(&mut scope, render_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found(ErrorCode::NOT_FOUND, "No such render."))?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(render_view(row))
}

/// 403 without naming a permission, for a policy refusal rather than a role
/// refusal. Kept as a function so the status code is written once.
fn axum_status_forbidden() -> axum::http::StatusCode {
    axum::http::StatusCode::FORBIDDEN
}

fn extension_for(mime: &str, kind: MediaKind) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        _ => match kind {
            MediaKind::Image => "png",
            MediaKind::Video => "mp4",
            MediaKind::Music => "mp3",
        },
    }
}

/// Ask the provider for the asset. The credential is resolved here and used for
/// one request; it is not written to the render row or to a log.
async fn call_provider(
    ctx: &ServiceContext,
    environment_id: Uuid,
    profile: &ProviderProfile,
    kind: MediaKind,
    target: &MediaTarget,
    prompt: &str,
    request: &RenderRequest,
) -> Result<(Vec<u8>, String), String> {
    let credential = match profile.secret_ref.as_deref() {
        Some(locator) => Some(secret_service::credential(ctx, environment_id, locator).await?),
        None => None,
    };
    let label_key = format!("media_path_{}", kind.as_str());
    let path = profile
        .labels
        .get(&label_key)
        .and_then(Value::as_str)
        .filter(|value| value.starts_with('/'))
        .unwrap_or_else(|| kind.default_path());
    let url = format!("{}{}", profile.endpoint.trim_end_matches('/'), path);
    let mut body = json!({ "model": target.model, "prompt": prompt, "n": 1 });
    if let Some(size) = request.size.as_deref() {
        body["size"] = json!(size);
    }
    if let Some(seconds) = request.duration_seconds {
        body["duration"] = json!(seconds.clamp(1, 60));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|error| format!("media client: {error}"))?;
    let mut call = client.post(&url).json(&body);
    if let Some(credential) = credential {
        call = call.bearer_auth(credential);
    }
    let response = call.send().await.map_err(|error| format!("provider request failed: {error}"))?;
    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("provider answered {status} with an unreadable body: {error}"))?;
    if !status.is_success() {
        let message = payload
            .get("error")
            .and_then(|error| error.get("message").or(Some(error)))
            .and_then(Value::as_str)
            .unwrap_or("the provider refused the request");
        return Err(format!("provider answered {status}: {message}"));
    }
    decode_asset(&payload, kind)
}

/// Pull the bytes out of the several shapes media APIs use.
fn decode_asset(payload: &Value, kind: MediaKind) -> Result<(Vec<u8>, String), String> {
    let default_mime = match kind {
        MediaKind::Image => "image/png",
        MediaKind::Video => "video/mp4",
        MediaKind::Music => "audio/mpeg",
    };
    let mime = payload
        .get("mime_type")
        .or_else(|| payload.get("content_type"))
        .and_then(Value::as_str)
        .unwrap_or(default_mime)
        .to_string();
    let encoded = payload
        .pointer("/data/0/b64_json")
        .or_else(|| payload.get("/b64_json"))
        .or_else(|| payload.get("/audio"))
        .or_else(|| payload.get("/video"))
        .or_else(|| payload.get("/image"))
        .and_then(Value::as_str);
    if let Some(encoded) = encoded {
        let cleaned = encoded.split(',').last().unwrap_or(encoded);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(cleaned)
            .map_err(|_| "the provider returned a value that is not base64.".to_string())?;
        return Ok((bytes, mime));
    }
    Err("the provider answered without an asset (expected b64_json or audio/video/image data).".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base64_image_is_decoded_and_named() {
        let payload = json!({ "data": [{ "b64_json": "aGVsbG8=" }] });
        let (bytes, mime) = decode_asset(&payload, MediaKind::Image).expect("decoded");
        assert_eq!(bytes, b"hello");
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn a_data_uri_prefix_is_tolerated() {
        let payload = json!({ "audio": "data:audio/wav;base64,aGVsbG8=", "mime_type": "audio/wav" });
        let (bytes, mime) = decode_asset(&payload, MediaKind::Music).expect("decoded");
        assert_eq!(bytes, b"hello");
        assert_eq!(mime, "audio/wav");
    }

    #[test]
    fn an_answer_without_an_asset_is_an_error_not_an_empty_file() {
        let error = decode_asset(&json!({ "data": [] }), MediaKind::Video).unwrap_err();
        assert!(error.contains("without an asset"));
    }

    #[test]
    fn extensions_follow_the_mime_type() {
        assert_eq!(extension_for("video/webm", MediaKind::Video), "webm");
        assert_eq!(extension_for("application/octet-stream", MediaKind::Music), "mp3");
    }
}
