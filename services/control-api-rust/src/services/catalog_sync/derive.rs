//! The requirement rule from ADR 0013, applied to what a source states.
//!
//! The catalogue is data, and every number in it has to be attributable. A
//! synced row therefore gets its gate from the one figure the repository
//! publishes: the size of the artefact.
//!
//! ADR 0013 derives weights from a parameter count when a human writes a row,
//! because a human has no files to weigh. A sync does: the repository states the
//! file sizes, and inverting the formula (parameters × an assumed dtype) would
//! turn a label into a measurement. A parameter count therefore never produces a
//! gate here — a stated size does, and the count is carried as the display
//! label it is.
//!
//! The rule, as the seed migrations state it:
//!
//! * `min_vram = weights + overhead`, where the overhead is 10 % above 100 GiB
//!   and between 6 and 16 GiB below it (KV cache plus the runtime's own
//!   residency);
//! * `recommended_vram = 1.25 x min_vram`, rounded up to the next GiB;
//! * `min_gpu_count = ceil(min_vram / 80 GiB)`; above one card the model is
//!   also flagged `unified_memory_only`, because a single pool can hold it while
//!   a discrete card cannot;
//! * a CPU-readable artefact (GGUF) also gets a RAM gate — the file plus ~2 GiB
//!   of working set, since llama.cpp maps the file rather than copying it. A
//!   safetensors row has no CPU path, so it states no RAM figure.
//!
//! With no size there is no gate. A row without a gate would be classified as
//! "needs zero bytes" and read as installable on every machine, which is the
//! failure the catalogue exists to prevent; `gate_for` returns `None` and the
//! sync refuses to write the row.

use crate::domain::model::ModelRuntime;

const GIB: i64 = 1024 * 1024 * 1024;

/// The card a multi-GPU count is expressed in.
///
/// 80 GiB is the largest single device the catalogue targets (H100/A100 80 GB,
/// GB200 per-die pool), so a model that needs more than that needs more than one
/// device. It is a divisor, not a measurement.
pub const REFERENCE_CARD_BYTES: i64 = 80 * GIB;

/// Fixed runtime and KV overhead below the proportional regime.
const MIN_OVERHEAD_BYTES: i64 = 6 * GIB;

/// Upper end of the fixed overhead; the clamp keeps a small model from being
/// given a percentage so small it would not survive a real context window.
const MAX_OVERHEAD_BYTES: i64 = 16 * GIB;

/// Weights above this get a proportional overhead instead of a fixed one.
const PROPORTIONAL_ABOVE_BYTES: i64 = 100 * GIB;

/// Working set a mapped GGUF file needs on the CPU side.
const CPU_WORKING_SET_BYTES: i64 = 2 * GIB;

/// The kind of file a source publishes, which decides the runtime and whether a
/// CPU gate exists at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    /// Full-precision or quantized weights that only a GPU runtime loads.
    Safetensors,
    /// A GGUF file llama.cpp and Ollama map directly, on a GPU or on the CPU.
    Gguf,
}

/// The memory gate a row carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gate {
    pub min_vram_bytes: i64,
    pub recommended_vram_bytes: i64,
    pub min_gpu_count: i32,
    pub unified_memory_only: bool,
}

/// Everything the rule produces for one artefact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requirements {
    pub runtime: ModelRuntime,
    pub gate: Option<Gate>,
    pub min_ram_bytes: Option<i64>,
    pub recommended_ram_bytes: Option<i64>,
}

/// Derive the gates from a stated artefact size.
///
/// `weights_bytes` is what the repository says the files weigh; `artifact`
/// decides the runtime and whether the CPU path exists. A missing or
/// meaningless size yields no gate rather than a zero one.
pub fn requirements(weights_bytes: Option<i64>, artifact: ArtifactKind) -> Requirements {
    let runtime = match artifact {
        ArtifactKind::Safetensors => ModelRuntime::Cuda,
        ArtifactKind::Gguf => ModelRuntime::Hybrid,
    };

    let Some(gate) = gate_for(weights_bytes) else {
        return Requirements {
            runtime,
            gate: None,
            min_ram_bytes: None,
            recommended_ram_bytes: None,
        };
    };

    let (min_ram_bytes, recommended_ram_bytes) = match artifact {
        ArtifactKind::Gguf => {
            let weights = weights_bytes.unwrap_or(0);
            let min_ram = round_up_to_gib(weights.saturating_add(CPU_WORKING_SET_BYTES));
            (Some(min_ram), Some(recommended(min_ram)))
        }
        ArtifactKind::Safetensors => (None, None),
    };

    Requirements {
        runtime,
        gate: Some(gate),
        min_ram_bytes,
        recommended_ram_bytes,
    }
}

/// The VRAM gate for a stated artefact size, or `None` when no size is stated.
pub fn gate_for(weights_bytes: Option<i64>) -> Option<Gate> {
    let weights = weights_bytes.filter(|bytes| *bytes > 0)?;

    let overhead = if weights >= PROPORTIONAL_ABOVE_BYTES {
        weights / 10
    } else {
        (weights / 10).clamp(MIN_OVERHEAD_BYTES, MAX_OVERHEAD_BYTES)
    };
    let min_vram = round_up_to_gib(weights.saturating_add(overhead));
    let min_gpu_count = ceil_div(min_vram, REFERENCE_CARD_BYTES)
        .clamp(1, i64::from(i32::MAX)) as i32;

    Some(Gate {
        min_vram_bytes: min_vram,
        recommended_vram_bytes: recommended(min_vram),
        min_gpu_count,
        // A model that does not fit one card is what a unified pool is for; the
        // card count stays as the discrete alternative.
        unified_memory_only: min_gpu_count > 1,
    })
}

/// 1.25 x the minimum, rounded up to the next GiB.
fn recommended(minimum: i64) -> i64 {
    round_up_to_gib(minimum.saturating_mul(5) / 4)
}

/// Round up to a whole GiB, so a catalogue figure reads like a requirement
/// rather than like a file size.
fn round_up_to_gib(bytes: i64) -> i64 {
    if bytes <= 0 {
        return 0;
    }
    ceil_div(bytes, GIB) * GIB
}

/// Ceiling division for non-negative values.
fn ceil_div(value: i64, divisor: i64) -> i64 {
    if divisor <= 0 {
        return 0;
    }
    value / divisor + i64::from(value % divisor != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: i64 = 1024 * 1024 * 1024;

    #[test]
    fn a_small_artefact_gets_the_fixed_overhead() {
        let gate = gate_for(Some(8 * GIB)).expect("a stated size yields a gate");
        assert_eq!(gate.min_vram_bytes, 14 * GIB);
        assert_eq!(gate.recommended_vram_bytes, 18 * GIB);
        assert_eq!(gate.min_gpu_count, 1);
        assert!(!gate.unified_memory_only);
    }

    #[test]
    fn a_tiny_artefact_still_gets_the_floor_rather_than_a_percentage() {
        // 0.75 B parameters at BF16 is 1.4 GiB; 10 % of that is nothing next to
        // a KV cache, so the fixed floor applies.
        let gate = gate_for(Some(GIB + GIB / 2)).expect("gate");
        assert_eq!(gate.min_vram_bytes, 8 * GIB);
    }

    #[test]
    fn the_overhead_becomes_proportional_above_a_hundred_gib() {
        let hundred = gate_for(Some(100 * GIB)).expect("gate");
        assert_eq!(hundred.min_vram_bytes, 110 * GIB);

        let two_hundred = gate_for(Some(200 * GIB)).expect("gate");
        assert_eq!(two_hundred.min_vram_bytes, 220 * GIB);
        // 220 / 80 = 2.75 -> three cards.
        assert_eq!(two_hundred.min_gpu_count, 3);
        assert_eq!(two_hundred.recommended_vram_bytes, 275 * GIB);
    }

    #[test]
    fn a_model_above_one_card_is_flagged_for_a_unified_pool() {
        let gate = gate_for(Some(160 * GIB)).expect("gate");
        assert_eq!(gate.min_vram_bytes, 176 * GIB);
        assert!(gate.unified_memory_only);
        assert_eq!(gate.min_gpu_count, 3);
    }

    #[test]
    fn a_model_that_fits_one_card_is_not_flagged() {
        let gate = gate_for(Some(60 * GIB)).expect("gate");
        assert_eq!(gate.min_vram_bytes, 66 * GIB);
        assert!(!gate.unified_memory_only);
        assert_eq!(gate.min_gpu_count, 1);
    }

    #[test]
    fn the_recommendation_is_a_quarter_above_the_minimum_rounded_up() {
        // 14 GiB * 1.25 = 17.5 GiB, which is not a requirement anybody can
        // configure, so it rounds to 18.
        let gate = gate_for(Some(8 * GIB)).expect("gate");
        assert_eq!(gate.recommended_vram_bytes, 18 * GIB);
        assert!(gate.recommended_vram_bytes > gate.min_vram_bytes);
    }

    #[test]
    fn no_stated_size_means_no_gate_at_all() {
        // The classifier treats a missing gate as "needs nothing", so a row
        // without a size must not reach it at all.
        assert!(gate_for(None).is_none());
        assert!(gate_for(Some(0)).is_none());
        assert!(gate_for(Some(-1)).is_none());
    }

    #[test]
    fn a_gguf_states_both_a_gpu_and_a_cpu_gate() {
        let req = requirements(Some(5 * GIB), ArtifactKind::Gguf);
        assert_eq!(req.runtime, ModelRuntime::Hybrid);
        let gate = req.gate.expect("gate");
        assert_eq!(gate.min_vram_bytes, 11 * GIB);
        // llama.cpp maps the file and needs a small working set beside it.
        assert_eq!(req.min_ram_bytes, Some(7 * GIB));
        assert_eq!(req.recommended_ram_bytes, Some(9 * GIB));
    }

    #[test]
    fn a_safetensors_artefact_states_no_ram_figure() {
        // There is no CPU path for it, and a RAM number would only invite the
        // reader to try one.
        let req = requirements(Some(8 * GIB), ArtifactKind::Safetensors);
        assert_eq!(req.runtime, ModelRuntime::Cuda);
        assert!(req.min_ram_bytes.is_none());
        assert!(req.recommended_ram_bytes.is_none());
    }

    #[test]
    fn a_row_without_a_size_states_neither_runtime_gate() {
        let req = requirements(None, ArtifactKind::Gguf);
        assert!(req.gate.is_none());
        assert!(req.min_ram_bytes.is_none());
        assert!(req.recommended_ram_bytes.is_none());
    }
}
