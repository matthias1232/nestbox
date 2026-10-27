//! The Ollama registry: one GGUF file with exact layer sizes and a stated
//! parameter class.
//!
//! The registry publishes no listing of the library, so the probes are a fixed
//! list of model and tag pairs. That is a deliberate limitation: a listing that
//! does not exist cannot be read, and guessing one would mean probing thousands
//! of tag names on every sync.
//!
//! Two requests per model:
//!
//! * the manifest, which names the layers and their exact byte sizes — the model
//!   layer is the GGUF file and therefore the number the gate is derived from;
//! * the config blob, which states the parameter class (`8.2B`), the file type
//!   (`Q4_K_M`) and the model family (`qwen3`).
//!
//! The blob endpoint answers with a redirect to object storage; the HTTP client
//! follows it, so a caller sees one request.

use std::time::Instant;

use serde::Deserialize;

use super::derive::ArtifactKind;
use super::{get_json_accepting, Candidate, Fetched, SourceError, SOURCE_BUDGET};

/// The manifest media type the registry should answer with.
const MANIFEST_ACCEPT: &str = "application/vnd.docker.distribution.manifest.v2+json";

/// The layer that holds the weights.
const MODEL_LAYER: &str = "application/vnd.ollama.image.model";

/// The library models the sync probes, as `model:tag`.
///
/// Every entry is a published tag; a tag that disappears upstream costs one
/// failed probe in the job log and nothing else.
pub const PROBES: &[&str] = &[
    "llama3.1:8b",
    "llama3.2:3b",
    "llama3.3:70b",
    "qwen3:8b",
    "qwen3:32b",
    "qwen2.5-coder:7b",
    "qwen2.5:14b",
    "gemma3:4b",
    "gemma3:12b",
    "deepseek-r1:8b",
    "deepseek-r1:32b",
    "mistral:7b",
    "phi4:14b",
    "codellama:13b",
    "nemotron:70b",
    "granite3.3:8b",
];

/// How many probes are read at once.
const CONCURRENCY: usize = 4;

/// Read the probed library models.
pub async fn fetch(
    client: &reqwest::Client,
    limit: usize,
) -> Result<Fetched, SourceError> {
    let started = Instant::now();
    let mut fetched = Fetched::default();
    let probes: Vec<&str> = PROBES.iter().copied().take(limit).collect();

    for chunk in probes.chunks(CONCURRENCY) {
        if started.elapsed() > SOURCE_BUDGET {
            fetched.note = Some(format!(
                "stopped after {SOURCE_BUDGET:?}; the rest of the probes were not read"
            ));
            break;
        }
        let probed =
            futures::future::join_all(chunk.iter().map(|probe| describe(client, probe))).await;
        for outcome in probed {
            match outcome {
                Ok(candidate) => fetched.candidates.push(candidate),
                Err(_) => fetched.failed += 1,
            }
        }
    }

    if fetched.candidates.is_empty() && fetched.failed == 0 {
        fetched.note = Some("no library tag was read".to_string());
    }
    Ok(fetched)
}

/// Read one model and tag.
async fn describe(client: &reqwest::Client, probe: &str) -> Result<Candidate, SourceError> {
    let (model, tag) = probe
        .split_once(':')
        .ok_or_else(|| SourceError::new(format!("probe '{probe}' is not model:tag")))?;

    let manifest_url =
        format!("https://registry.ollama.ai/v2/library/{model}/manifests/{tag}");
    let manifest: Manifest =
        get_json_accepting(client, &manifest_url, Some(MANIFEST_ACCEPT)).await?;

    let weights: i64 = manifest
        .layers
        .iter()
        .filter(|layer| layer.media_type == MODEL_LAYER)
        .map(|layer| layer.size)
        .sum();

    let config_url = format!(
        "https://registry.ollama.ai/v2/library/{model}/blobs/{}",
        manifest.config.digest
    );
    let config: Option<Config> = get_json_accepting(client, &config_url, None).await.ok();

    Ok(Candidate {
        source: "ollama",
        source_id: probe.to_string(),
        name: probe.to_string(),
        // The registry states a model family, not a publisher; the family is the
        // honest grouping key and the publisher stays absent.
        publisher: None,
        family_hint: config.as_ref().and_then(|config| config.model_family.clone()),
        license: None,
        description: None,
        parameters: config
            .as_ref()
            .and_then(|config| config.model_type.as_deref())
            .and_then(parse_parameter_label),
        expert_count: None,
        context_length: None,
        quantization: config.as_ref().and_then(|config| config.file_type.clone()),
        weights_bytes: (weights > 0).then_some(weights),
        artifact: ArtifactKind::Gguf,
        arch: Some("gguf".to_string()),
        source_uri: Some(format!("https://ollama.com/library/{probe}")),
        // The manifest digest changes when the tag is re-pushed, which is
        // exactly the event that should read as an update.
        version: short_digest(&manifest.config.digest),
    })
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    config: Descriptor,
    #[serde(default)]
    layers: Vec<Layer>,
}

#[derive(Debug, Default, Deserialize)]
struct Descriptor {
    #[serde(default)]
    digest: String,
}

#[derive(Debug, Deserialize)]
struct Layer {
    #[serde(default, rename = "mediaType")]
    media_type: String,
    #[serde(default)]
    size: i64,
}

/// The config blob as the registry stores it.
#[derive(Debug, Deserialize)]
struct Config {
    /// For example `qwen3`.
    #[serde(default)]
    model_family: Option<String>,
    /// The parameter class, for example `8.2B`.
    #[serde(default)]
    model_type: Option<String>,
    /// The quantization, for example `Q4_K_M`.
    #[serde(default)]
    file_type: Option<String>,
}

/// The first twelve hex characters of a digest, which is what a version column
/// holds.
fn short_digest(digest: &str) -> String {
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
    match hex.get(..12) {
        Some(short) => short.to_string(),
        None => "unknown".to_string(),
    }
}

/// A parameter class as the registry states it: `8.2B`, `70B`, `1.5B`, `7b`.
///
/// The multiplier is the one the label spells out. A label this parser does not
/// understand yields `None`, which keeps the column honest instead of guessing
/// from the tag.
pub fn parse_parameter_label(label: &str) -> Option<i64> {
    let label = label.trim();
    let (number, multiplier) = match label.chars().last()?.to_ascii_uppercase() {
        'B' => (&label[..label.len() - 1], 1_000_000_000_f64),
        'M' => (&label[..label.len() - 1], 1_000_000_f64),
        'K' => (&label[..label.len() - 1], 1_000_f64),
        'T' => (&label[..label.len() - 1], 1_000_000_000_000_f64),
        _ => return None,
    };
    let value = number.trim().parse::<f64>().ok()?;
    if !(value.is_finite() && value > 0.0) {
        return None;
    }
    Some((value * multiplier).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parameter_class_becomes_a_count() {
        assert_eq!(parse_parameter_label("8.2B"), Some(8_200_000_000));
        assert_eq!(parse_parameter_label("70B"), Some(70_000_000_000));
        assert_eq!(parse_parameter_label("1.5b"), Some(1_500_000_000));
        assert_eq!(parse_parameter_label("270M"), Some(270_000_000));
    }

    #[test]
    fn a_label_that_states_no_unit_is_not_guessed() {
        assert_eq!(parse_parameter_label("8"), None);
        assert_eq!(parse_parameter_label(""), None);
        assert_eq!(parse_parameter_label("latest"), None);
        assert_eq!(parse_parameter_label("B"), None);
    }

    #[test]
    fn the_version_is_the_manifest_digest() {
        assert_eq!(
            short_digest("sha256:05a61d37b08453e59290add468e3bb2f688e23a01e967fecb0e2fa41218cea76"),
            "05a61d37b084"
        );
        assert_eq!(short_digest("abc"), "unknown");
    }

    #[test]
    fn the_model_layer_is_the_only_weight() {
        // The template, license and params layers are not weights, and counting
        // them would inflate the gate by a few kilobytes.
        let manifest = Manifest {
            config: Descriptor::default(),
            layers: vec![
                Layer {
                    media_type: MODEL_LAYER.to_string(),
                    size: 5_225_374_496,
                },
                Layer {
                    media_type: "application/vnd.ollama.image.license".to_string(),
                    size: 11_338,
                },
            ],
        };
        let weights: i64 = manifest
            .layers
            .iter()
            .filter(|layer| layer.media_type == MODEL_LAYER)
            .map(|layer| layer.size)
            .sum();
        assert_eq!(weights, 5_225_374_496);
    }
}
