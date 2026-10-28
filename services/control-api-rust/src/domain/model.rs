//! The model catalog, hardware detection, and the fit classification.
//!
//! The classifier is the feature: an operator must be able to see, before
//! clicking install, which models this machine can run and why not when it
//! cannot (ADR 0013).
//!
//! The classification, the shortfall, and the resource bars all come from here,
//! so the badge in the console and the gate in the API cannot disagree. The
//! client renders the verdict; it does not compute it.
//!
//! Three kinds of machine are modelled, because they gate differently:
//!
//! * discrete GPU(s): a model must fit one card (or `min_gpu_count` cards);
//! * unified memory (DGX Spark / GB10): one pool that is both RAM and VRAM;
//! * no GPU: only a `cpu` or `hybrid` model fits, gated by RAM and cores.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::model_install::FreeBudget;

/// A GPU as the platform probe found it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GpuDevice {
    pub index: u32,
    pub name: String,
    /// Total memory in bytes.
    pub memory_bytes: i64,
    /// Free memory at detection time, if the driver reports it.
    pub free_bytes: Option<i64>,
}

/// What this machine has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HardwareProfile {
    pub id: Uuid,
    pub environment_id: Uuid,
    /// The node this measurement describes.
    pub node_id: Uuid,
    pub detected_at: DateTime<Utc>,
    pub hostname: Option<String>,
    pub gpus: Vec<GpuDevice>,
    pub total_vram_bytes: i64,
    /// A GB10-class machine reports unified memory rather than per-card VRAM.
    /// Storing it separately is what lets the classifier treat it correctly
    /// instead of adding it to VRAM and double-counting.
    pub unified_memory_bytes: Option<i64>,
    /// Where the unified figure came from, so an inferred number is never
    /// mistaken for a measured one.
    #[serde(default)]
    pub unified_memory_source: Option<String>,
    pub cuda_version: Option<String>,
    pub driver_version: Option<String>,
    /// Why detection produced what it did: no driver, no devices, permission.
    pub detection_note: Option<String>,
    #[serde(default)]
    pub cpu_model: Option<String>,
    #[serde(default)]
    pub cpu_cores: Option<i32>,
    #[serde(default)]
    pub cpu_threads: Option<i32>,
    #[serde(default)]
    pub system_ram_bytes: Option<i64>,
}

impl HardwareProfile {
    /// The memory a single model can address on the GPU side.
    ///
    /// On a discrete-GPU machine this is the largest single card, because a
    /// model that does not fit one card needs tensor parallelism, which the
    /// catalog expresses separately via `min_gpu_count`. On a unified-memory
    /// machine it is the unified pool.
    pub fn addressable_memory_bytes(&self) -> i64 {
        if let Some(unified) = self.unified_memory_bytes {
            return unified;
        }
        self.gpus.iter().map(|g| g.memory_bytes).max().unwrap_or(0)
    }

    /// The memory a CPU-side model can address.
    ///
    /// On a unified machine the pool is the RAM; otherwise it is system RAM.
    pub fn addressable_ram_bytes(&self) -> i64 {
        self.unified_memory_bytes
            .or(self.system_ram_bytes)
            .unwrap_or(0)
    }

    /// The number of addressable GPU devices.
    ///
    /// A unified-memory machine reports no discrete cards but has one pool, so
    /// it counts as a single device. Reporting zero would make every model look
    /// like a multi-GPU request on exactly the hardware the product targets.
    pub fn gpu_count(&self) -> i32 {
        if !self.gpus.is_empty() {
            self.gpus.len() as i32
        } else if self.unified_memory_bytes.unwrap_or(0) > 0 {
            1
        } else {
            0
        }
    }

    /// Whether the machine can host GPU models at all.
    pub fn has_gpu(&self) -> bool {
        !self.gpus.is_empty() || self.unified_memory_bytes.unwrap_or(0) > 0
    }

    /// Whether this is a unified-memory machine.
    pub fn is_unified(&self) -> bool {
        self.unified_memory_bytes.unwrap_or(0) > 0
    }

    /// Cores available for a CPU-side model; zero when unknown.
    pub fn cores(&self) -> i32 {
        self.cpu_cores.unwrap_or(0)
    }
}

/// Where a model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelRuntime {
    /// Needs a GPU.
    Cuda,
    /// Runs on the CPU alone.
    Cpu,
    /// Prefers a GPU, falls back to the CPU with the RAM gate applied.
    Hybrid,
}

impl ModelRuntime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Cpu => "cpu",
            Self::Hybrid => "hybrid",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "cuda" => Self::Cuda,
            "cpu" => Self::Cpu,
            "hybrid" => Self::Hybrid,
            _ => return None,
        })
    }
}

/// A catalog entry.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Model {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub family: String,
    pub parameters: String,
    pub context_length: Option<i64>,
    pub quantization: Option<String>,
    pub weights_bytes: Option<i64>,
    /// The GPU gate: what one card (or the unified pool) must have.
    pub min_vram_bytes: Option<i64>,
    pub recommended_vram_bytes: Option<i64>,
    pub min_gpu_count: i32,
    /// The CPU-side gate.
    pub min_ram_bytes: Option<i64>,
    pub recommended_ram_bytes: Option<i64>,
    pub min_cpu_cores: Option<i32>,
    pub recommended_cpu_cores: Option<i32>,
    pub runtime: ModelRuntime,
    pub arch: Option<String>,
    pub artifact_format: Option<String>,
    /// Where the weights come from, as `huggingface:<repo>/<file>`.
    ///
    /// A model without this has no artefact the platform knows how to fetch, so
    /// it cannot be installed as a local server; the install says that instead
    /// of reserving memory for nothing.
    pub artifact_ref: Option<String>,
    /// The runtime in `model_server_images` that serves this model, if any.
    pub serve_runtime: Option<String>,
    pub publisher: Option<String>,
    pub license: Option<String>,
    pub description: Option<String>,
    /// Weights exceed any discrete card but fit a unified pool.
    pub unified_memory_only: bool,
    pub is_cpu_only: bool,
    pub source_uri: Option<String>,
    pub version: String,
    pub installed_version: Option<String>,
    pub state: ModelState,
    pub installed_at: Option<DateTime<Utc>>,
    /// True for an entry served by a cloud provider rather than installed here.
    pub is_remote: bool,
    pub provider_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    NotInstalled,
    Installing,
    Installed,
    Updating,
    Uninstalling,
    Failed,
    /// Served by a cloud provider; not present on this machine.
    Remote,
}

impl ModelState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::Installing => "installing",
            Self::Installed => "installed",
            Self::Updating => "updating",
            Self::Uninstalling => "uninstalling",
            Self::Failed => "failed",
            Self::Remote => "remote",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "not_installed" => Self::NotInstalled,
            "installing" => Self::Installing,
            "installed" => Self::Installed,
            "updating" => Self::Updating,
            "uninstalling" => Self::Uninstalling,
            "failed" => Self::Failed,
            "remote" => Self::Remote,
            _ => return None,
        })
    }
}

/// The verdict on whether this machine can run a model.
///
/// A closed set, so the console has a finite list of badges to render and cannot
/// invent a state the server does not produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Fits on the GPU side.
    Installable,
    /// Fits on the CPU side (RAM and cores), with no GPU involved.
    CpuInstallable,
    AlreadyInstalled,
    UpdateAvailable,
    /// Enough GPUs but not enough memory on one.
    InsufficientVram,
    /// Enough memory per card but too few cards.
    InsufficientGpus,
    /// Not enough system RAM for the CPU path.
    InsufficientRam,
    /// Not enough cores for the CPU path.
    InsufficientCpu,
    /// Only fits a unified-memory pool, and this machine has none.
    NeedsUnifiedMemory,
    /// No GPU at all, and the model needs one.
    NoGpu,
    /// Served by a provider; nothing to install.
    Remote,
    /// The catalogue does not state what the model requires, so no machine can
    /// be compared against it. Never "does not fit" and never installable: the
    /// honest answer is that the requirement is not published (ADR 0019).
    Unknown,
}

impl Fit {
    /// Whether the install button should be enabled.
    pub fn allows_install(self) -> bool {
        matches!(
            self,
            Self::Installable | Self::CpuInstallable | Self::UpdateAvailable
        )
    }

    /// Whether this verdict belongs in the console's "fits this machine" filter.
    pub fn fits(self) -> bool {
        matches!(
            self,
            Self::Installable | Self::CpuInstallable | Self::AlreadyInstalled | Self::UpdateAvailable
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installable => "installable",
            Self::CpuInstallable => "cpu_installable",
            Self::AlreadyInstalled => "already_installed",
            Self::UpdateAvailable => "update_available",
            Self::InsufficientVram => "insufficient_vram",
            Self::InsufficientGpus => "insufficient_gpus",
            Self::InsufficientRam => "insufficient_ram",
            Self::InsufficientCpu => "insufficient_cpu",
            Self::NeedsUnifiedMemory => "needs_unified_memory",
            Self::NoGpu => "no_gpu",
            Self::Remote => "remote",
            Self::Unknown => "unknown",
        }
    }
}

/// Which resource a bar describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Vram,
    UnifiedMemory,
    Ram,
    CpuCores,
    Gpus,
}

/// One "have versus need" comparison, computed here so the console draws what
/// the gate decided.
///
/// `recommended` may be absent; `minimum` is the gate. `available` is what the
/// machine has. All in the resource's natural unit (bytes or a count).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ResourceBar {
    pub resource: Resource,
    pub available: i64,
    pub minimum: Option<i64>,
    pub recommended: Option<i64>,
    /// `ok` when available >= recommended (or >= minimum with no
    /// recommendation), `tight` when between minimum and recommended, `short`
    /// when below minimum, `unknown` when the machine's figure is missing.
    pub status: BarStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BarStatus {
    Ok,
    Tight,
    Short,
    Unknown,
    /// This resource does not gate this model.
    NotRequired,
}

/// The classification plus the numbers that produced it.
///
/// The shortfall is returned rather than recomputed by the client, so the
/// explanation matches the decision. "Needs 80 GiB, has 24 GiB" is an answer;
/// a red badge alone is not.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FitReport {
    pub fit: Fit,
    pub required_vram_bytes: Option<i64>,
    pub required_gpus: i32,
    pub available_vram_bytes: i64,
    pub available_gpus: i32,
    /// Bytes short, when memory is the blocker.
    pub vram_shortfall_bytes: Option<i64>,
    pub required_ram_bytes: Option<i64>,
    pub available_ram_bytes: i64,
    pub ram_shortfall_bytes: Option<i64>,
    pub required_cpu_cores: Option<i32>,
    pub available_cpu_cores: i32,
    /// Whether the verdict was reached on the CPU path.
    pub cpu_path: bool,
    /// The bars the console draws.
    pub bars: Vec<ResourceBar>,
    /// A sentence the console can show verbatim.
    pub reason: String,
}

/// Classify a model against a machine.
///
/// Order matters, and the order is the rule:
///
/// 1. remote → nothing to check;
/// 2. installed → never "uninstallable" on the machine where it runs;
/// 3. unified-only without a unified pool → cannot be rescued by RAM or VRAM;
/// 4. `cpu` → RAM and cores only;
/// 5. `hybrid` → the GPU gate if there is a GPU, else the CPU gate;
/// 6. `cuda` → the GPU gate.
///
/// `free` is what the node has left after its installs reserved their share.
/// The gate runs against that, not against the last measurement, so a second
/// model that would fit an empty node is reported as insufficient here.
pub fn classify(model: &Model, hardware: &HardwareProfile, free: &FreeBudget) -> FitReport {
    if model.is_remote || model.state == ModelState::Remote {
        // Nothing is installed here, so no resource gates anything. The bars
        // stay in the response, marked `not_required`, so the table has the
        // same shape on every row.
        return report(model, hardware, free, Fit::Remote,
            "Served by a cloud provider; nothing to install on this machine.".into(),
            false, false);
    }

    if model.state == ModelState::Installed {
        let update = model
            .installed_version
            .as_deref()
            .is_some_and(|installed| installed != model.version);
        return report(
            model,
            hardware,
            free,
            if update { Fit::UpdateAvailable } else { Fit::AlreadyInstalled },
            if update {
                format!(
                    "Installed version {} can be updated to {}.",
                    model.installed_version.as_deref().unwrap_or("unknown"),
                    model.version
                )
            } else {
                format!("Installed and up to date ({}).", model.version)
            },
            false,
            false,
        );
    }

    // A requirement the catalogue does not state is not a requirement of zero.
    // Synced rows can carry null gates for a repository that publishes no size
    // (ADR 0019); without this check the gate arithmetic below would compare the
    // machine against zero and call the model installable everywhere.
    if let Some(missing) = missing_requirement(model, hardware) {
        return report(
            model,
            hardware,
            free,
            Fit::Unknown,
            format!(
                "{} does not publish its {missing} requirement, so it cannot be compared \
                 with this machine. Install stays disabled until the catalogue states it.",
                model.name
            ),
            chosen_cpu_path(model, hardware),
            false,
        );
    }

    if model.unified_memory_only && !hardware.is_unified() {
        return report(
            model,
            hardware,
            free,
            Fit::NeedsUnifiedMemory,
            format!(
                "{} needs a unified-memory machine (DGX Spark / GB10) with about {}; \
                 its weights exceed any single discrete card.",
                model.name,
                human_gib(model.min_vram_bytes.unwrap_or(0))
            ),
            false,
            true,
        );
    }

    let use_cpu_path = chosen_cpu_path(model, hardware);
    if use_cpu_path {
        return classify_cpu(model, hardware, free, use_cpu_path);
    }

    classify_gpu(model, hardware, free, use_cpu_path)
}

/// The requirement the chosen path needs and the catalogue does not state.
///
/// The minimum is the gate; a `recommended` figure without a minimum never
/// decided anything. Asking the chosen path is what keeps a GPU model with a
/// VRAM figure from being called unknown on a CPU-only machine that would use
/// its RAM gate, and the reverse.
fn missing_requirement(model: &Model, hardware: &HardwareProfile) -> Option<&'static str> {
    if chosen_cpu_path(model, hardware) {
        model.min_ram_bytes.is_none().then_some("RAM")
    } else {
        model.min_vram_bytes.is_none().then_some("GPU memory")
    }
}

/// Which path the model takes on this machine: the rule the bars follow too.
fn chosen_cpu_path(model: &Model, hardware: &HardwareProfile) -> bool {
    match model.runtime {
        ModelRuntime::Cpu => true,
        ModelRuntime::Hybrid => !hardware.has_gpu(),
        ModelRuntime::Cuda => false,
    }
}

/// Assemble a report with the bars for the path that was taken.
fn report(
    model: &Model,
    hardware: &HardwareProfile,
    free: &FreeBudget,
    fit: Fit,
    reason: String,
    cpu_path: bool,
    gated: bool,
) -> FitReport {
    FitReport {
        fit,
        required_vram_bytes: model.min_vram_bytes,
        required_gpus: model.min_gpu_count,
        available_vram_bytes: free.vram_free_bytes,
        available_gpus: hardware.gpu_count(),
        vram_shortfall_bytes: None,
        required_ram_bytes: model.min_ram_bytes,
        available_ram_bytes: free.ram_free_bytes,
        ram_shortfall_bytes: None,
        required_cpu_cores: model.min_cpu_cores,
        available_cpu_cores: hardware.cores(),
        cpu_path,
        bars: bars_for(model, hardware, free, cpu_path, gated),
        reason,
    }
}

fn classify_cpu(
    model: &Model,
    hardware: &HardwareProfile,
    free: &FreeBudget,
    cpu_path: bool,
) -> FitReport {
    let available_ram = free.ram_free_bytes;
    let required_ram = model.min_ram_bytes.unwrap_or(0);

    if available_ram == 0 && free.ram_total_bytes == 0 {
        return report(
            model,
            hardware,
            free,
            Fit::InsufficientRam,
            format!(
                "{} runs on the CPU and needs about {} of RAM, but this machine's RAM \
                 was not detected. Probe the hardware again.",
                model.name,
                human_gib(required_ram)
            ),
            cpu_path,
            true,
        );
    }

    if available_ram < required_ram {
        let shortfall = required_ram - available_ram;
        let reason = if free.ram_reserved_bytes > 0 {
            format!(
                "{} needs about {} of RAM on the CPU; {} of this node's {} are reserved by {}. \
                 {} are free, short by {}.",
                model.name,
                human_gib(required_ram),
                human_gib(free.ram_reserved_bytes),
                human_gib(free.ram_total_bytes),
                free.ram_holders(),
                human_gib(available_ram),
                human_gib(shortfall)
            )
        } else {
            format!(
                "{} needs about {} of RAM on the CPU; this machine has {}. Short by {}.",
                model.name,
                human_gib(required_ram),
                human_gib(available_ram),
                human_gib(shortfall)
            )
        };
        let mut fit = report(
            model,
            hardware,
            free,
            Fit::InsufficientRam,
            reason,
            cpu_path,
            true,
        );
        fit.ram_shortfall_bytes = Some(shortfall);
        return fit;
    }

    if let Some(required_cores) = model.min_cpu_cores {
        let cores = hardware.cores();
        if cores > 0 && cores < required_cores {
            return report(
                model,
                hardware,
                free,
                Fit::InsufficientCpu,
                format!(
                    "{} wants at least {} CPU cores; this machine has {}.",
                    model.name, required_cores, cores
                ),
                cpu_path,
                true,
            );
        }
    }

    let recommended = model.recommended_ram_bytes.unwrap_or(required_ram);
    let tight = available_ram < recommended;
    report(
        model,
        hardware,
        free,
        Fit::CpuInstallable,
        if tight {
            format!(
                "Runs on the CPU: needs {} of RAM, {} recommended, this machine has {}. \
                 Expect it to be slow.",
                human_gib(required_ram),
                human_gib(recommended),
                human_gib(available_ram)
            )
        } else {
            format!(
                "Runs on the CPU without a GPU: needs {} of RAM, this machine has {}.",
                human_gib(required_ram),
                human_gib(available_ram)
            )
        },
        cpu_path,
        true,
    )
}

fn classify_gpu(
    model: &Model,
    hardware: &HardwareProfile,
    free: &FreeBudget,
    cpu_path: bool,
) -> FitReport {
    let available_vram = free.vram_free_bytes;
    let available_gpus = hardware.gpu_count();
    let required_vram = model.min_vram_bytes.unwrap_or(0);

    if !hardware.has_gpu() {
        let mut fit = report(
            model,
            hardware,
            free,
            Fit::NoGpu,
            format!(
                "No GPU detected. {} needs about {} of GPU memory.",
                model.name,
                human_gib(required_vram)
            ),
            cpu_path,
            true,
        );
        fit.available_vram_bytes = 0;
        fit.available_gpus = 0;
        return fit;
    }

    if available_gpus < model.min_gpu_count {
        return report(
            model,
            hardware,
            free,
            Fit::InsufficientGpus,
            format!(
                "{} needs {} GPUs; this machine has {}.",
                model.name, model.min_gpu_count, available_gpus
            ),
            cpu_path,
            true,
        );
    }

    if available_vram < required_vram {
        let shortfall = required_vram - available_vram;
        let reason = if free.vram_reserved_bytes > 0 {
            format!(
                "{} needs about {} of GPU memory; {} of this node's {} are reserved by {}. \
                 {} are free, short by {}.",
                model.name,
                human_gib(required_vram),
                human_gib(free.vram_reserved_bytes),
                human_gib(free.vram_total_bytes),
                free.vram_holders(),
                human_gib(available_vram),
                human_gib(shortfall)
            )
        } else {
            format!(
                "{} needs about {}; this machine has {}. Short by {}.",
                model.name,
                human_gib(required_vram),
                human_gib(available_vram),
                human_gib(shortfall)
            )
        };
        let mut fit = report(
            model,
            hardware,
            free,
            Fit::InsufficientVram,
            reason,
            cpu_path,
            true,
        );
        fit.vram_shortfall_bytes = Some(shortfall);
        return fit;
    }

    let recommended = model.recommended_vram_bytes.unwrap_or(required_vram);
    let pool = if hardware.is_unified() { "unified memory" } else { "GPU memory" };
    report(
        model,
        hardware,
        free,
        Fit::Installable,
        if available_vram < recommended {
            format!(
                "Fits: needs {} of {}, {} recommended, this machine has {}.",
                human_gib(required_vram),
                pool,
                human_gib(recommended),
                human_gib(available_vram)
            )
        } else {
            format!(
                "Fits: needs about {} of {}, this machine has {}.",
                human_gib(required_vram),
                pool,
                human_gib(available_vram)
            )
        },
        cpu_path,
        true,
    )
}

/// The "have versus need" bars for a model on a machine.
///
/// Every model gets the same set of bars so the table is scannable: a bar that
/// does not apply is marked `NotRequired` rather than omitted. Only the path
/// that decides the verdict is `required` — a hybrid model running on the CPU
/// must not show a red GPU bar next to a green "runs on CPU" badge. The unified
/// bar replaces the VRAM bar on a unified machine, because showing both would
/// double-count the same pool.
pub fn resource_bars(
    model: &Model,
    hardware: &HardwareProfile,
    free: &FreeBudget,
) -> Vec<ResourceBar> {
    bars_for(
        model,
        hardware,
        free,
        chosen_cpu_path(model, hardware),
        true,
    )
}

fn bars_for(
    model: &Model,
    hardware: &HardwareProfile,
    free: &FreeBudget,
    cpu_path: bool,
    gated: bool,
) -> Vec<ResourceBar> {
    let gpu_bar = if hardware.is_unified() {
        bar(
            Resource::UnifiedMemory,
            Some(free.vram_free_bytes),
            model.min_vram_bytes,
            model.recommended_vram_bytes,
            gated && !cpu_path,
        )
    } else {
        bar(
            Resource::Vram,
            Some(free.vram_free_bytes).filter(|v| *v > 0),
            model.min_vram_bytes,
            model.recommended_vram_bytes,
            gated && !cpu_path,
        )
    };

    let ram_bar = bar(
        Resource::Ram,
        Some(free.ram_free_bytes).filter(|v| *v > 0),
        model.min_ram_bytes,
        model.recommended_ram_bytes,
        gated && cpu_path,
    );

    let cpu_bar = bar(
        Resource::CpuCores,
        Some(i64::from(hardware.cores())).filter(|v| *v > 0),
        model.min_cpu_cores.map(i64::from),
        model.recommended_cpu_cores.map(i64::from),
        gated && cpu_path,
    );

    let gpu_count_bar = bar(
        Resource::Gpus,
        Some(i64::from(hardware.gpu_count())),
        Some(i64::from(model.min_gpu_count)),
        None,
        gated && !cpu_path,
    );

    vec![gpu_bar, ram_bar, cpu_bar, gpu_count_bar]
}

fn bar(
    resource: Resource,
    available: Option<i64>,
    minimum: Option<i64>,
    recommended: Option<i64>,
    applies: bool,
) -> ResourceBar {
    let status = if !applies || minimum.is_none() {
        BarStatus::NotRequired
    } else {
        match (available, minimum) {
            (None, _) => BarStatus::Unknown,
            (Some(have), Some(min)) if have < min => BarStatus::Short,
            (Some(have), Some(_)) => match recommended {
                Some(rec) if have < rec => BarStatus::Tight,
                _ => BarStatus::Ok,
            },
            _ => BarStatus::NotRequired,
        }
    };
    ResourceBar {
        resource,
        available: available.unwrap_or(0),
        minimum,
        recommended,
        status,
    }
}

/// Bytes as GiB, rounded, for a human-readable reason string.
///
/// Shared with the reservation rule in `model_install`, so a fit reason and an
/// install refusal state the same bytes the same way.
pub(crate) fn human_gib(bytes: i64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes <= 0 {
        return "0 GiB".into();
    }
    let gib = bytes as f64 / GIB;
    if gib < 10.0 {
        format!("{gib:.1} GiB")
    } else {
        format!("{gib:.0} GiB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model_install::{FreeBudget, ReservationHolder};
    use chrono::Utc;

    const GIB: i64 = 1024 * 1024 * 1024;

    /// The budget of an empty machine: every byte the measurement reports.
    fn free(hardware: &HardwareProfile) -> FreeBudget {
        FreeBudget::new(
            hardware.addressable_ram_bytes(),
            hardware.addressable_memory_bytes(),
            Vec::new(),
        )
    }

    /// Classify against an empty node. Tests that need reservations call
    /// `super::classify` with a budget of their own.
    fn classify(model: &Model, hardware: &HardwareProfile) -> FitReport {
        super::classify(model, hardware, &free(hardware))
    }

    fn resource_bars(model: &Model, hardware: &HardwareProfile) -> Vec<ResourceBar> {
        super::resource_bars(model, hardware, &free(hardware))
    }

    fn hardware(gpus: &[i64], unified: Option<i64>) -> HardwareProfile {
        HardwareProfile {
            id: Uuid::nil(),
            environment_id: Uuid::nil(),
            node_id: Uuid::nil(),
            detected_at: Utc::now(),
            hostname: Some("test".into()),
            gpus: gpus
                .iter()
                .enumerate()
                .map(|(i, m)| GpuDevice {
                    index: i as u32,
                    name: format!("GPU {i}"),
                    memory_bytes: *m,
                    free_bytes: None,
                })
                .collect(),
            total_vram_bytes: gpus.iter().sum(),
            unified_memory_bytes: unified,
            unified_memory_source: None,
            cuda_version: None,
            driver_version: None,
            detection_note: None,
            cpu_model: Some("Test CPU".into()),
            cpu_cores: Some(8),
            cpu_threads: Some(16),
            system_ram_bytes: Some(32 * GIB),
        }
    }

    fn model(min_vram_gib: i64, min_gpus: i32) -> Model {
        Model {
            id: Uuid::nil(),
            key: "m".into(),
            name: "Test Model".into(),
            family: "nemotron".into(),
            parameters: "x".into(),
            context_length: Some(131072),
            quantization: Some("NVFP4".into()),
            weights_bytes: None,
            min_vram_bytes: Some(min_vram_gib * GIB),
            recommended_vram_bytes: None,
            min_gpu_count: min_gpus,
            min_ram_bytes: None,
            recommended_ram_bytes: None,
            min_cpu_cores: None,
            recommended_cpu_cores: None,
            runtime: ModelRuntime::Cuda,
            arch: Some("tensorrt".into()),
            artifact_format: None,
        artifact_ref: None,
        serve_runtime: None,
            publisher: Some("NVIDIA".into()),
            license: None,
            description: None,
            unified_memory_only: false,
            is_cpu_only: false,
            source_uri: None,
            version: "1.0.0".into(),
            installed_version: None,
            state: ModelState::NotInstalled,
            installed_at: None,
            is_remote: false,
            provider_key: None,
        }
    }

    fn cpu_model(min_ram_gib: i64, cores: Option<i32>) -> Model {
        let mut m = model(0, 1);
        m.min_vram_bytes = None;
        m.runtime = ModelRuntime::Cpu;
        m.is_cpu_only = true;
        m.min_ram_bytes = Some(min_ram_gib * GIB);
        m.recommended_ram_bytes = Some(min_ram_gib * 2 * GIB);
        m.min_cpu_cores = cores;
        m
    }

    #[test]
    fn a_machine_with_no_gpu_reports_no_gpu_with_a_reason() {
        let report = classify(&model(24, 1), &hardware(&[], None));
        assert_eq!(report.fit, Fit::NoGpu);
        assert!(report.reason.contains("No GPU detected"));
        assert!(!report.fit.allows_install());
    }

    #[test]
    fn a_model_that_fits_is_installable() {
        let report = classify(&model(24, 1), &hardware(&[24 * GIB], None));
        assert_eq!(report.fit, Fit::Installable);
        assert!(report.fit.allows_install());
    }

    #[test]
    fn insufficient_memory_reports_the_shortfall() {
        let report = classify(&model(80, 1), &hardware(&[24 * GIB], None));
        assert_eq!(report.fit, Fit::InsufficientVram);
        assert_eq!(report.vram_shortfall_bytes, Some(56 * GIB));
        assert!(report.reason.contains("Short by 56 GiB"));
    }

    #[test]
    fn an_install_that_already_reserved_the_ram_makes_a_fitting_model_insufficient() {
        // 16 GiB total, 10 GiB held by one install: a model that needs 8 GiB
        // fits the hardware but not this node, and the reason says who holds it.
        let mut hw = hardware(&[], None);
        hw.system_ram_bytes = Some(16 * GIB);
        let budget = FreeBudget::new(
            16 * GIB,
            0,
            vec![ReservationHolder {
                model_key: "qwen3-1.7b".into(),
                ram_bytes: 10 * GIB,
                vram_bytes: 0,
            }],
        );

        let report = super::classify(&cpu_model(8, None), &hw, &budget);
        assert_eq!(report.fit, Fit::InsufficientRam);
        assert_eq!(report.available_ram_bytes, 6 * GIB);
        assert_eq!(report.ram_shortfall_bytes, Some(2 * GIB));
        assert!(
            report.reason.contains("reserved by qwen3-1.7b"),
            "reason should name the install: {}",
            report.reason
        );
        assert_eq!(super::classify(&cpu_model(4, None), &hw, &budget).fit, Fit::CpuInstallable);
    }

    #[test]
    fn insufficient_gpus_is_distinct_from_insufficient_memory() {
        let report = classify(&model(280, 4), &hardware(&[96 * GIB], None));
        assert_eq!(report.fit, Fit::InsufficientGpus);
        assert!(report.reason.contains("needs 4 GPUs"));
    }

    #[test]
    fn unified_memory_is_used_instead_of_per_card_vram() {
        let report = classify(&model(24, 1), &hardware(&[], Some(128 * GIB)));
        assert_eq!(report.fit, Fit::Installable);
        assert_eq!(report.available_vram_bytes, 128 * GIB);
        assert!(report.reason.contains("unified memory"));
    }

    #[test]
    fn a_cpu_model_fits_a_machine_with_no_gpu() {
        // The whole point of the CPU catalogue: the appliance VM has no GPU.
        let report = classify(&cpu_model(4, Some(2)), &hardware(&[], None));
        assert_eq!(report.fit, Fit::CpuInstallable);
        assert!(report.cpu_path);
        assert!(report.fit.allows_install());
        assert!(report.fit.fits());
    }

    #[test]
    fn a_cpu_model_is_gated_by_ram_not_vram() {
        let report = classify(&cpu_model(64, None), &hardware(&[], None));
        assert_eq!(report.fit, Fit::InsufficientRam);
        assert_eq!(report.ram_shortfall_bytes, Some(32 * GIB));
    }

    #[test]
    fn a_cpu_model_is_gated_by_cores_when_stated() {
        let report = classify(&cpu_model(4, Some(32)), &hardware(&[], None));
        assert_eq!(report.fit, Fit::InsufficientCpu);
    }

    #[test]
    fn a_hybrid_model_uses_the_gpu_when_there_is_one_and_the_cpu_otherwise() {
        let mut m = model(8, 1);
        m.runtime = ModelRuntime::Hybrid;
        m.min_ram_bytes = Some(8 * GIB);

        let on_gpu = classify(&m, &hardware(&[24 * GIB], None));
        assert_eq!(on_gpu.fit, Fit::Installable);
        assert!(!on_gpu.cpu_path);

        let on_cpu = classify(&m, &hardware(&[], None));
        assert_eq!(on_cpu.fit, Fit::CpuInstallable);
        assert!(on_cpu.cpu_path);
    }

    #[test]
    fn a_unified_only_model_cannot_be_rescued_by_a_big_discrete_card() {
        let mut m = model(100, 1);
        m.unified_memory_only = true;
        let report = classify(&m, &hardware(&[96 * GIB], None));
        assert_eq!(report.fit, Fit::NeedsUnifiedMemory);

        let on_spark = classify(&m, &hardware(&[], Some(128 * GIB)));
        assert_eq!(on_spark.fit, Fit::Installable);
    }

    #[test]
    fn bars_mark_what_does_not_apply_rather_than_omitting_it() {
        let bars = resource_bars(&cpu_model(4, Some(2)), &hardware(&[], None));
        assert_eq!(bars.len(), 4);
        let vram = bars.iter().find(|b| b.resource == Resource::Vram).expect("vram bar");
        assert_eq!(vram.status, BarStatus::NotRequired);
        let ram = bars.iter().find(|b| b.resource == Resource::Ram).expect("ram bar");
        assert_eq!(ram.status, BarStatus::Ok);
    }

    #[test]
    fn a_bar_is_tight_between_minimum_and_recommended() {
        let mut m = model(16, 1);
        m.recommended_vram_bytes = Some(32 * GIB);
        let bars = resource_bars(&m, &hardware(&[24 * GIB], None));
        let vram = bars.iter().find(|b| b.resource == Resource::Vram).expect("vram bar");
        assert_eq!(vram.status, BarStatus::Tight);
    }

    #[test]
    fn a_unified_machine_shows_the_pool_bar_instead_of_vram() {
        let bars = resource_bars(&model(24, 1), &hardware(&[], Some(128 * GIB)));
        assert!(bars.iter().any(|b| b.resource == Resource::UnifiedMemory));
        assert!(!bars.iter().any(|b| b.resource == Resource::Vram));
    }

    #[test]
    fn the_bars_follow_the_path_that_decided_the_verdict() {
        // A hybrid model running on the CPU must not show a red GPU bar next to
        // a green "runs on CPU" badge: the GPU is not the gate on this machine.
        let mut m = model(8, 1);
        m.runtime = ModelRuntime::Hybrid;
        m.min_ram_bytes = Some(8 * GIB);
        m.recommended_ram_bytes = Some(64 * GIB);

        let on_cpu = classify(&m, &hardware(&[], None));
        assert_eq!(on_cpu.fit, Fit::CpuInstallable);
        let vram = on_cpu.bars.iter().find(|b| b.resource == Resource::Vram).expect("vram bar");
        assert_eq!(vram.status, BarStatus::NotRequired);
        let ram = on_cpu.bars.iter().find(|b| b.resource == Resource::Ram).expect("ram bar");
        assert_eq!(ram.status, BarStatus::Tight);

        let on_gpu = classify(&m, &hardware(&[24 * GIB], None));
        assert_eq!(on_gpu.fit, Fit::Installable);
        let vram = on_gpu.bars.iter().find(|b| b.resource == Resource::Vram).expect("vram bar");
        assert_eq!(vram.status, BarStatus::Ok);
        let ram = on_gpu.bars.iter().find(|b| b.resource == Resource::Ram).expect("ram bar");
        assert_eq!(ram.status, BarStatus::NotRequired);
    }

    #[test]
    fn a_cloud_model_has_no_gating_bar() {
        let mut m = model(280, 4);
        m.is_remote = true;
        m.state = ModelState::Remote;
        let report = classify(&m, &hardware(&[], None));
        assert!(report.bars.iter().all(|b| b.status == BarStatus::NotRequired));
    }

    #[test]
    fn an_installed_model_is_not_reported_as_uninstallable() {
        let mut m = model(80, 1);
        m.state = ModelState::Installed;
        m.installed_version = Some("1.0.0".into());
        let report = classify(&m, &hardware(&[8 * GIB], None));
        assert_eq!(report.fit, Fit::AlreadyInstalled);
    }

    #[test]
    fn a_newer_version_is_reported_as_an_update() {
        let mut m = model(24, 1);
        m.state = ModelState::Installed;
        m.installed_version = Some("0.9.0".into());
        let report = classify(&m, &hardware(&[24 * GIB], None));
        assert_eq!(report.fit, Fit::UpdateAvailable);
        assert!(report.fit.allows_install());
    }

    #[test]
    fn a_remote_model_needs_no_hardware() {
        let mut m = model(280, 4);
        m.is_remote = true;
        m.state = ModelState::Remote;
        let report = classify(&m, &hardware(&[], None));
        assert_eq!(report.fit, Fit::Remote);
        assert!(!report.fit.allows_install());
        assert!(!report.fit.fits());
    }

    #[test]
    fn the_largest_single_card_is_the_addressable_memory() {
        let hw = hardware(&[24 * GIB, 24 * GIB], None);
        assert_eq!(hw.addressable_memory_bytes(), 24 * GIB);
        assert_eq!(hw.gpu_count(), 2);
    }

    #[test]
    fn small_figures_keep_a_decimal() {
        assert_eq!(human_gib(GIB / 2), "0.5 GiB");
        assert_eq!(human_gib(24 * GIB), "24 GiB");
    }

    #[test]
    fn a_model_that_publishes_no_size_is_unknown_not_installable() {
        // The sync writes such rows for repositories that state no size (ADR
        // 0019). Reading the missing gate as zero would call the model
        // installable on a machine that cannot hold it.
        let mut m = model(0, 1);
        m.min_vram_bytes = None;
        m.weights_bytes = None;
        let report = classify(&m, &hardware(&[24 * GIB], None));
        assert_eq!(report.fit, Fit::Unknown);
        assert!(!report.fit.allows_install());
        assert!(!report.fit.fits());
        assert!(report.reason.contains("does not publish"));
        assert!(report.reason.contains("GPU memory"));
    }

    #[test]
    fn a_known_size_without_a_gate_is_still_unknown() {
        // Weights are known — the files can be weighed — but no VRAM figure was
        // derived from them, so the GPU gate has no number to compare.
        let mut m = model(0, 1);
        m.weights_bytes = Some(8 * GIB);
        m.min_vram_bytes = None;
        let report = classify(&m, &hardware(&[24 * GIB], None));
        assert_eq!(report.fit, Fit::Unknown);
        assert!(!report.fit.allows_install());
    }

    #[test]
    fn a_cpu_model_without_a_ram_figure_is_unknown() {
        let mut m = cpu_model(4, None);
        m.min_ram_bytes = None;
        m.recommended_ram_bytes = None;
        let report = classify(&m, &hardware(&[], None));
        assert_eq!(report.fit, Fit::Unknown);
        assert!(report.reason.contains("RAM"));
        assert!(!report.fit.allows_install());
    }

    #[test]
    fn a_hybrid_model_is_unknown_only_on_the_path_without_a_figure() {
        // A GGUF row with a VRAM gate but no RAM gate runs on the GPU path and
        // is unknown on a machine without one.
        let mut m = model(8, 1);
        m.runtime = ModelRuntime::Hybrid;
        m.min_ram_bytes = None;
        assert_eq!(classify(&m, &hardware(&[24 * GIB], None)).fit, Fit::Installable);
        assert_eq!(classify(&m, &hardware(&[], None)).fit, Fit::Unknown);
    }

    #[test]
    fn an_unknown_model_gates_no_bar() {
        let mut m = model(0, 1);
        m.min_vram_bytes = None;
        let report = classify(&m, &hardware(&[24 * GIB], None));
        assert!(report
            .bars
            .iter()
            .all(|bar| bar.status == BarStatus::NotRequired));
    }

    #[test]
    fn a_published_gate_is_classified_exactly_as_before() {
        let mut m = model(24, 1);
        m.weights_bytes = Some(16 * GIB);
        let report = classify(&m, &hardware(&[24 * GIB], None));
        assert_eq!(report.fit, Fit::Installable);
        assert!(report.fit.allows_install());
    }
}
