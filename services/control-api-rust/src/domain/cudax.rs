//! CUDA-X components and their installation on a node.
//!
//! A component is a library or runtime image (CUDA, cuDNN, NCCL, TensorRT-LLM,
//! NIM, ...). Installing one reserves nothing and starts nothing; it makes the
//! software available to model servers on that node.
//!
//! The rule this module exists to keep: **the assessment warns, it never
//! gates.** A node without a GPU, an old driver or a missing container runtime
//! produces a stored warning and the install proceeds if the operator asked for
//! it. `InstalledUnsupported` is an honest state, not a failure.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::model::HardwareProfile;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CudaxComponent {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub category: String,
    pub version: String,
    pub description: String,
    /// Packages the runner may install; declarative, from the catalog only.
    pub apt_packages: Vec<String>,
    /// Images the runner may pull; declarative, from the catalog only.
    pub container_images: Vec<String>,
    pub requires_gpu: bool,
    pub min_driver: String,
    pub min_cuda: String,
    pub docs_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CudaxState {
    /// Assigned to the node, nothing attempted yet.
    Planned,
    Installing,
    Installed,
    /// On disk, but the node cannot use it (no GPU, old driver, no container GPU).
    InstalledUnsupported,
    Failed,
    Removed,
}

impl CudaxState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Installing => "installing",
            Self::Installed => "installed",
            Self::InstalledUnsupported => "installed_unsupported",
            Self::Failed => "failed",
            Self::Removed => "removed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "planned" => Self::Planned,
            "installing" => Self::Installing,
            "installed" => Self::Installed,
            "installed_unsupported" => Self::InstalledUnsupported,
            "failed" => Self::Failed,
            "removed" => Self::Removed,
            _ => return None,
        })
    }
}

/// What the operator is told before and after an assignment.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LibraryAssessment {
    /// No warnings at all. Only then may a caller claim the node supports it.
    pub supported: bool,
    pub warnings: Vec<String>,
    pub detected_gpu: String,
    pub detected_driver: Option<String>,
    pub detected_cuda: Option<String>,
}

impl LibraryAssessment {
    pub fn none(detected_gpu: String) -> Self {
        Self {
            supported: true,
            warnings: Vec::new(),
            detected_gpu,
            detected_driver: None,
            detected_cuda: None,
        }
    }
}

/// Loose NVIDIA-style version comparison: 12.10 is newer than 12.9.
fn version_at_least(found: &str, required: &str) -> bool {
    fn parts(value: &str) -> Vec<u64> {
        value
            .split(['.', '-', ' '])
            .filter_map(|part| {
                let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            })
            .collect()
    }
    let found = parts(found);
    let required = parts(required);
    if required.is_empty() {
        return true;
    }
    for index in 0..required.len() {
        let left = found.get(index).copied().unwrap_or(0);
        let right = required[index];
        if left != right {
            return left > right;
        }
    }
    true
}

/// Compare a component with the probe of one node.
///
/// Every finding is a warning. The caller decides whether to continue; the
/// service never turns these into a refusal.
pub fn assess(component: &CudaxComponent, hardware: &HardwareProfile) -> LibraryAssessment {
    let detected_gpu = hardware
        .gpus
        .first()
        .map(|gpu| gpu.name.clone())
        .unwrap_or_default();
    let mut warnings = Vec::new();

    if hardware.gpus.is_empty() {
        warnings.push(
            "No NVIDIA GPU was detected on this node. The component will install, but GPU \
             workloads cannot run until a GPU and driver are present."
                .to_string(),
        );
    }
    if component.requires_gpu && hardware.gpus.is_empty() {
        warnings.push(format!(
            "{} requires a GPU; this node reports none.",
            component.name
        ));
    }
    if let Some(driver) = hardware.driver_version.as_deref() {
        if !component.min_driver.is_empty() && !version_at_least(driver, &component.min_driver) {
            warnings.push(format!(
                "Installed driver {driver} is older than the recommended {} for {}.",
                component.min_driver, component.name
            ));
        }
    } else if !component.min_driver.is_empty() {
        warnings.push(format!(
            "The driver version could not be detected; {} recommends {}.",
            component.name, component.min_driver
        ));
    }
    if let Some(cuda) = hardware.cuda_version.as_deref() {
        if !component.min_cuda.is_empty() && !version_at_least(cuda, &component.min_cuda) {
            warnings.push(format!(
                "Detected CUDA {cuda} is older than {} required by {}.",
                component.min_cuda, component.name
            ));
        }
    }
    if hardware
        .detection_note
        .as_deref()
        .is_some_and(|note| note.to_lowercase().contains("container"))
    {
        warnings.push(
            "The node's container GPU runtime was not confirmed; container-based runtimes may \
             not start."
                .to_string(),
        );
    }

    LibraryAssessment {
        supported: warnings.is_empty(),
        warnings,
        detected_gpu,
        detected_driver: hardware.driver_version.clone(),
        detected_cuda: hardware.cuda_version.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeLibraryInstall {
    pub id: Uuid,
    pub node_id: Uuid,
    pub node_name: String,
    pub component: CudaxComponent,
    pub version: String,
    pub state: CudaxState,
    pub force_install: bool,
    pub assessment: LibraryAssessment,
    pub last_error: Option<String>,
    pub installed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::GpuDevice;

    fn component(requires_gpu: bool, min_driver: &str) -> CudaxComponent {
        CudaxComponent {
            id: Uuid::nil(),
            key: "nccl".to_string(),
            name: "NCCL".to_string(),
            category: "communication".to_string(),
            version: "2".to_string(),
            description: String::new(),
            apt_packages: vec!["libnccl2".to_string()],
            container_images: vec![],
            requires_gpu,
            min_driver: min_driver.to_string(),
            min_cuda: "12.0".to_string(),
            docs_url: String::new(),
        }
    }

    fn hardware(gpus: Vec<GpuDevice>, driver: Option<&str>, cuda: Option<&str>) -> HardwareProfile {
        HardwareProfile {
            id: Uuid::nil(),
            environment_id: Uuid::nil(),
            node_id: Uuid::nil(),
            detected_at: Utc::now(),
            hostname: None,
            total_vram_bytes: gpus.iter().map(|gpu| gpu.memory_bytes).sum(),
            gpus,
            unified_memory_bytes: None,
            unified_memory_source: None,
            cuda_version: cuda.map(str::to_string),
            driver_version: driver.map(str::to_string),
            detection_note: None,
            cpu_model: None,
            cpu_cores: None,
            cpu_threads: None,
            system_ram_bytes: None,
        }
    }

    fn gpu() -> GpuDevice {
        GpuDevice {
            index: 0,
            name: "NVIDIA RTX 6000".to_string(),
            memory_bytes: 48 * 1024 * 1024 * 1024,
            free_bytes: None,
        }
    }

    #[test]
    fn a_node_without_a_gpu_is_warned_but_not_refused() {
        let assessment = assess(&component(true, "525.60"), &hardware(vec![], None, None));
        assert!(!assessment.supported);
        assert!(assessment.warnings.iter().any(|warning| warning.contains("No NVIDIA GPU")));
    }

    #[test]
    fn a_current_driver_and_gpu_is_supported() {
        let assessment = assess(
            &component(true, "525.60"),
            &hardware(vec![gpu()], Some("550.54"), Some("12.4")),
        );
        assert!(assessment.supported, "{:?}", assessment.warnings);
    }

    #[test]
    fn an_old_driver_is_a_warning_with_its_numbers() {
        let assessment = assess(
            &component(true, "535.54"),
            &hardware(vec![gpu()], Some("525.60"), Some("12.0")),
        );
        assert!(!assessment.supported);
        assert!(assessment.warnings.iter().any(|warning| warning.contains("525.60")));
    }

    #[test]
    fn versions_compare_numerically_not_lexically() {
        assert!(version_at_least("12.10", "12.9"));
        assert!(!version_at_least("12.9", "12.10"));
        assert!(version_at_least("535.54.03", "535.54"));
    }
}
