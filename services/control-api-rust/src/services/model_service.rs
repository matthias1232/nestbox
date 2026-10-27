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

/// What the caller wants to see in the catalog.
///
/// Every field is a narrowing; an empty filter returns everything. The fit
/// filter is a verdict switch, not a second classifier: it keeps or drops rows
/// by the verdict `classify` already produced.
#[derive(Debug, Clone, Default)]
pub struct CatalogQuery {
    pub family: Option<String>,
    pub publisher: Option<String>,
    pub runtime: Option<String>,
    pub cpu_only: bool,
    pub unified_only: bool,
    /// `Some(true)` keeps only what fits this machine, `Some(false)` only what
    /// does not, `None` keeps both.
    pub fits: Option<bool>,
}

impl CatalogQuery {
    fn store_filter(&self) -> store::CatalogFilter {
        store::CatalogFilter {
            family: self.family.clone(),
            publisher: self.publisher.clone(),
            runtime: self.runtime.clone(),
            cpu_only: self.cpu_only,
            unified_only: self.unified_only,
        }
    }
}

/// The catalog, classified against this machine.
pub async fn catalog(
    ctx: &ServiceContext,
    principal: &Principal,
    refresh_hardware: bool,
    query: &CatalogQuery,
) -> Result<Vec<ModelView>, ApiError> {
    require(principal, Permission::ModelRead)?;

    let hardware = detect_or_load(ctx, principal, refresh_hardware).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let models = store::list_catalog(&mut scope, &query.store_filter())
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(models
        .into_iter()
        .map(|model| ModelView {
            fit: classify(&model, &hardware),
            model,
        })
        .filter(|view| query.fits.is_none_or(|wanted| view.fit.fit.fits() == wanted))
        .collect())
}

/// One model with its verdict, for the detail view.
pub async fn requirements(
    ctx: &ServiceContext,
    principal: &Principal,
    key: &str,
) -> Result<ModelView, ApiError> {
    require(principal, Permission::ModelRead)?;
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
    scope.commit().await.map_err(ApiError::from)?;

    Ok(ModelView {
        fit: classify(&model, &hardware),
        model,
    })
}

/// Counts for the filter chips: what the catalog contains and how much of it
/// fits this machine.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct Facets {
    pub total: i64,
    pub fits: i64,
    pub families: std::collections::BTreeMap<String, i64>,
    pub publishers: std::collections::BTreeMap<String, i64>,
    pub runtimes: std::collections::BTreeMap<String, i64>,
    /// How many entries carry each verdict, so a badge count is never guessed.
    pub fits_by_verdict: std::collections::BTreeMap<String, i64>,
    pub cpu_only: i64,
    pub unified_only: i64,
}

/// The facet counts, computed from the same classification the list uses.
pub async fn facets(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Facets, ApiError> {
    require(principal, Permission::ModelRead)?;
    let hardware = detect_or_load(ctx, principal, false).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let stored = store::catalog_facets(&mut scope).await.map_err(ApiError::from)?;
    let models = store::list_catalog(&mut scope, &store::CatalogFilter::default())
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let mut facets = Facets {
        total: stored.total,
        fits: 0,
        families: stored.families,
        publishers: stored.publishers,
        runtimes: stored.runtimes,
        fits_by_verdict: std::collections::BTreeMap::new(),
        cpu_only: stored.cpu_only,
        unified_only: stored.unified_only,
    };
    for model in models {
        let report = classify(&model, &hardware);
        if report.fit.fits() {
            facets.fits += 1;
        }
        *facets
            .fits_by_verdict
            .entry(report.fit.as_str().to_string())
            .or_insert(0) += 1;
    }
    Ok(facets)
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

    // Phase 1: the local node. Package 2 of the node plan routes this through
    // the node's runner and takes the node as a parameter.
    let node = crate::store::postgres::nodes::local(&mut scope)
        .await
        .map_err(ApiError::from)?;

    if !refresh {
        if let Some(profile) = store::latest_hardware(&mut scope, node.id)
            .await
            .map_err(ApiError::from)?
        {
            scope.commit().await.map_err(ApiError::from)?;
            return Ok(profile);
        }
    }

    let profile = probe_hardware(principal.environment_id, node.id);
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
/// Three sources, each read from the kernel's own files rather than from a
/// tool that may not be installed:
///
/// * CPU model, cores, and threads from `/proc/cpuinfo`;
/// * system RAM from `/proc/meminfo` (`MemTotal`, the kernel's usable figure);
/// * GPUs from `nvidia-smi`, which is absent on a CPU-only host.
///
/// A machine without a GPU is not an error: it is a fact, and the RAM and core
/// count are what decide whether a CPU model fits. The detection note says why
/// the GPU list is empty so the console can explain it without guessing.
pub fn probe_hardware(environment_id: uuid::Uuid, node_id: uuid::Uuid) -> HardwareProfile {
    let hostname = std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_string());

    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu = parse_cpuinfo(&cpuinfo);
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let system_ram_bytes = parse_meminfo_total(&meminfo);

    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,memory.total,memory.free",
            "--format=csv,noheader,nounits",
        ])
        .output();

    let (gpus, driver_version, cuda_version, gpu_note) = match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let gpus = parse_nvidia_smi(&text);
            let cuda = query_cuda_version();
            if gpus.is_empty() {
                (
                    gpus,
                    None,
                    cuda,
                    Some(
                        "nvidia-smi ran but reported no usable GPU. GPU models cannot be \
                         installed; CPU models are gated by RAM and cores only."
                            .to_string(),
                    ),
                )
            } else {
                (gpus, driver_query("--query-gpu=driver_version"), cuda, None)
            }
        }
        Ok(_) => (
            Vec::new(),
            None,
            None,
            Some(
                "nvidia-smi ran but reported no usable GPU. GPU models cannot be installed; \
                 CPU models are gated by RAM and cores only."
                    .to_string(),
            ),
        ),
        Err(_) => (
            Vec::new(),
            None,
            None,
            Some(
                "No NVIDIA driver tooling in this environment. GPU models need a GPU host; \
                 CPU models are gated by RAM and cores only."
                    .to_string(),
            ),
        ),
    };

    let total_vram: i64 = gpus.iter().map(|gpu| gpu.memory_bytes).sum();

    // A unified-memory machine (DGX Spark / GB10) has no discrete VRAM: the
    // model and the CPU draw from one pool. That pool is deliberately *not*
    // inferred from "no discrete card but a lot of RAM", because every CPU-only
    // VM would then look like a Spark and wrongly pass GPU gates. It is only
    // reported when the hardware identifies itself as GB10/Grace.
    let unified = detect_unified_pool(cpu.model.as_deref(), &gpus, system_ram_bytes);

    let note = match (&unified.source, gpu_note) {
        (Some(source), _) => Some(source.clone()),
        (None, Some(note)) => Some(note),
        (None, None) => None,
    };

    HardwareProfile {
        id: uuid::Uuid::new_v4(),
        environment_id,
        node_id,
        detected_at: chrono::Utc::now(),
        hostname,
        gpus,
        total_vram_bytes: total_vram,
        unified_memory_bytes: unified.bytes,
        unified_memory_source: unified.source,
        cuda_version,
        driver_version,
        detection_note: note,
        cpu_model: cpu.model,
        cpu_cores: cpu.cores,
        cpu_threads: cpu.threads,
        system_ram_bytes,
    }
}

/// The CPU facts the classifier needs, as `/proc/cpuinfo` describes them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CpuFacts {
    model: Option<String>,
    /// Distinct physical cores across all sockets.
    cores: Option<i32>,
    /// Logical processors, which is what a thread pool can use.
    threads: Option<i32>,
}

/// Parse model name, physical cores, and logical processors.
///
/// `/proc/cpuinfo` is a sequence of `key\t: value` blocks separated by blank
/// lines. The physical core count needs the `(physical id, core id)` pairs; on
/// an ARM host (GB10) those keys are absent, so the logical count is the honest
/// answer there and is reported as both.
fn parse_cpuinfo(text: &str) -> CpuFacts {
    let mut model: Option<String> = None;
    let mut threads = 0i32;
    let mut pairs: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    let mut physical: Option<String> = None;
    let mut core: Option<String> = None;
    let mut cores_field: Option<i32> = None;

    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            // A blank line ends a processor block.
            if line.trim().is_empty() {
                physical = None;
                core = None;
            }
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "processor" => threads += 1,
            "model name" | "Model" if model.is_none() => model = Some(value.to_string()),
            "physical id" => physical = Some(value.to_string()),
            "core id" => core = Some(value.to_string()),
            "cpu cores" => cores_field = value.parse::<i32>().ok().or(cores_field),
            _ => {}
        }
        if let (Some(p), Some(c)) = (physical.as_ref(), core.as_ref()) {
            pairs.insert((p.clone(), c.clone()));
        }
    }

    let cores = if !pairs.is_empty() {
        Some(pairs.len() as i32)
    } else {
        cores_field.or(if threads > 0 { Some(threads) } else { None })
    };

    CpuFacts {
        model,
        cores,
        threads: (threads > 0).then_some(threads),
    }
}

/// `MemTotal` from `/proc/meminfo`, in bytes.
///
/// The value is in kB. It is what the kernel says it can give to user space,
/// which is the right figure for "will this model load" — not the installed
/// DIMM size.
fn parse_meminfo_total(text: &str) -> Option<i64> {
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("MemTotal:") else {
            continue;
        };
        let value = rest.trim().trim_end_matches(" kB").trim();
        return value.parse::<i64>().ok().map(|kb| kb * 1024);
    }
    None
}

/// The unified pool, when the hardware says it is a GB10-class machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct UnifiedPool {
    bytes: Option<i64>,
    source: Option<String>,
}

/// Detect a unified memory pool without guessing.
///
/// Only two things count as evidence: `nvidia-smi` naming a GB10 device, or
/// `/proc/cpuinfo` naming NVIDIA's Grace/GB10 part. Both are reported together
/// with the pool size, so a reader can see the number was measured from
/// `MemTotal` on a machine that is known to share it.
fn detect_unified_pool(
    cpu_model: Option<&str>,
    gpus: &[GpuDevice],
    system_ram_bytes: Option<i64>,
) -> UnifiedPool {
    let names_gb10 = |value: &str| {
        let lowered = value.to_ascii_lowercase();
        lowered.contains("gb10") || lowered.contains("grace")
    };

    let from_gpu = gpus.iter().any(|gpu| names_gb10(&gpu.name));
    let from_cpu = cpu_model.is_some_and(names_gb10);
    if !from_gpu && !from_cpu {
        return UnifiedPool::default();
    }

    let Some(ram) = system_ram_bytes else {
        return UnifiedPool {
            bytes: None,
            source: Some(
                "Unified-memory machine (GB10/Grace) recognised, but MemTotal could not be \
                 read, so the pool size is unknown rather than guessed."
                    .to_string(),
            ),
        };
    };

    let evidence = if from_gpu {
        "nvidia-smi reports a GB10 device"
    } else {
        "the CPU identifies as NVIDIA GB10/Grace"
    };
    UnifiedPool {
        bytes: Some(ram),
        source: Some(format!(
            "Unified memory: {evidence}; pool = MemTotal from /proc/meminfo ({} GiB), \
             shared by CPU and GPU.",
            ram / (1024 * 1024 * 1024)
        )),
    }
}

/// The CUDA version from the `nvidia-smi` header (`CUDA Version: 12.4`).
///
/// The header is human-formatted, so this reads the line rather than querying a
/// field that `nvidia-smi` does not offer for CUDA.
fn query_cuda_version() -> Option<String> {
    let output = Command::new("nvidia-smi").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(index) = line.find("CUDA Version:") {
            let value = line[index + "CUDA Version:".len()..]
                .trim()
                .split_whitespace()
                .next()?
                .trim_end_matches('|')
                .trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
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
        let profile = probe_hardware(uuid::Uuid::nil(), uuid::Uuid::nil());
        if profile.gpus.is_empty() {
            assert!(profile.detection_note.is_some());
        }
    }

    #[test]
    fn cpuinfo_yields_model_cores_and_threads() {
        let text = "\
processor\t: 0
model name\t: AMD EPYC 7B13 64-Core Processor
physical id\t: 0
core id\t\t: 0
cpu cores\t: 2

processor\t: 1
model name\t: AMD EPYC 7B13 64-Core Processor
physical id\t: 0
core id\t\t: 1
cpu cores\t: 2

processor\t: 2
model name\t: AMD EPYC 7B13 64-Core Processor
physical id\t: 0
core id\t\t: 0
cpu cores\t: 2
";
        let facts = parse_cpuinfo(text);
        assert_eq!(facts.model.as_deref(), Some("AMD EPYC 7B13 64-Core Processor"));
        assert_eq!(facts.cores, Some(2));
        assert_eq!(facts.threads, Some(3));
    }

    #[test]
    fn an_arm_cpuinfo_without_core_ids_falls_back_to_the_logical_count() {
        // GB10 reports neither `physical id` nor `core id`.
        let text = "\
processor\t: 0
model name\t: NVIDIA GB10
processor\t: 1
model name\t: NVIDIA GB10
";
        let facts = parse_cpuinfo(text);
        assert_eq!(facts.model.as_deref(), Some("NVIDIA GB10"));
        assert_eq!(facts.cores, Some(2));
        assert_eq!(facts.threads, Some(2));
    }

    #[test]
    fn empty_cpuinfo_yields_no_claims() {
        let facts = parse_cpuinfo("");
        assert_eq!(facts, CpuFacts::default());
    }

    #[test]
    fn memtotal_is_read_in_bytes() {
        let text = "MemTotal:       16324516 kB\nMemFree:         1234567 kB\n";
        assert_eq!(parse_meminfo_total(text), Some(16324516 * 1024));
        assert_eq!(parse_meminfo_total("MemFree: 1 kB\n"), None);
    }

    #[test]
    fn a_plain_x86_machine_is_never_treated_as_unified_memory() {
        // The dangerous mistake would be "no discrete card but plenty of RAM,
        // so it must be a Spark": that would let GPU gates pass on a CPU VM.
        let pool = detect_unified_pool(Some("AMD EPYC 7B13"), &[], Some(128 * 1024 * 1024 * 1024));
        assert_eq!(pool, UnifiedPool::default());
    }

    #[test]
    fn a_gb10_machine_reports_the_pool_with_its_source() {
        let gpus = vec![GpuDevice {
            index: 0,
            name: "NVIDIA GB10".into(),
            memory_bytes: 0,
            free_bytes: None,
        }];
        let pool = detect_unified_pool(Some("NVIDIA GB10"), &gpus, Some(128 * 1024 * 1024 * 1024));
        assert_eq!(pool.bytes, Some(128 * 1024 * 1024 * 1024));
        let source = pool.source.expect("source note");
        assert!(source.contains("MemTotal"));
    }

    #[test]
    fn a_recognised_unified_machine_without_meminfo_reports_unknown_rather_than_a_number() {
        let pool = detect_unified_pool(Some("NVIDIA GB10"), &[], None);
        assert_eq!(pool.bytes, None);
        assert!(pool.source.expect("note").contains("unknown"));
    }
}
