//! Ist-Zustand einer Sandbox, wie ihn der Runtime-Adapter meldet.
//!
//! Diese Typen sind bewusst getrennt vom Soll-Zustand (`Sandbox`): der
//! Controller speichert Wünsche, der Adapter liefert Fakten.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Netzwerkschnittstelle einer Sandbox.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct NetworkInterface {
    pub name: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub gateway: String,
}

/// Ressourcenlimits, die der Runtime tatsächlich gesetzt hat.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct RuntimeResources {
    #[serde(default)]
    pub memory_limit: u64,
    #[serde(default)]
    pub nano_cpus: u64,
    #[serde(default)]
    pub pids_limit: i64,
}

/// Live-Fakten zu einer Sandbox.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct RuntimeFacts {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub container_name: String,
    pub status: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub pack_key: String,
    #[serde(default)]
    pub tool_profile: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub gateway: String,
    #[serde(default)]
    pub network: Vec<NetworkInterface>,
    #[serde(default)]
    pub uptime_seconds: u64,
    #[serde(default)]
    pub health: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub resources: RuntimeResources,
}

/// Wunsch zum Erzeugen einer Sandbox – die Sprache des Adapters.
#[derive(Debug, Clone)]
pub struct SandboxSpec {
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub pack_key: String,
    pub tool_profile: String,
    pub memory_limit: String,
    pub cpu_limit: f64,
    pub pids_limit: i64,
    /// Bind mounts. Sources come from the persistent-root table, never from a
    /// request; the runner validates them against its persistent base again.
    pub mounts: Vec<crate::domain::persist::MountSpec>,
    /// GPUs to hand to the container. Zero on a node without `container_gpu`.
    pub gpu_count: u32,
}

/// What the runner found on its own host, as raw text the control plane parses.
///
/// Raw rather than parsed on purpose: the parsers are tested Rust, and a
/// second implementation in the runner would be a second opinion.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct HardwareReport {
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub cpuinfo: String,
    #[serde(default)]
    pub meminfo: String,
    /// `nvidia-smi --query-gpu=index,name,memory.total,memory.free --format=csv,noheader,nounits`
    #[serde(default)]
    pub nvidia_smi_csv: Option<String>,
    /// The plain `nvidia-smi` header, for the CUDA version.
    #[serde(default)]
    pub nvidia_smi_header: Option<String>,
    /// Whether the Docker daemon has an NVIDIA runtime registered.
    #[serde(default)]
    pub container_gpu: bool,
    #[serde(default)]
    pub note: Option<String>,
}

/// A request to build an image from a Dockerfile.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BuildRequest {
    /// `nestbox-recipe/<slug>:<hash>`
    pub tag: String,
    pub dockerfile: String,
    /// Extra files by relative name, e.g. the entrypoint script.
    #[serde(default)]
    pub files: Vec<(String, String)>,
}

/// What a build produced.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct BuildResult {
    pub tag: String,
    #[serde(default)]
    pub image_id: Option<String>,
    #[serde(default)]
    pub log: String,
    #[serde(default)]
    pub success: bool,
}

/// What a command run inside a sandbox produced.
///
/// `truncated` is a separate flag rather than a marker inside the text, so a
/// caller can tell "the output ended" from "the output was cut" without parsing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct ExecResult {
    pub exit_code: Option<i64>,
    pub output: String,
    #[serde(default)]
    pub truncated: bool,
}

/// Convert an operator-facing memory limit into the form Docker accepts.
///
/// Operators write `2Gi` because that is what Kubernetes and every cloud console
/// uses. Docker accepts only `b`, `k`, `m`, `g` suffixes, and rejects `2Gi` with
/// "should specify the units" — a message that does not mention the real
/// problem. Normalising here means the console can keep the familiar notation
/// and the runtime gets something it understands.
///
/// Returns bytes as `<n>b`, which is unambiguous and cannot be misread as
/// decimal megabytes.
pub fn normalize_memory_limit(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("A memory limit is required.".to_string());
    }

    let lowered = trimmed.to_ascii_lowercase();
    let (number, unit) = split_quantity(&lowered);

    let multiplier = match memory_multiplier(unit) {
        Some(multiplier) => multiplier,
        None => {
            return Err(format!(
                "Unknown memory unit '{unit}'. Use b, k, m, g, or ki, mi, gi."
            ))
        }
    };

    let quantity: u64 = number
        .parse()
        .map_err(|_| format!("'{number}' is not a whole number of memory units."))?;
    if quantity == 0 {
        return Err("A memory limit must be greater than zero.".to_string());
    }

    let bytes = quantity
        .checked_mul(multiplier)
        .ok_or_else(|| "The memory limit is too large.".to_string())?;

    // 1 TiB is far beyond what this appliance runs; refusing it catches a typo
    // like `2t` where `2g` was meant, before it reaches the runtime.
    if bytes > 1_099_511_627_776 {
        return Err("A memory limit above 1 TiB is not accepted.".to_string());
    }

    Ok(format!("{bytes}b"))
}

/// The number of bytes one unit stands for.
///
/// One table for reading a limit and for answering "how big is that", so a
/// sandbox's request and the budget it is checked against cannot disagree about
/// what `2Gi` means.
fn memory_multiplier(unit: &str) -> Option<u64> {
    Some(match unit {
        "" | "b" => 1,
        "k" | "kb" => 1_000,
        "ki" | "kib" => 1_024,
        "m" | "mb" => 1_000_000,
        "mi" | "mib" => 1_048_576,
        "g" | "gb" => 1_000_000_000,
        "gi" | "gib" => 1_073_741_824,
        "t" | "tb" => 1_000_000_000_000,
        "ti" | "tib" => 1_099_511_627_776,
        _ => return None,
    })
}

/// The size a memory limit names, in bytes.
///
/// The same parsing as `normalize_memory_limit`, because a request has to be
/// compared against a node's budget before it is accepted, and comparing a
/// string with a byte count is how "32Gi fits in 7.8 GiB" gets written.
pub fn memory_bytes(value: &str) -> Result<i64, String> {
    let lowered = value.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return Err("A memory limit is required.".to_string());
    }
    let (number, unit) = split_quantity(&lowered);
    let multiplier = memory_multiplier(unit)
        .ok_or_else(|| format!("Unknown memory unit '{unit}'. Use b, k, m, g, or ki, mi, gi."))?;
    let quantity: i64 = number
        .parse()
        .map_err(|_| format!("'{number}' is not a whole number of memory units."))?;
    quantity
        .checked_mul(multiplier as i64)
        .ok_or_else(|| "The memory limit is too large.".to_string())
}

/// Split `2gi` into (`2`, `gi`). A trailing non-digit run is the unit.
fn split_quantity(value: &str) -> (&str, &str) {
    let split = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    (&value[..split], &value[split..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kubernetes_units_become_bytes() {
        assert_eq!(normalize_memory_limit("2Gi").as_deref(), Ok("2147483648b"));
        assert_eq!(normalize_memory_limit("512Mi").as_deref(), Ok("536870912b"));
        assert_eq!(normalize_memory_limit("1g").as_deref(), Ok("1000000000b"));
    }

    #[test]
    fn a_limit_has_a_byte_count() {
        assert_eq!(memory_bytes("2Gi"), Ok(2_147_483_648));
        assert_eq!(memory_bytes("512Mi"), Ok(536_870_912));
        assert_eq!(memory_bytes("1g"), Ok(1_000_000_000));
        assert!(memory_bytes("2Xi").is_err());
        assert!(memory_bytes("").is_err());
        // The two readings agree, which is the point of one table.
        let normalized = normalize_memory_limit("3Gi").expect("valid");
        assert_eq!(normalized, format!("{}b", memory_bytes("3Gi").expect("valid")));
    }

    #[test]
    fn a_bare_number_is_bytes() {
        assert_eq!(normalize_memory_limit("1048576").as_deref(), Ok("1048576b"));
    }

    #[test]
    fn nonsense_is_named() {
        assert!(normalize_memory_limit("").is_err());
        assert!(normalize_memory_limit("lots").is_err());
        assert!(normalize_memory_limit("0g").is_err());
        assert!(normalize_memory_limit("2zz").is_err());
    }

    #[test]
    fn an_absurd_value_is_refused_rather_than_sent_to_the_runtime() {
        assert!(normalize_memory_limit("2000g").is_err());
    }
}
