//! Models: hardware detection, fit classification, and install jobs.
//!
//! The classifier lives in `domain::model` because it is a pure rule. This
//! module supplies the facts it needs — a stored detection result or a fresh
//! probe — and turns an install request into a job.

use std::process::Command;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::job::JobKind;
use crate::domain::model::{
    classify, FitReport, GpuDevice, HardwareProfile, Model, ModelState,
};
use crate::error::{ApiError, ErrorCode};
use crate::store::TenantScope;
use crate::store::postgres::models as store;

/// One catalog entry with its verdict.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct ModelView {
    #[serde(flatten)]
    pub model: Model,
    pub fit: FitReport,
}

/// The catalog, classified against this machine.
pub async fn catalog(
    ctx: &ServiceContext,
    principal: &Principal,
    refresh_hardware: bool,
) -> Result<Vec<ModelView>, ApiError> {
    require(principal, Permission::ModelRead)?;

    let hardware = detect_or_load(ctx, principal, refresh_hardware).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let models = store::list_catalog(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(models
        .into_iter()
        .map(|model| ModelView {
            fit: classify(&model, &hardware),
            model,
        })
        .collect())
}

/// Only the installed models.
pub async fn installed(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<Model>, ApiError> {
    require(principal, Permission::ModelRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let models = store::list_installed(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(models)
}

/// The detected hardware, or a fresh probe.
pub async fn hardware(
    ctx: &ServiceContext,
    principal: &Principal,
    refresh: bool,
) -> Result<HardwareProfile, ApiError> {
    require(principal, Permission::ModelRead)?;
    detect_or_load(ctx, principal, refresh).await
}

/// Install a model as a job.
///
/// Refused when the machine cannot run it: the whole point of the fit report is
/// that the button is disabled with a reason, not that the install fails later.
pub async fn install(
    ctx: &ServiceContext,
    principal: &Principal,
    key: &str,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::ModelInstall)?;

    let hardware = detect_or_load(ctx, principal, false).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    let model = store::get_by_key(&mut scope, key)
        .await
        .map_err(|error| match error {
            crate::error::StoreError::NotFound => ApiError::not_found(
                ErrorCode::MODEL_NOT_FOUND,
                format!("No model with key '{key}'."),
            ),
            other => ApiError::from(other),
        })?;

    let report = classify(&model, &hardware);
    if !report.fit.allows_install() {
        scope.commit().await.map_err(ApiError::from)?;
        return Err(ApiError::conflict(
            ErrorCode::MODEL_NOT_INSTALLABLE,
            report.reason,
        ));
    }

    let job = crate::store::postgres::jobs::create(
        &mut scope,
        if model.state == ModelState::Installed {
            JobKind::ModelUpdate
        } else {
            JobKind::ModelInstall
        },
        &[model.id],
        &serde_json::json!({ "model_key": model.key, "version": model.version }),
        None,
        Some(principal.user_id),
    )
    .await
    .map_err(ApiError::from)?;

    store::set_state(&mut scope, &model.key, ModelState::Installing, None)
        .await
        .map_err(ApiError::from)?;

    scope.commit().await.map_err(ApiError::from)?;

    Ok(serde_json::json!({
        "job_id": job.id,
        "model_key": model.key,
        "fit": report.fit,
    }))
}

/// Uninstall a model as a job.
pub async fn uninstall(
    ctx: &ServiceContext,
    principal: &Principal,
    key: &str,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::ModelInstall)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let model = store::get_by_key(&mut scope, key)
        .await
        .map_err(|error| match error {
            crate::error::StoreError::NotFound => ApiError::not_found(
                ErrorCode::MODEL_NOT_FOUND,
                format!("No model with key '{key}'."),
            ),
            other => ApiError::from(other),
        })?;

    let job = crate::store::postgres::jobs::create(
        &mut scope,
        JobKind::ModelUninstall,
        &[model.id],
        &serde_json::json!({ "model_key": model.key }),
        None,
        Some(principal.user_id),
    )
    .await
    .map_err(ApiError::from)?;

    store::set_state(&mut scope, &model.key, ModelState::Uninstalling, None)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(serde_json::json!({ "job_id": job.id, "model_key": model.key }))
}

/// Load the latest detection, or probe the machine if there is none or a refresh
/// was requested.
async fn detect_or_load(
    ctx: &ServiceContext,
    principal: &Principal,
    refresh: bool,
) -> Result<HardwareProfile, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    if !refresh {
        if let Some(profile) = store::latest_hardware(&mut scope)
            .await
            .map_err(ApiError::from)?
        {
            scope.commit().await.map_err(ApiError::from)?;
            return Ok(profile);
        }
    }

    let profile = probe_hardware(principal.environment_id);
    store::insert_hardware(&mut scope, &profile)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(profile)
}

/// Ask the platform what it has.
///
/// The command is a fixed argument vector with no interpolation, so the only way
/// this can be influenced from outside is by controlling what `nvidia-smi` is on
/// `PATH` — which is the container's own filesystem, not user input.
///
/// A machine with no GPU, or a container without the driver, is not an error:
/// it is a fact, and it is reported with the reason so the console can explain
/// why every local model is unavailable.
pub fn probe_hardware(environment_id: uuid::Uuid) -> HardwareProfile {
    let hostname = std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_string());

    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,memory.total,memory.free",
            "--format=csv,noheader,nounits",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let gpus = parse_nvidia_smi(&text);
            let total: i64 = gpus.iter().map(|gpu| gpu.memory_bytes).sum();
            HardwareProfile {
                id: uuid::Uuid::new_v4(),
                environment_id,
                detected_at: chrono::Utc::now(),
                hostname,
                gpus,
                total_vram_bytes: total,
                unified_memory_bytes: None,
                cuda_version: driver_query("--query-gpu=driver_version"),
                driver_version: driver_query("--query-gpu=driver_version"),
                detection_note: None,
            }
        }
        Ok(_) => HardwareProfile {
            id: uuid::Uuid::new_v4(),
            environment_id,
            detected_at: chrono::Utc::now(),
            hostname,
            gpus: Vec::new(),
            total_vram_bytes: 0,
            unified_memory_bytes: None,
            cuda_version: None,
            driver_version: None,
            detection_note: Some(
                "nvidia-smi ran but reported no usable GPU. Local models cannot be installed."
                    .to_string(),
            ),
        },
        Err(_) => HardwareProfile {
            id: uuid::Uuid::new_v4(),
            environment_id,
            detected_at: chrono::Utc::now(),
            hostname,
            gpus: Vec::new(),
            total_vram_bytes: 0,
            unified_memory_bytes: None,
            cuda_version: None,
            driver_version: None,
            detection_note: Some(
                "No NVIDIA driver tooling in this environment. Cloud models work normally; \
                 local models need a GPU host."
                    .to_string(),
            ),
        },
    }
}

fn driver_query(field: &str) -> Option<String> {
    let output = Command::new("nvidia-smi")
        .args([field, "--format=csv,noheader"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Parse `index, name, total MiB, free MiB` lines.
fn parse_nvidia_smi(text: &str) -> Vec<GpuDevice> {
    const MIB: i64 = 1024 * 1024;
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split(',').map(str::trim);
            let index = parts.next()?.parse::<u32>().ok()?;
            let name = parts.next()?.to_string();
            let total = parts.next()?.parse::<i64>().ok()?;
            let free = parts.next().and_then(|v| v.parse::<i64>().ok());
            Some(GpuDevice {
                index,
                name,
                memory_bytes: total * MIB,
                free_bytes: free.map(|value| value * MIB),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvidia_smi_output_is_parsed() {
        let text = "0, NVIDIA H100 80GB HBM3, 81559, 81000\n1, NVIDIA H100 80GB HBM3, 81559, 0\n";
        let gpus = parse_nvidia_smi(text);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].index, 0);
        assert_eq!(gpus[0].name, "NVIDIA H100 80GB HBM3");
        assert_eq!(gpus[0].memory_bytes, 81559 * 1024 * 1024);
        assert_eq!(gpus[1].free_bytes, Some(0));
    }

    #[test]
    fn garbage_output_yields_no_gpus_rather_than_a_panic() {
        assert!(parse_nvidia_smi("no gpu here").is_empty());
        assert!(parse_nvidia_smi("").is_empty());
        assert!(parse_nvidia_smi("0, , , ").is_empty());
    }

    #[test]
    fn a_machine_without_nvidia_smi_reports_a_reason() {
        // This is the honest path on a host with no GPU tooling, which is also
        // the path the development appliance takes.
        let profile = probe_hardware(uuid::Uuid::nil());
        if profile.gpus.is_empty() {
            assert!(profile.detection_note.is_some());
        }
    }
}
