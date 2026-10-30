//! One copy of a catalog model on one node.
//!
//! The catalog describes a model; an install is that model on a machine. The
//! same model may be installed on two nodes. An install reserves memory on its
//! node for as long as it exists, which is how a second model that no longer
//! fits is reported honestly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::model::{human_gib, Model, ModelState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    /// Reserved and committed; nothing fetched yet.
    Installing,
    /// The node's runner is fetching the artefact.
    Downloading,
    /// The artefact is on disk and the server container is up, but it has not
    /// answered a probe yet.
    Starting,
    Installed,
    Updating,
    Uninstalling,
    Failed,
}

impl InstallState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installing => "installing",
            Self::Downloading => "downloading",
            Self::Starting => "starting",
            Self::Installed => "installed",
            Self::Updating => "updating",
            Self::Uninstalling => "uninstalling",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "installing" => Self::Installing,
            "downloading" => Self::Downloading,
            "starting" => Self::Starting,
            "installed" => Self::Installed,
            "updating" => Self::Updating,
            "uninstalling" => Self::Uninstalling,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    /// Whether the install still holds its reservation.
    ///
    /// A failed install holds nothing; an uninstalling one still does until the
    /// job finishes, because the server may still be running.
    pub fn reserves(self) -> bool {
        !matches!(self, Self::Failed)
    }

    /// The catalog-facing state of this install.
    ///
    /// The catalog still reports one state per model; when a node is named, the
    /// service substitutes the install's state for that node so "already
    /// installed" and "update available" are answered from `model_installs`.
    pub fn model_state(self) -> ModelState {
        match self {
            Self::Installing | Self::Downloading | Self::Starting => ModelState::Installing,
            Self::Installed => ModelState::Installed,
            Self::Updating => ModelState::Updating,
            Self::Uninstalling => ModelState::Uninstalling,
            Self::Failed => ModelState::Failed,
        }
    }
}

/// The runtime flags an operator tuned for one installed model.
///
/// Every field is optional: an absent field means "leave the image's own
/// default". The set is deliberately the llama.cpp server surface the product
/// actually exposes (context, GPU layers, batching, KV cache, threads, memory
/// behaviour, sampling seed), not an escape hatch for arbitrary arguments: the
/// value is turned into an argument vector here, and the runner never builds a
/// shell string from it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct ModelTuning {
    /// `--ctx-size`: the context window the server allocates.
    pub context_size: Option<i64>,
    /// `--n-gpu-layers`: how many layers go to the GPU (`-1` = all).
    pub gpu_layers: Option<i32>,
    /// `--parallel`: how many requests the server serves at once.
    pub parallel: Option<i32>,
    /// `--batch-size` and `--ubatch-size`: prompt processing chunks.
    pub batch_size: Option<i32>,
    pub ubatch_size: Option<i32>,
    /// `--threads` / `--threads-batch`; absent uses the node's cores.
    pub threads: Option<i32>,
    pub threads_batch: Option<i32>,
    /// `-fa on|off`.
    pub flash_attention: Option<bool>,
    /// `--cache-type-k` / `--cache-type-v` (`f16`, `bf16`, `q8_0`, `q4_0`, ...).
    pub cache_type_k: Option<String>,
    pub cache_type_v: Option<String>,
    /// `--no-mmap` when false.
    pub mmap: Option<bool>,
    /// `--mlock`.
    pub mlock: Option<bool>,
    /// `--no-kv-offload` when false.
    pub kv_offload: Option<bool>,
    /// `--tensor-split`: per-GPU proportions, comma separated.
    pub tensor_split: Option<String>,
    /// `--main-gpu`.
    pub main_gpu: Option<i32>,
    /// `--seed`.
    pub seed: Option<i64>,
    /// `--metrics`: on unless explicitly switched off, because the model's
    /// graphs and token counts depend on it.
    pub metrics: Option<bool>,
    /// `--jinja`: use the model's chat template.
    pub jinja: Option<bool>,
}

/// Cache types llama.cpp accepts for the KV cache.
const CACHE_TYPES: &[&str] = &["f16", "bf16", "q8_0", "q4_0", "q4_1", "iq4_nl", "q5_0", "q5_1"];

impl ModelTuning {
    /// Reject values that cannot work, and warn about values that can but are
    /// expensive. Warnings do not block: the same rule as an install.
    pub fn validate(&self) -> Result<Vec<String>, String> {
        let mut warnings = Vec::new();
        if let Some(context) = self.context_size {
            if context < 128 {
                return Err("context_size must be at least 128".to_string());
            }
            if context > 1_048_576 {
                return Err("context_size above 1048576 is not supported".to_string());
            }
            if context > 131_072 {
                warnings.push(format!(
                    "A context of {context} tokens needs a large KV cache; watch the node's memory."
                ));
            }
        }
        if let Some(layers) = self.gpu_layers {
            if !(-1..=999).contains(&layers) {
                return Err("gpu_layers must be between -1 and 999".to_string());
            }
        }
        if let Some(parallel) = self.parallel {
            if !(1..=256).contains(&parallel) {
                return Err("parallel must be between 1 and 256".to_string());
            }
        }
        for (name, value, low, high) in [
            ("batch_size", self.batch_size, 32, 16_384),
            ("ubatch_size", self.ubatch_size, 32, 16_384),
            ("threads", self.threads, 1, 1024),
            ("threads_batch", self.threads_batch, 1, 1024),
        ] {
            if let Some(value) = value {
                if !(low..=high).contains(&value) {
                    return Err(format!("{name} must be between {low} and {high}"));
                }
            }
        }
        if let (Some(batch), Some(ubatch)) = (self.batch_size, self.ubatch_size) {
            if ubatch > batch {
                return Err("ubatch_size cannot be larger than batch_size".to_string());
            }
        }
        for (name, value) in [
            ("cache_type_k", self.cache_type_k.as_deref()),
            ("cache_type_v", self.cache_type_v.as_deref()),
        ] {
            if let Some(value) = value {
                if !CACHE_TYPES.contains(&value) {
                    return Err(format!(
                        "{name} must be one of {}",
                        CACHE_TYPES.join(", ")
                    ));
                }
            }
        }
        if let Some(split) = self.tensor_split.as_deref() {
            if split.split(',').any(|part| part.trim().parse::<f32>().is_err()) {
                return Err("tensor_split must be comma separated numbers".to_string());
            }
        }
        if let (Some(context), Some(parallel)) = (self.context_size, self.parallel) {
            if context.saturating_mul(i64::from(parallel)) > 4_194_304 {
                warnings.push(
                    "context_size × parallel is very large; the KV cache may not fit the node."
                        .to_string(),
                );
            }
        }
        Ok(warnings)
    }

    /// Rewrite the image's command with the tuned flags.
    ///
    /// Controlled flags are removed first, so a value replaces the image default
    /// instead of being appended after it and depending on llama.cpp's handling
    /// of repeated flags.
    pub fn apply(&self, command: &mut Vec<String>, threads: i64) {
        for flag in [
            "--ctx-size",
            "--n-gpu-layers",
            "--parallel",
            "--batch-size",
            "--ubatch-size",
            "--threads",
            "--threads-batch",
            "-fa",
            "--flash-attn",
            "--cache-type-k",
            "--cache-type-v",
            "--no-mmap",
            "--mlock",
            "--no-kv-offload",
            "--tensor-split",
            "--main-gpu",
            "--seed",
            "--metrics",
            "--jinja",
        ] {
            strip_flag(command, flag);
        }
        if let Some(value) = self.context_size {
            push_flag(command, "--ctx-size", value.to_string());
        }
        if let Some(value) = self.gpu_layers {
            push_flag(command, "--n-gpu-layers", value.to_string());
        }
        if let Some(value) = self.parallel {
            push_flag(command, "--parallel", value.to_string());
        }
        if let Some(value) = self.batch_size {
            push_flag(command, "--batch-size", value.to_string());
        }
        if let Some(value) = self.ubatch_size {
            push_flag(command, "--ubatch-size", value.to_string());
        }
        if let Some(value) = self.threads {
            push_flag(command, "--threads", value.to_string());
        } else {
            push_flag(command, "--threads", threads.max(1).to_string());
        }
        if let Some(value) = self.threads_batch {
            push_flag(command, "--threads-batch", value.to_string());
        }
        if let Some(value) = self.flash_attention {
            push_flag(command, "-fa", if value { "on".into() } else { "off".into() });
        }
        if let Some(value) = self.cache_type_k.as_deref() {
            push_flag(command, "--cache-type-k", value.to_string());
        }
        if let Some(value) = self.cache_type_v.as_deref() {
            push_flag(command, "--cache-type-v", value.to_string());
        }
        if self.mmap == Some(false) {
            command.push("--no-mmap".to_string());
        }
        if self.mlock == Some(true) {
            command.push("--mlock".to_string());
        }
        if self.kv_offload == Some(false) {
            command.push("--no-kv-offload".to_string());
        }
        if let Some(value) = self.tensor_split.as_deref() {
            push_flag(command, "--tensor-split", value.to_string());
        }
        if let Some(value) = self.main_gpu {
            push_flag(command, "--main-gpu", value.to_string());
        }
        if let Some(value) = self.seed {
            push_flag(command, "--seed", value.to_string());
        }
        if self.metrics != Some(false) {
            command.push("--metrics".to_string());
        }
        if self.jinja == Some(true) {
            command.push("--jinja".to_string());
        }
    }
}

/// Append `--flag value`.
fn push_flag(command: &mut Vec<String>, flag: &str, value: String) {
    command.push(flag.to_string());
    command.push(value);
}

/// Remove a flag and its value, if the command has it.
fn strip_flag(command: &mut Vec<String>, flag: &str) {
    let mut index = 0;
    while index < command.len() {
        if command[index] == flag {
            let takes_value = command
                .get(index + 1)
                .is_some_and(|next| !next.starts_with('-') || next.parse::<f64>().is_ok());
            let count = if takes_value { 2 } else { 1 };
            command.drain(index..(index + count).min(command.len()));
        } else {
            index += 1;
        }
    }
}

/// A model install as stored and returned.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelInstall {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub model_key: String,
    pub node_id: Uuid,
    pub state: InstallState,
    pub installed_version: Option<String>,
    pub reserved_ram_bytes: i64,
    pub reserved_vram_bytes: i64,
    /// Whether the server runs and the reservation counts against the node.
    /// Off keeps the artefact and the figures, and frees the memory.
    pub enabled: bool,
    pub provider_profile_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    /// The operator's runtime flags; empty means the image defaults.
    #[serde(default)]
    pub settings: ModelTuning,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What an install of this model reserves on a node.
///
/// Recommended where the catalog has it, minimum otherwise. A GPU model
/// reserves VRAM; a CPU model reserves RAM; a hybrid reserves whichever path
/// the node will take, which the caller decides from the node's hardware.
pub fn reservation_for(model: &Model, cpu_path: bool) -> (i64, i64) {
    let ram = model
        .recommended_ram_bytes
        .or(model.min_ram_bytes)
        .unwrap_or(0);
    let vram = model
        .recommended_vram_bytes
        .or(model.min_vram_bytes)
        .unwrap_or(0);
    if cpu_path {
        (ram, 0)
    } else {
        // A GPU server still needs host RAM to stage the weights; the catalog's
        // RAM figure covers that where present.
        (ram, vram)
    }
}

/// One install's share of a node, for the sentence that explains a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationHolder {
    pub model_key: String,
    pub ram_bytes: i64,
    pub vram_bytes: i64,
}

impl ReservationHolder {
    /// The holding of one install, or nothing when it reserves no memory.
    ///
    /// A failed install reserves nothing, and an install whose catalog row has
    /// no numbers holds zero; neither belongs in a reason string.
    pub fn of(install: &ModelInstall) -> Option<Self> {
        if !install.state.reserves() || !install.enabled {
            return None;
        }
        let ram = install.reserved_ram_bytes.max(0);
        let vram = install.reserved_vram_bytes.max(0);
        if ram == 0 && vram == 0 {
            return None;
        }
        Some(Self {
            model_key: install.model_key.clone(),
            ram_bytes: ram,
            vram_bytes: vram,
        })
    }
}

/// What a node has left after its installs reserved their share.
///
/// This is the arithmetic the fit runs against (ADR 0018): the totals come from
/// the latest measurement, the reservations from the installs on that node, and
/// the difference is what a new model may use. `free` never goes below zero, so
/// an over-subscribed node reads as "0 GiB free" rather than a negative number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FreeBudget {
    pub ram_total_bytes: i64,
    pub ram_reserved_bytes: i64,
    pub ram_free_bytes: i64,
    pub vram_total_bytes: i64,
    pub vram_reserved_bytes: i64,
    pub vram_free_bytes: i64,
    /// The installs that hold the reserved memory, by model key.
    pub holders: Vec<ReservationHolder>,
}

impl FreeBudget {
    /// Build the budget from explicit holdings; used by the service and tests.
    pub fn new(
        ram_total_bytes: i64,
        vram_total_bytes: i64,
        holders: Vec<ReservationHolder>,
    ) -> Self {
        let ram_reserved_bytes = holders.iter().map(|h| h.ram_bytes).sum::<i64>().max(0);
        let vram_reserved_bytes = holders.iter().map(|h| h.vram_bytes).sum::<i64>().max(0);
        Self {
            ram_total_bytes: ram_total_bytes.max(0),
            ram_free_bytes: (ram_total_bytes.max(0) - ram_reserved_bytes).max(0),
            ram_reserved_bytes,
            vram_total_bytes: vram_total_bytes.max(0),
            vram_free_bytes: (vram_total_bytes.max(0) - vram_reserved_bytes).max(0),
            vram_reserved_bytes,
            holders,
        }
    }

    /// Build the budget from the installs on one node.
    ///
    /// Failed installs are left out here rather than at the call site, because
    /// "a failed install holds nothing" is a rule, not a caller's choice.
    pub fn from_installs(
        ram_total_bytes: i64,
        vram_total_bytes: i64,
        installs: &[ModelInstall],
    ) -> Self {
        let holders = installs
            .iter()
            .filter_map(ReservationHolder::of)
            .collect();
        Self::new(ram_total_bytes, vram_total_bytes, holders)
    }

    /// Whether a new reservation fits what is left.
    ///
    /// The reason names the installs that hold the memory, because "does not
    /// fit" without "these are holding it" leaves the operator guessing.
    pub fn check(&self, ram_bytes: i64, vram_bytes: i64) -> Result<(), InsufficientFit> {
        if ram_bytes > self.ram_free_bytes {
            return Err(InsufficientFit::Ram {
                requested: ram_bytes,
                free: self.ram_free_bytes,
                total: self.ram_total_bytes,
                reserved: self.ram_reserved_bytes,
                reserved_by: self.ram_holders(),
            });
        }
        if vram_bytes > self.vram_free_bytes {
            return Err(InsufficientFit::Vram {
                requested: vram_bytes,
                free: self.vram_free_bytes,
                total: self.vram_total_bytes,
                reserved: self.vram_reserved_bytes,
                reserved_by: self.vram_holders(),
            });
        }
        Ok(())
    }

    /// The installs that hold RAM, as one readable clause.
    pub fn ram_holders(&self) -> String {
        holder_names(&self.holders, true)
    }

    /// The installs that hold VRAM, as one readable clause.
    pub fn vram_holders(&self) -> String {
        holder_names(&self.holders, false)
    }
}

fn holder_names(holders: &[ReservationHolder], ram: bool) -> String {
    let names: Vec<&str> = holders
        .iter()
        .filter(|holder| (if ram { holder.ram_bytes } else { holder.vram_bytes }) > 0)
        .map(|holder| holder.model_key.as_str())
        .collect();
    if names.is_empty() {
        "an install that is no longer recorded".to_string()
    } else {
        names.join(", ")
    }
}

/// Why an install's reservation does not fit a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsufficientFit {
    Ram {
        requested: i64,
        free: i64,
        total: i64,
        reserved: i64,
        reserved_by: String,
    },
    Vram {
        requested: i64,
        free: i64,
        total: i64,
        reserved: i64,
        reserved_by: String,
    },
}

impl InsufficientFit {
    /// A sentence for `node.insufficient_resources`, with the numbers that
    /// produced it.
    pub fn message(&self) -> String {
        match self {
            Self::Ram {
                requested,
                free,
                total,
                reserved,
                reserved_by,
            } => shortage("RAM", *requested, *free, *total, *reserved, reserved_by),
            Self::Vram {
                requested,
                free,
                total,
                reserved,
                reserved_by,
            } => shortage(
                "GPU memory",
                *requested,
                *free,
                *total,
                *reserved,
                reserved_by,
            ),
        }
    }
}

fn shortage(
    resource: &str,
    requested: i64,
    free: i64,
    total: i64,
    reserved: i64,
    reserved_by: &str,
) -> String {
    if reserved > 0 {
        format!(
            "{} of {} of {resource} are reserved by {reserved_by}; the install needs {} and only {} are free.",
            human_gib(reserved),
            human_gib(total),
            human_gib(requested),
            human_gib(free)
        )
    } else {
        format!(
            "The node has {} of {resource} free; the install needs {}.",
            human_gib(free),
            human_gib(requested)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::{ModelRuntime, ModelState};

    const GIB: i64 = 1024 * 1024 * 1024;

    fn model(min_vram: Option<i64>, rec_vram: Option<i64>, min_ram: Option<i64>, rec_ram: Option<i64>) -> Model {
        Model {
            id: Uuid::nil(),
            key: "m".into(),
            name: "M".into(),
            family: "f".into(),
            parameters: "x".into(),
            context_length: None,
            quantization: None,
            weights_bytes: None,
            min_vram_bytes: min_vram,
            recommended_vram_bytes: rec_vram,
            min_gpu_count: 1,
            min_ram_bytes: min_ram,
            recommended_ram_bytes: rec_ram,
            min_cpu_cores: None,
            recommended_cpu_cores: None,
            runtime: ModelRuntime::Hybrid,
            arch: None,
            artifact_format: None,
            artifact_ref: None,
            serve_runtime: None,
            publisher: None,
            license: None,
            description: None,
            unified_memory_only: false,
            is_cpu_only: false,
            source_uri: None,
            version: "1".into(),
            installed_version: None,
            state: ModelState::NotInstalled,
            installed_at: None,
            is_remote: false,
            provider_key: None,
        }
    }

    #[test]
    fn the_recommended_figure_is_reserved_when_present() {
        let m = model(Some(8 * GIB), Some(12 * GIB), Some(8 * GIB), Some(16 * GIB));
        assert_eq!(reservation_for(&m, false), (16 * GIB, 12 * GIB));
        assert_eq!(reservation_for(&m, true), (16 * GIB, 0));
    }

    #[test]
    fn the_minimum_is_the_fallback() {
        let m = model(Some(8 * GIB), None, Some(8 * GIB), None);
        assert_eq!(reservation_for(&m, false), (8 * GIB, 8 * GIB));
    }

    #[test]
    fn a_failed_install_holds_nothing() {
        assert!(!InstallState::Failed.reserves());
        assert!(InstallState::Uninstalling.reserves());
    }

    fn install(key: &str, state: InstallState, ram: i64, vram: i64) -> ModelInstall {
        ModelInstall {
            id: Uuid::nil(),
            environment_id: Uuid::nil(),
            model_key: key.into(),
            node_id: Uuid::nil(),
            state,
            installed_version: None,
            reserved_ram_bytes: ram,
            reserved_vram_bytes: vram,
            enabled: true,
            provider_profile_id: None,
            job_id: None,
            settings: ModelTuning::default(),
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn a_disabled_install_holds_no_memory() {
        let mut off = install("m", InstallState::Installed, 4 << 30, 0);
        off.enabled = false;
        let on = install("n", InstallState::Installed, 2 << 30, 0);
        let free = FreeBudget::from_installs(16 << 30, 16 << 30, &[off, on]);
        assert_eq!(free.ram_reserved_bytes, 2 << 30);
    }

    #[test]
    fn the_free_budget_is_the_total_minus_the_reservations() {
        let installs = [install("qwen3-1.7b", InstallState::Installed, 5 * GIB, 0)];
        let budget = FreeBudget::from_installs(16 * GIB, 0, &installs);
        assert_eq!(budget.ram_reserved_bytes, 5 * GIB);
        assert_eq!(budget.ram_free_bytes, 11 * GIB);
        assert!(budget.check(8 * GIB, 0).is_ok());
        assert_eq!(budget.ram_holders(), "qwen3-1.7b");
    }

    #[test]
    fn a_failed_install_is_not_subtracted() {
        let installs = [
            install("qwen3-1.7b", InstallState::Failed, 5 * GIB, 0),
            install("gemma-4-12b", InstallState::Installed, 4 * GIB, 0),
        ];
        let budget = FreeBudget::from_installs(16 * GIB, 0, &installs);
        assert_eq!(budget.ram_reserved_bytes, 4 * GIB);
        assert_eq!(budget.ram_free_bytes, 12 * GIB);
        assert_eq!(budget.ram_holders(), "gemma-4-12b");
    }

    #[test]
    fn removing_an_install_gives_the_memory_back() {
        let with_install = [install("qwen3-1.7b", InstallState::Installed, 5 * GIB, 0)];
        let before = FreeBudget::from_installs(16 * GIB, 0, &with_install);
        let after = FreeBudget::from_installs(16 * GIB, 0, &[]);
        assert_eq!(before.ram_free_bytes, 11 * GIB);
        assert_eq!(after.ram_free_bytes, 16 * GIB);
        assert_eq!(after.ram_reserved_bytes, 0);
    }

    #[test]
    fn free_never_goes_negative_when_reservations_exceed_the_measurement() {
        let installs = [install("huge", InstallState::Installed, 20 * GIB, 0)];
        let budget = FreeBudget::from_installs(16 * GIB, 0, &installs);
        assert_eq!(budget.ram_reserved_bytes, 20 * GIB);
        assert_eq!(budget.ram_free_bytes, 0);
    }

    #[test]
    fn a_reservation_beyond_the_free_budget_names_the_install_that_holds_it() {
        let installs = [install("qwen3-1.7b", InstallState::Installed, 10 * GIB, 0)];
        let budget = FreeBudget::from_installs(16 * GIB, 0, &installs);
        let error = budget
            .check(8 * GIB, 0)
            .expect_err("8 GiB cannot fit 6 GiB free");
        assert_eq!(
            error,
            InsufficientFit::Ram {
                requested: 8 * GIB,
                free: 6 * GIB,
                total: 16 * GIB,
                reserved: 10 * GIB,
                reserved_by: "qwen3-1.7b".into(),
            }
        );
        let message = error.message();
        assert!(message.contains("10 GiB of 16 GiB of RAM are reserved by qwen3-1.7b"));
        assert!(message.contains("only 6.0 GiB are free"));
    }

    #[test]
    fn an_install_maps_to_the_catalog_state_it_represents() {
        assert_eq!(
            InstallState::Installed.model_state(),
            ModelState::Installed
        );
        assert_eq!(InstallState::Failed.model_state(), ModelState::Failed);
    }
}

/// An approved image that serves a model, from `model_server_images`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelServerImage {
    /// `llama_cpp` today; the column exists so a second runtime is a row, not
    /// a code change.
    pub runtime: String,
    /// The repository, without the digest.
    pub image: String,
    /// `repo@sha256:…`, the only form a node starts.
    pub digest: String,
    /// The port the server listens on inside its container.
    pub port: u16,
    /// The argument vector, with `{model}` and `{threads}` as placeholders.
    pub command: Vec<String>,
    pub notes: Option<String>,
}

/// One model artefact, parsed from `models.artifact_ref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    /// `org/repo` on Hugging Face.
    pub repo: String,
    /// The file inside the repository.
    pub file: String,
}

impl Artifact {
    /// Parse `huggingface:<repo>/<file>`.
    ///
    /// The shape is checked by a database constraint as well; parsing here means
    /// a row that slipped through is reported as "cannot be served" instead of
    /// becoming a download URL assembled from unvalidated text.
    pub fn parse(reference: &str) -> Option<Self> {
        let rest = reference.strip_prefix("huggingface:")?;
        let (repo, file) = rest.rsplit_once('/')?;
        let mut parts = repo.split('/');
        let owner = parts.next()?;
        let name = parts.next()?;
        if parts.next().is_some() || owner.is_empty() || name.is_empty() || file.is_empty() {
            return None;
        }
        Some(Self {
            repo: format!("{owner}/{name}"),
            file: file.to_string(),
        })
    }
}

impl ModelServerImage {
    /// Turn the stored template into the argument vector to start.
    ///
    /// Only the two placeholders are substituted, and `{model}` becomes the
    /// path inside the container. The runner re-checks that path, so a template
    /// that names something else is refused on the node instead of mounted.
    pub fn command_for(&self, model_path: &str, threads: i64) -> Vec<String> {
        self.command
            .iter()
            .map(|part| {
                part.replace("{model}", model_path)
                    .replace("{threads}", &threads.max(1).to_string())
            })
            .collect()
    }
}

#[cfg(test)]
mod serving_tests {
    use super::{Artifact, ModelServerImage};

    #[test]
    fn every_serving_state_round_trips_and_holds_its_reservation() {
        use super::InstallState;
        use crate::domain::model::ModelState;
        for state in [
            InstallState::Installing,
            InstallState::Downloading,
            InstallState::Starting,
            InstallState::Installed,
            InstallState::Updating,
            InstallState::Uninstalling,
        ] {
            assert_eq!(InstallState::parse(state.as_str()), Some(state));
            assert!(state.reserves(), "{} must keep its reservation", state.as_str());
        }
        // A failed install holds nothing, which is what makes a retry possible
        // without a second reservation.
        assert!(!InstallState::Failed.reserves());
        // The three phases before "installed" all read as `installing` in the
        // catalogue: there is one installed state, and it is the one that
        // answers.
        for state in [
            InstallState::Installing,
            InstallState::Downloading,
            InstallState::Starting,
        ] {
            assert_eq!(state.model_state(), ModelState::Installing);
        }
        assert_eq!(InstallState::Installed.model_state(), ModelState::Installed);
    }

    #[test]
    fn artifact_reference_is_repo_and_file() {
        let artifact = Artifact::parse("huggingface:Qwen/Qwen3-1.7B-GGUF/Qwen3-1.7B-Q8_0.gguf")
            .expect("parse");
        assert_eq!(artifact.repo, "Qwen/Qwen3-1.7B-GGUF");
        assert_eq!(artifact.file, "Qwen3-1.7B-Q8_0.gguf");
    }

    #[test]
    fn artifact_reference_refuses_other_schemes_and_shapes() {
        for value in [
            "",
            "https://huggingface.co/Qwen/x/y.gguf",
            "huggingface:Qwen",
            "huggingface:/y.gguf",
            "huggingface:a/b/c/y.gguf",
        ] {
            assert!(Artifact::parse(value).is_none(), "{value} should not parse");
        }
    }

    #[test]
    fn server_command_substitutes_both_placeholders() {
        let image = ModelServerImage {
            runtime: "llama_cpp".into(),
            image: "ghcr.io/ggml-org/llama.cpp".into(),
            digest: "ghcr.io/ggml-org/llama.cpp@sha256:0".into(),
            port: 8080,
            command: vec!["-m".into(), "{model}".into(), "--threads".into(), "{threads}".into()],
            notes: None,
        };
        let command = image.command_for("/models/m.gguf", 0);
        assert_eq!(command, vec!["-m", "/models/m.gguf", "--threads", "1"]);
    }
}

#[cfg(test)]
mod tuning_tests {
    use super::ModelTuning;

    fn command() -> Vec<String> {
        vec![
            "llama-server".to_string(),
            "-m".to_string(),
            "/models/x.gguf".to_string(),
            "--ctx-size".to_string(),
            "4096".to_string(),
            "--threads".to_string(),
            "8".to_string(),
        ]
    }

    #[test]
    fn tuning_replaces_the_flags_it_controls() {
        let tuning = ModelTuning {
            context_size: Some(32768),
            threads: Some(4),
            flash_attention: Some(true),
            cache_type_k: Some("q8_0".to_string()),
            metrics: Some(true),
            ..Default::default()
        };
        let mut args = command();
        tuning.apply(&mut args, 8);
        let joined = args.join(" ");
        assert_eq!(joined.matches("--ctx-size").count(), 1, "{joined}");
        assert!(joined.contains("--ctx-size 32768"));
        assert_eq!(joined.matches("--threads").count(), 1, "{joined}");
        assert!(joined.contains("--threads 4"));
        assert!(joined.contains("-fa on"));
        assert!(joined.contains("--cache-type-k q8_0"));
        assert!(joined.contains("--metrics"));
    }

    #[test]
    fn metrics_are_on_unless_explicitly_off() {
        let mut args = command();
        ModelTuning::default().apply(&mut args, 8);
        assert!(args.join(" ").contains("--metrics"));
        let mut args = command();
        ModelTuning {
            metrics: Some(false),
            ..Default::default()
        }
        .apply(&mut args, 8);
        assert!(!args.join(" ").contains("--metrics"));
    }

    #[test]
    fn impossible_values_are_errors_and_expensive_ones_are_warnings() {
        let err = ModelTuning {
            context_size: Some(16),
            ..Default::default()
        }
        .validate()
        .expect_err("context below 128 must be refused");
        assert!(err.contains("context_size"));

        let warnings = ModelTuning {
            context_size: Some(262_144),
            parallel: Some(32),
            ..Default::default()
        }
        .validate()
        .expect("a large context is a warning, not an error");
        assert!(!warnings.is_empty());

        assert!(ModelTuning {
            cache_type_k: Some("q2_k".to_string()),
            ..Default::default()
        }
        .validate()
        .is_err());

        assert!(ModelTuning {
            batch_size: Some(128),
            ubatch_size: Some(256),
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
