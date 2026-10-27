//! Models: hardware detection, fit classification, and install jobs.
//!
//! The classifier lives in `domain::model` because it is a pure rule. This
//! module supplies the facts it needs — a stored detection result or a fresh
//! probe of the node — and turns an install request into a job.
//!
//! The probe runs on the node, not here. A container sees only the devices it
//! was given, so measuring from the control plane would describe the control
//! plane's container and call it the machine. The runner ships raw text and the
//! parsers below turn it into numbers; the result is stored per node, so a node
//! that is currently unreachable can still be described by its last
//! measurement.

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::job::JobKind;
use crate::domain::model::{
    classify, FitReport, GpuDevice, HardwareProfile, Model, ModelState,
};
use crate::domain::node::{Node, NodeStatus};
use crate::domain::runtime::HardwareReport;
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::runtime::RuntimeError;
use crate::store::TenantScope;
use crate::store::postgres::models as store;
use crate::store::postgres::nodes;

/// One catalog entry with its verdict.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct ModelView {
    #[serde(flatten)]
    pub model: Model,
    pub fit: FitReport,
}

/// A node's hardware next to the node facts that qualify it.
///
/// The profile is the measurement; the node carries what is not a measurement —
/// its status after the probe and whether containers on it may be given a GPU.
/// A GPU model needs both, because a card that exists on the host is not a card
/// a container may use.
#[derive(Debug, Clone)]
pub struct NodeHardware {
    pub profile: HardwareProfile,
    pub node: Node,
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

    // Package 3 of the node plan adds the node to the catalog filter; until
    // then the catalog is classified against the local node.
    let measured = detect_or_load(ctx, principal, refresh_hardware, None).await?;

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
            fit: classify(&model, &measured.profile),
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
    let measured = detect_or_load(ctx, principal, false, None).await?;

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
        fit: classify(&model, &measured.profile),
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
    let measured = detect_or_load(ctx, principal, false, None).await?;

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
        let report = classify(&model, &measured.profile);
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

/// The hardware of one node, as last measured or freshly probed.
///
/// `node` is a node name (`local`, `gpu-box-01`) or an id; `None` means the
/// local node. The node is named because the control plane is not the machine:
/// the numbers come from that node's own runner.
pub async fn hardware(
    ctx: &ServiceContext,
    principal: &Principal,
    refresh: bool,
    node: Option<&str>,
) -> Result<NodeHardware, ApiError> {
    require(principal, Permission::NodeRead)?;
    detect_or_load(ctx, principal, refresh, node).await
}

/// Probe one node's hardware through its own runner, now.
///
/// Always measures, and needs `node.manage`: this reaches a machine that may be
/// remote, so it is an operator action rather than a read.
pub async fn probe_node(
    ctx: &ServiceContext,
    principal: &Principal,
    node_id: uuid::Uuid,
) -> Result<NodeHardware, ApiError> {
    require(principal, Permission::NodeManage)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let node = nodes::get(&mut scope, node_id)
        .await
        .map_err(|error| node_missing(error, &node_id.to_string()))?;
    scope.commit().await.map_err(ApiError::from)?;

    probe_and_store(ctx, principal.environment_id, node).await
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

    let measured = detect_or_load(ctx, principal, false, None).await?;

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

    let report = classify(&model, &measured.profile);
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

/// Load the latest measurement of a node, or probe the node if there is none or
/// a refresh was requested.
async fn detect_or_load(
    ctx: &ServiceContext,
    principal: &Principal,
    refresh: bool,
    node_ref: Option<&str>,
) -> Result<NodeHardware, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    let node = resolve_node(&mut scope, node_ref).await?;

    if !refresh {
        if let Some(profile) = store::latest_hardware(&mut scope, node.id)
            .await
            .map_err(ApiError::from)?
        {
            scope.commit().await.map_err(ApiError::from)?;
            return Ok(NodeHardware { profile, node });
        }
    }
    scope.commit().await.map_err(ApiError::from)?;

    // The measurement happens outside the transaction: a runner that never
    // answers must not hold a database transaction open for its timeout.
    probe_and_store(ctx, principal.environment_id, node).await
}

/// The node a request named, or the local one.
///
/// A name is the stable identifier in URLs (`local`, `gpu-box-01`); an id is
/// accepted because that is what a client holds after listing. A malformed
/// value is not a bad request but a name that does not exist, so both cases end
/// in `404 node.not_found`.
async fn resolve_node(
    scope: &mut TenantScope<'_>,
    node_ref: Option<&str>,
) -> Result<Node, ApiError> {
    let Some(value) = node_ref.map(str::trim).filter(|value| !value.is_empty()) else {
        return nodes::local(scope).await.map_err(|error| node_missing(error, "local"));
    };

    let found = match uuid::Uuid::parse_str(value) {
        Ok(id) => nodes::get(scope, id).await,
        Err(_) => nodes::get_by_name(scope, value).await,
    };
    found.map_err(|error| node_missing(error, value))
}

/// Map a missing node row to the published `node.not_found` code.
fn node_missing(error: StoreError, what: &str) -> ApiError {
    match error {
        StoreError::NotFound => ApiError::not_found(
            ErrorCode::NODE_NOT_FOUND,
            format!(
                "No node '{what}'. Address a node by its id or its name; 'local' always exists."
            ),
        ),
        other => ApiError::from(other),
    }
}

/// What one probe produced: the profile plus the container-GPU answer.
///
/// The flag belongs to the node rather than to the profile, because it is a
/// property of the Docker daemon, not of the cards, and it changes without the
/// hardware changing.
struct Measurement {
    profile: HardwareProfile,
    container_gpu: bool,
}

/// Measure a node and record both the numbers and that the node answered.
async fn probe_and_store(
    ctx: &ServiceContext,
    environment_id: uuid::Uuid,
    node: Node,
) -> Result<NodeHardware, ApiError> {
    match probe_hardware(ctx, environment_id, &node).await {
        Ok(measured) => store_probe(ctx, node, measured).await,
        Err(error) => node_offline(ctx, environment_id, node, error).await,
    }
}

/// Persist a measurement and mark the node online with what it reported.
async fn store_probe(
    ctx: &ServiceContext,
    node: Node,
    measured: Measurement,
) -> Result<NodeHardware, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, measured.profile.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::insert_hardware(&mut scope, &measured.profile)
        .await
        .map_err(ApiError::from)?;
    nodes::mark_seen(&mut scope, node.id, NodeStatus::Online, measured.container_gpu)
        .await
        .map_err(ApiError::from)?;
    let updated = nodes::get(&mut scope, node.id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(NodeHardware {
        profile: measured.profile,
        node: updated,
    })
}

/// Record that a node did not answer, and fall back to what was known.
///
/// Nothing is invented. No profile is written, the node is marked offline when
/// its runner was unreachable and unknown when the runner answered with an
/// error, and the previous measurement — if there is one — is returned
/// unchanged: the last thing that was measured is still the best knowledge, and
/// a fresh `detected_at` would make it look newer than it is. Without an earlier
/// measurement there is nothing honest to return, so the answer is `503` and the
/// runner's own message goes to the log rather than to the client.
async fn node_offline(
    ctx: &ServiceContext,
    environment_id: uuid::Uuid,
    node: Node,
    error: RuntimeError,
) -> Result<NodeHardware, ApiError> {
    let status = match &error {
        RuntimeError::Unavailable(_) => NodeStatus::Offline,
        RuntimeError::NotFound(_) | RuntimeError::Rejected(_) => NodeStatus::Unknown,
    };

    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    nodes::mark_seen(&mut scope, node.id, status, node.container_gpu)
        .await
        .map_err(ApiError::from)?;
    let previous = store::latest_hardware(&mut scope, node.id)
        .await
        .map_err(ApiError::from)?;
    let updated = nodes::get(&mut scope, node.id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let Some(profile) = previous else {
        return Err(ApiError::unavailable(
            ErrorCode::NODE_RUNTIME_UNAVAILABLE,
            format!(
                "The runner of node '{}' did not answer and the node has never been measured.",
                node.name
            ),
        )
        .with_internal(error.to_string()));
    };

    Ok(NodeHardware {
        profile,
        node: updated,
    })
}

/// Ask a node's runner what the node has.
///
/// The runner ships files, not conclusions: `/proc/cpuinfo`, `/proc/meminfo`,
/// and the two `nvidia-smi` outputs. It is asked rather than measured here
/// because the control plane may run in a container that sees none of the
/// node's devices, and a container that reports its own limits as the machine's
/// would be wrong in the direction that matters — it would say "no GPU" about a
/// GPU box.
///
/// Phase 1 has one adapter, so every node is answered by the runner the control
/// plane is configured with. A remote node needs its own adapter before its
/// numbers can honestly come from anywhere else; until then it must not be
/// probed, which is why no route creates one.
async fn probe_hardware(
    ctx: &ServiceContext,
    environment_id: uuid::Uuid,
    node: &Node,
) -> Result<Measurement, RuntimeError> {
    let report = ctx.runtime.hardware().await?;
    Ok(Measurement {
        profile: profile_from_report(environment_id, node.id, &report),
        container_gpu: report.container_gpu,
    })
}

/// Turn a runner's raw report into a profile.
///
/// Parsing lives here so the numbers are unit-tested Rust rather than a second
/// implementation in Python; the runner stays a transport. A machine without a
/// GPU is not an error: it is a fact, and the RAM and core count are what decide
/// whether a CPU model fits. The detection note explains why the GPU list is
/// what it is, so the console never has to guess.
fn profile_from_report(
    environment_id: uuid::Uuid,
    node_id: uuid::Uuid,
    report: &HardwareReport,
) -> HardwareProfile {
    let cpu = parse_cpuinfo(&report.cpuinfo);
    let system_ram_bytes = parse_meminfo_total(&report.meminfo);
    let gpus = report
        .nvidia_smi_csv
        .as_deref()
        .map(parse_nvidia_smi)
        .unwrap_or_default();
    let (driver_version, cuda_version) =
        parse_nvidia_smi_header(report.nvidia_smi_header.as_deref().unwrap_or_default());

    // A unified-memory machine (DGX Spark / GB10) has no discrete VRAM: the
    // model and the CPU draw from one pool. That pool is deliberately *not*
    // inferred from "no discrete card but a lot of RAM", because every CPU-only
    // VM would then look like a Spark and wrongly pass GPU gates. It is only
    // reported when the hardware identifies itself as GB10/Grace.
    let unified = detect_unified_pool(cpu.model.as_deref(), &gpus, system_ram_bytes);

    let notes = match unified.source.clone() {
        Some(source) => vec![source],
        None => gpu_notes(report, &gpus),
    };
    let note = (!notes.is_empty()).then(|| notes.join(" "));
    let total_vram: i64 = gpus.iter().map(|gpu| gpu.memory_bytes).sum();

    HardwareProfile {
        id: uuid::Uuid::new_v4(),
        environment_id,
        node_id,
        detected_at: chrono::Utc::now(),
        hostname: report.hostname.clone(),
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

/// Why the GPU list is what it is, and what that means for containers.
///
/// The runner's own sentence wins over ours where it has one: it stands next to
/// the node and may name the actual reason. What is never done is reporting a
/// card the node has as capacity a container may use — the Docker daemon needs
/// an NVIDIA runtime for that, and without it a GPU model is not installable on
/// that node however many cards the host shows.
fn gpu_notes(report: &HardwareReport, gpus: &[GpuDevice]) -> Vec<String> {
    let runner_note = report
        .note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());

    if report.nvidia_smi_csv.is_none() {
        return vec![runner_note.map(str::to_string).unwrap_or_else(|| {
            "The runner found no nvidia-smi on this node. GPU models need a GPU host; CPU models \
             are gated by RAM and cores only."
                .to_string()
        })];
    }

    let mut notes = Vec::new();
    if gpus.is_empty() {
        notes.push(
            "nvidia-smi ran on the node but reported no usable GPU. GPU models cannot be \
             installed here; CPU models are gated by RAM and cores only."
                .to_string(),
        );
    } else if !report.container_gpu {
        notes.push(
            "The node has NVIDIA devices, but its container runtime has no NVIDIA support, so a \
             container cannot be given one. GPU models are not installable on this node."
                .to_string(),
        );
    }
    if let Some(note) = runner_note {
        notes.push(format!("Runner: {note}"));
    }
    notes
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

/// The driver and CUDA versions from the plain `nvidia-smi` header.
///
/// Both are printed in the same table line:
/// `| NVIDIA-SMI 535.104.05   Driver Version: 535.104.05   CUDA Version: 12.2 |`.
/// Neither is a query field that `nvidia-smi` offers per device, so the line is
/// read. A field that is not there stays `None` rather than becoming a guess.
fn parse_nvidia_smi_header(text: &str) -> (Option<String>, Option<String>) {
    fn field(text: &str, name: &str) -> Option<String> {
        for line in text.lines() {
            let Some(index) = line.find(name) else {
                continue;
            };
            let value = line[index + name.len()..]
                .trim()
                .split_whitespace()
                .next()?
                .trim_end_matches('|')
                .trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
        None
    }
    (
        field(text, "Driver Version:"),
        field(text, "CUDA Version:"),
    )
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
    use crate::runtime::RuntimeAdapter;

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

    /// A report from a plain x86 node with no NVIDIA tooling, as the runner
    /// sends it: raw files, one sentence about the missing tool, no GPU flag.
    fn report_without_nvidia_tooling() -> HardwareReport {
        HardwareReport {
            hostname: Some("appliance".into()),
            cpuinfo: "\
processor\t: 0
model name\t: Intel(R) Core(TM) i7-2600 CPU @ 3.40GHz
physical id\t: 0
core id\t\t: 0

processor\t: 1
model name\t: Intel(R) Core(TM) i7-2600 CPU @ 3.40GHz
physical id\t: 0
core id\t\t: 0
"
            .into(),
            meminfo: "MemTotal:        8176000 kB\n".into(),
            nvidia_smi_csv: None,
            nvidia_smi_header: None,
            container_gpu: false,
            note: Some(
                "nvidia-smi is not available on this node; GPU models need a GPU host.".into(),
            ),
        }
    }

    #[tokio::test]
    async fn the_fake_runners_report_parses_to_the_machines_numbers() {
        // The fake adapter is what every service test runs against, so its
        // report has to mean the same thing a real node's report means.
        let report = crate::runtime::fake::FakeRuntime::new()
            .hardware()
            .await
            .expect("the fake adapter always answers");

        assert!(
            !report.container_gpu,
            "the fake node cannot give a container a GPU, so no GPU model may be \
             called installable there"
        );

        let profile = profile_from_report(uuid::Uuid::nil(), uuid::Uuid::nil(), &report);

        assert_eq!(profile.cpu_model.as_deref(), Some("Fake CPU"));
        assert_eq!(profile.cpu_cores, Some(2));
        assert_eq!(profile.cpu_threads, Some(2));
        assert_eq!(profile.system_ram_bytes, Some(8 * 1024 * 1024 * 1024));
        assert_eq!(profile.gpu_count(), 0);
        assert_eq!(profile.total_vram_bytes, 0);
        assert_eq!(profile.node_id, uuid::Uuid::nil());
        assert_eq!(
            profile.detection_note.as_deref(),
            Some("fake runtime: no GPU"),
            "the runner's own sentence is what an operator reads"
        );
    }

    #[test]
    fn a_node_without_nvidia_tooling_yields_no_gpu_and_the_runners_reason() {
        let profile = profile_from_report(
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            &report_without_nvidia_tooling(),
        );

        assert!(profile.gpus.is_empty());
        assert_eq!(profile.cpu_cores, Some(1));
        assert!(!profile.is_unified(), "a plain x86 node is not a Spark");
        assert_eq!(
            profile.detection_note.as_deref(),
            Some("nvidia-smi is not available on this node; GPU models need a GPU host.")
        );
    }

    #[test]
    fn a_gpu_a_container_cannot_reach_is_reported_as_such() {
        let report = HardwareReport {
            nvidia_smi_csv: Some("0, NVIDIA H100 80GB HBM3, 81559, 81000\n".into()),
            nvidia_smi_header: Some(
                "| NVIDIA-SMI 535.104.05   Driver Version: 535.104.05   CUDA Version: 12.2 |"
                    .into(),
            ),
            container_gpu: false,
            ..HardwareReport::default()
        };

        let profile = profile_from_report(uuid::Uuid::nil(), uuid::Uuid::nil(), &report);

        assert_eq!(profile.gpu_count(), 1);
        assert_eq!(profile.total_vram_bytes, 81559 * 1024 * 1024);
        assert_eq!(profile.driver_version.as_deref(), Some("535.104.05"));
        assert_eq!(profile.cuda_version.as_deref(), Some("12.2"));
        let note = profile.detection_note.expect("a reason for the refusal");
        assert!(
            note.contains("container runtime"),
            "the note must say a container cannot use the card, not that it is missing: {note}"
        );
    }

    #[test]
    fn a_node_with_a_gpu_container_runtime_reports_no_complaint() {
        let report = HardwareReport {
            nvidia_smi_csv: Some("0, NVIDIA H100 80GB HBM3, 81559, 81000\n".into()),
            container_gpu: true,
            ..HardwareReport::default()
        };

        let profile = profile_from_report(uuid::Uuid::nil(), uuid::Uuid::nil(), &report);
        assert_eq!(profile.gpu_count(), 1);
        assert_eq!(profile.detection_note, None);
    }

    #[test]
    fn the_nvidia_smi_header_yields_driver_and_cuda_versions() {
        let header = "\
+-----------------------------------------------------------------------------+
| NVIDIA-SMI 535.104.05   Driver Version: 535.104.05   CUDA Version: 12.2     |
|-------------------------------+----------------------+----------------------+
";
        assert_eq!(
            parse_nvidia_smi_header(header),
            (Some("535.104.05".into()), Some("12.2".into()))
        );
        assert_eq!(parse_nvidia_smi_header(""), (None, None));
        assert_eq!(
            parse_nvidia_smi_header("no versions in here"),
            (None, None)
        );
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
