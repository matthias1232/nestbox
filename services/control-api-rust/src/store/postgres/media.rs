//! Media settings per environment and per sandbox, and the render record.
//!
//! Three tables live here (`0046_media_mcp.sql`). The settings reads are one
//! row each, so there is no pagination and no list shape to keep in sync with a
//! filter; `media_renders` is the only growing table and its list is bounded by
//! the caller's limit.

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::media::{MediaKind, MediaSettings, MediaTarget, SandboxMediaSettings};
use crate::error::StoreError;

/// One render as the API reports it: the prompt and where the asset is, never
/// a provider credential.
#[derive(Debug, Clone)]
pub struct RenderRow {
    pub id: Uuid,
    pub sandbox_id: Uuid,
    pub kind: MediaKind,
    pub provider_name: Option<String>,
    pub model: String,
    pub prompt: String,
    pub status: String,
    pub asset_name: Option<String>,
    pub mime_type: Option<String>,
    pub bytes: Option<i64>,
    pub error: Option<String>,
    pub duration_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

const RENDER_COLUMNS: &str = "id, sandbox_id, kind, provider_name, model, prompt, status, asset_name, \
     mime_type, bytes, error, duration_ms, created_at, finished_at";

fn target_from(row: &sqlx::postgres::PgRow, prefix: &str) -> Result<Option<MediaTarget>, StoreError> {
    let provider: Option<Uuid> = row.try_get(format!("{prefix}_provider_id").as_str())?;
    let model: Option<String> = row.try_get(format!("{prefix}_model").as_str())?;
    Ok(match (provider, model) {
        (Some(provider_id), Some(model)) if !model.is_empty() => Some(MediaTarget { provider_id, model }),
        // The CHECK constraint makes a half pair impossible; a NULL pair is a
        // deliberate "inherit".
        _ => None,
    })
}

/// The environment default, or an all-empty default when the row does not exist
/// yet. Rendering is enabled by default so a fresh appliance does not look
/// broken before an admin saves anything; the absence of a model is what makes
/// a request fail, and it says so by name.
pub async fn settings(scope: &mut TenantScope<'_>) -> Result<MediaSettings, StoreError> {
    let row = sqlx::query(
        "SELECT enabled, image_provider_id, image_model, video_provider_id, video_model, \
                music_provider_id, music_model, allowed_provider_ids \
         FROM media_settings WHERE environment_id = $1",
    )
    .bind(scope.environment_id())
    .fetch_optional(scope.connection())
    .await?;
    let Some(row) = row else {
        return Ok(MediaSettings { enabled: true, ..Default::default() });
    };
    Ok(MediaSettings {
        enabled: row.try_get("enabled")?,
        image: target_from(&row, "image")?,
        video: target_from(&row, "video")?,
        music: target_from(&row, "music")?,
        allowed_provider_ids: row.try_get("allowed_provider_ids")?,
    })
}

/// Write the environment default. Idempotent by environment, so a second save
/// replaces rather than stacking rows.
pub async fn save_settings(
    scope: &mut TenantScope<'_>,
    settings: &MediaSettings,
    actor: Option<Uuid>,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO media_settings (environment_id, enabled, image_provider_id, image_model, \
            video_provider_id, video_model, music_provider_id, music_model, \
            allowed_provider_ids, updated_by, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now()) \
         ON CONFLICT (environment_id) DO UPDATE SET \
            enabled = EXCLUDED.enabled, \
            image_provider_id = EXCLUDED.image_provider_id, image_model = EXCLUDED.image_model, \
            video_provider_id = EXCLUDED.video_provider_id, video_model = EXCLUDED.video_model, \
            music_provider_id = EXCLUDED.music_provider_id, music_model = EXCLUDED.music_model, \
            allowed_provider_ids = EXCLUDED.allowed_provider_ids, \
            updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(scope.environment_id())
    .bind(settings.enabled)
    .bind(settings.image.as_ref().map(|item| item.provider_id))
    .bind(settings.image.as_ref().map(|item| item.model.as_str()))
    .bind(settings.video.as_ref().map(|item| item.provider_id))
    .bind(settings.video.as_ref().map(|item| item.model.as_str()))
    .bind(settings.music.as_ref().map(|item| item.provider_id))
    .bind(settings.music.as_ref().map(|item| item.model.as_str()))
    .bind(&settings.allowed_provider_ids)
    .bind(actor)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// A sandbox's own choice, or `None` when it inherits everything.
pub async fn sandbox_settings(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<Option<SandboxMediaSettings>, StoreError> {
    let row = sqlx::query(
        "SELECT image_provider_id, image_model, video_provider_id, video_model, \
                music_provider_id, music_model \
         FROM sandbox_media_settings WHERE sandbox_id = $1 AND environment_id = $2",
    )
    .bind(sandbox_id)
    .bind(scope.environment_id())
    .fetch_optional(scope.connection())
    .await?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(SandboxMediaSettings {
        image: target_from(&row, "image")?,
        video: target_from(&row, "video")?,
        music: target_from(&row, "music")?,
    }))
}

/// Replace a sandbox's choice. All three kinds are written together, so a
/// cleared kind means "inherit" rather than "keep the previous value".
pub async fn save_sandbox_settings(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    settings: &SandboxMediaSettings,
    actor: Option<Uuid>,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO sandbox_media_settings (sandbox_id, environment_id, image_provider_id, image_model, \
            video_provider_id, video_model, music_provider_id, music_model, updated_by, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now()) \
         ON CONFLICT (sandbox_id) DO UPDATE SET \
            image_provider_id = EXCLUDED.image_provider_id, image_model = EXCLUDED.image_model, \
            video_provider_id = EXCLUDED.video_provider_id, video_model = EXCLUDED.video_model, \
            music_provider_id = EXCLUDED.music_provider_id, music_model = EXCLUDED.music_model, \
            updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(sandbox_id)
    .bind(scope.environment_id())
    .bind(settings.image.as_ref().map(|item| item.provider_id))
    .bind(settings.image.as_ref().map(|item| item.model.as_str()))
    .bind(settings.video.as_ref().map(|item| item.provider_id))
    .bind(settings.video.as_ref().map(|item| item.model.as_str()))
    .bind(settings.music.as_ref().map(|item| item.provider_id))
    .bind(settings.music.as_ref().map(|item| item.model.as_str()))
    .bind(actor)
    .execute(scope.connection())
    .await?;
    Ok(())
}

/// Record a render before the provider is called, so a timeout leaves a row
/// that says `running` rather than no evidence at all.
#[allow(clippy::too_many_arguments)]
pub async fn insert_render(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    kind: MediaKind,
    provider_id: Uuid,
    provider_name: &str,
    model: &str,
    prompt: &str,
    actor: Option<Uuid>,
) -> Result<Uuid, StoreError> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO media_renders (environment_id, sandbox_id, kind, provider_id, provider_name, \
            model, prompt, status, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'running', $8) RETURNING id",
    )
    .bind(scope.environment_id())
    .bind(sandbox_id)
    .bind(kind.as_str())
    .bind(provider_id)
    .bind(provider_name)
    .bind(model)
    .bind(prompt)
    .bind(actor)
    .fetch_one(scope.connection())
    .await?;
    Ok(id)
}

/// Store the outcome. The asset name is a locator under the media directory,
/// never an absolute path.
pub async fn finish_render(
    scope: &mut TenantScope<'_>,
    id: Uuid,
    asset: Option<(&str, &str, i64)>,
    error: Option<&str>,
    duration_ms: i32,
) -> Result<(), StoreError> {
    let (asset_name, mime_type, bytes) = match asset {
        Some((name, mime, size)) => (Some(name), Some(mime), Some(size)),
        None => (None, None, None),
    };
    sqlx::query(
        "UPDATE media_renders SET status = $2, asset_name = $3, mime_type = $4, bytes = $5, \
            error = $6, duration_ms = $7, finished_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(if asset_name.is_some() { "done" } else { "error" })
    .bind(asset_name)
    .bind(mime_type)
    .bind(bytes)
    .bind(error)
    .bind(duration_ms)
    .execute(scope.connection())
    .await?;
    Ok(())
}

fn row_to_render(row: &sqlx::postgres::PgRow) -> Result<RenderRow, StoreError> {
    let kind_raw: String = row.try_get("kind")?;
    Ok(RenderRow {
        id: row.try_get("id")?,
        sandbox_id: row.try_get("sandbox_id")?,
        kind: MediaKind::parse(&kind_raw).unwrap_or(MediaKind::Image),
        provider_name: row.try_get("provider_name")?,
        model: row.try_get("model")?,
        prompt: row.try_get("prompt")?,
        status: row.try_get("status")?,
        asset_name: row.try_get("asset_name")?,
        mime_type: row.try_get("mime_type")?,
        bytes: row.try_get("bytes")?,
        error: row.try_get("error")?,
        duration_ms: row.try_get("duration_ms")?,
        created_at: row.try_get("created_at")?,
        finished_at: row.try_get("finished_at")?,
    })
}

/// The recent renders for one sandbox, newest first.
pub async fn list_renders(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
    limit: i64,
) -> Result<Vec<RenderRow>, StoreError> {
    let rows = sqlx::query(&format!(
        "SELECT {RENDER_COLUMNS} FROM media_renders \
         WHERE sandbox_id = $1 AND environment_id = $2 ORDER BY created_at DESC LIMIT $3"
    ))
    .bind(sandbox_id)
    .bind(scope.environment_id())
    .bind(limit.clamp(1, 200))
    .fetch_all(scope.connection())
    .await?;
    rows.iter().map(row_to_render).collect()
}

/// One render, for the asset handler and for an ownership check.
pub async fn get_render(scope: &mut TenantScope<'_>, id: Uuid) -> Result<Option<RenderRow>, StoreError> {
    let row = sqlx::query(&format!(
        "SELECT {RENDER_COLUMNS} FROM media_renders WHERE id = $1 AND environment_id = $2"
    ))
    .bind(id)
    .bind(scope.environment_id())
    .fetch_optional(scope.connection())
    .await?;
    row.as_ref().map(row_to_render).transpose()
}
