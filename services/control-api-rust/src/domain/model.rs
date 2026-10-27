//! The model catalog, hardware detection, and the fit classification.
//!
//! The classifier is the feature: an operator must be able to see, before
//! clicking install, which models this machine can run and why not when it
//! cannot (ADR 0013).
//!
//! The classification and the shortfall both come from here, so the badge in the
//! console and the gate in the API cannot disagree. The client renders the
//! verdict; it does not compute it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

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
    pub detected_at: DateTime<Utc>,
    pub hostname: Option<String>,
    pub gpus: Vec<GpuDevice>,
    pub total_vram_bytes: i64,
    /// A GB10-class machine reports unified memory rather than per-card VRAM.
    /// Storing it separately is what lets the classifier treat it correctly
    /// instead of adding it to VRAM and double-counting.
    pub unified_memory_bytes: Option<i64>,
    pub cuda_version: Option<String>,
    pub driver_version: Option<String>,
    /// Why detection produced what it did: no driver, no devices, permission.
    pub detection_note: Option<String>,
}

impl HardwareProfile {
    /// The memory a single model can address.
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

    /// Whether the machine can host models at all.
    pub fn has_gpu(&self) -> bool {
        !self.gpus.is_empty() || self.unified_memory_bytes.unwrap_or(0) > 0
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
    /// The gate: what the machine must have.
    pub min_vram_bytes: Option<i64>,
    pub min_gpu_count: i32,
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
    Installable,
    AlreadyInstalled,
    UpdateAvailable,
    /// Enough GPUs but not enough memory on one.
    InsufficientVram,
    /// Enough memory per card but too few cards.
    InsufficientGpus,
    /// No GPU at all.
    NoGpu,
    /// Served by a provider; nothing to install.
    Remote,
}

impl Fit {
    /// Whether the install button should be enabled.
    pub fn allows_install(self) -> bool {
        matches!(self, Self::Installable | Self::UpdateAvailable)
    }
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
    /// A sentence the console can show verbatim.
    pub reason: String,
}

/// Classify a model against a machine.
///
/// Order matters: remote first (nothing to check), then already installed, then
/// the hardware gates. Checking hardware before "already installed" would show a
/// model as uninstallable on a machine where it is running.
pub fn classify(model: &Model, hardware: &HardwareProfile) -> FitReport {
    if model.is_remote || model.state == ModelState::Remote {
        return FitReport {
            fit: Fit::Remote,
            required_vram_bytes: None,
            required_gpus: 0,
            available_vram_bytes: hardware.addressable_memory_bytes(),
            available_gpus: hardware.gpu_count(),
            vram_shortfall_bytes: None,
            reason: "Served by a cloud provider; nothing to install on this machine.".into(),
        };
    }

    if model.state == ModelState::Installed {
        let update = model
            .installed_version
            .as_deref()
            .is_some_and(|installed| installed != model.version);
        return FitReport {
            fit: if update {
                Fit::UpdateAvailable
            } else {
                Fit::AlreadyInstalled
            },
            required_vram_bytes: model.min_vram_bytes,
            required_gpus: model.min_gpu_count,
            available_vram_bytes: hardware.addressable_memory_bytes(),
            available_gpus: hardware.gpu_count(),
            vram_shortfall_bytes: None,
            reason: if update {
                format!(
                    "Installed version {} can be updated to {}.",
                    model.installed_version.as_deref().unwrap_or("unknown"),
                    model.version
                )
            } else {
                format!("Installed and up to date ({}).", model.version)
            },
        };
    }

    let available_vram = hardware.addressable_memory_bytes();
    let available_gpus = hardware.gpu_count();
    let required_vram = model.min_vram_bytes.unwrap_or(0);

    if !hardware.has_gpu() {
        return FitReport {
            fit: Fit::NoGpu,
            required_vram_bytes: model.min_vram_bytes,
            required_gpus: model.min_gpu_count,
            available_vram_bytes: 0,
            available_gpus: 0,
            vram_shortfall_bytes: None,
            reason: format!(
                "No GPU detected. {} needs about {}.",
                model.name,
                human_gib(required_vram)
            ),
        };
    }

    if available_gpus < model.min_gpu_count {
        return FitReport {
            fit: Fit::InsufficientGpus,
            required_vram_bytes: model.min_vram_bytes,
            required_gpus: model.min_gpu_count,
            available_vram_bytes: available_vram,
            available_gpus,
            vram_shortfall_bytes: None,
            reason: format!(
                "{} needs {} GPUs; this machine has {}.",
                model.name, model.min_gpu_count, available_gpus
            ),
        };
    }

    if available_vram < required_vram {
        let shortfall = required_vram - available_vram;
        return FitReport {
            fit: Fit::InsufficientVram,
            required_vram_bytes: model.min_vram_bytes,
            required_gpus: model.min_gpu_count,
            available_vram_bytes: available_vram,
            available_gpus,
            vram_shortfall_bytes: Some(shortfall),
            reason: format!(
                "{} needs about {}; this machine has {}. Short by {}.",
                model.name,
                human_gib(required_vram),
                human_gib(available_vram),
                human_gib(shortfall)
            ),
        };
    }

    FitReport {
        fit: Fit::Installable,
        required_vram_bytes: model.min_vram_bytes,
        required_gpus: model.min_gpu_count,
        available_vram_bytes: available_vram,
        available_gpus,
        vram_shortfall_bytes: None,
        reason: format!(
            "Fits: needs about {} of {}, this machine has {}.",
            human_gib(required_vram),
            human_gib(available_vram),
            human_gib(available_vram)
        ),
    }
}

/// Bytes as GiB, rounded, for a human-readable reason string.
fn human_gib(bytes: i64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes <= 0 {
        return "0 GiB".into();
    }
    format!("{:.0} GiB", bytes as f64 / GIB)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    const GIB: i64 = 1024 * 1024 * 1024;

    fn hardware(gpus: &[i64], unified: Option<i64>) -> HardwareProfile {
        HardwareProfile {
            id: Uuid::nil(),
            environment_id: Uuid::nil(),
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
            cuda_version: None,
            driver_version: None,
            detection_note: None,
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
            min_gpu_count: min_gpus,
            source_uri: None,
            version: "1.0.0".into(),
            installed_version: None,
            state: ModelState::NotInstalled,
            installed_at: None,
            is_remote: false,
            provider_key: None,
        }
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
    fn insufficient_gpus_is_distinct_from_insufficient_memory() {
        // Enough per card, too few cards: a different problem for the operator.
        let report = classify(&model(280, 4), &hardware(&[96 * GIB], None));
        assert_eq!(report.fit, Fit::InsufficientGpus);
        assert!(report.reason.contains("needs 4 GPUs"));
    }

    #[test]
    fn unified_memory_is_used_instead_of_per_card_vram() {
        // A GB10-class machine: one unified pool, no discrete cards.
        let report = classify(&model(24, 1), &hardware(&[], Some(128 * GIB)));
        assert_eq!(report.fit, Fit::Installable);
        assert_eq!(report.available_vram_bytes, 128 * GIB);
    }

    #[test]
    fn an_installed_model_is_not_reported_as_uninstallable() {
        let mut m = model(80, 1);
        m.state = ModelState::Installed;
        m.installed_version = Some("1.0.0".into());
        // Even on hardware that would not fit it now.
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
    }

    #[test]
    fn the_largest_single_card_is_the_addressable_memory() {
        // Two 24 GiB cards do not make a 48 GiB model fit without tensor
        // parallelism, which is expressed by min_gpu_count instead.
        let hw = hardware(&[24 * GIB, 24 * GIB], None);
        assert_eq!(hw.addressable_memory_bytes(), 24 * GIB);
        assert_eq!(hw.gpu_count(), 2);
    }
}
