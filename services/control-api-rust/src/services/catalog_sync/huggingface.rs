//! Hugging Face: the most popular text-generation repositories, filtered to the
//! publishers this catalogue trusts.
//!
//! Three requests per model, and each answers a different question:
//!
//! * `/api/models/{id}?blobs=true` — the parameter count (`safetensors.total`),
//!   the weight files with their exact sizes, and the license from `cardData`;
//! * `/raw/main/config.json` — the context length and, for a mixture of
//!   experts, the expert count. A gated or private repository answers 401 here,
//!   which is why that request may fail without failing the model;
//! * the list request — which repositories exist and how popular they are.
//!
//! The allowlist is a publisher list, not a model list: it keeps the catalogue
//! to organisations whose artefacts are consistently published with sizes and
//! licenses, and it drops the very long tail of anonymous re-uploads that would
//! otherwise fill the top downloads.
//!
//! A repository that publishes both safetensors and GGUF files carries the same
//! weights twice, so the safetensors sum wins: subtracting one would describe
//! the model at half its size.

use std::time::Instant;

use serde::Deserialize;

use super::derive::ArtifactKind;
use super::{get_json, Candidate, Fetched, SourceError, SOURCE_BUDGET};

/// The publishers the catalogue takes models from.
pub const ALLOWED_PUBLISHERS: &[&str] = &[
    "deepseek-ai",
    "Qwen",
    "meta-llama",
    "google",
    "microsoft",
    "mistralai",
    "nvidia",
    "moonshotai",
    "zai-org",
    "MiniMaxAI",
    "openai",
    "unsloth",
    "bartowski",
];

/// How many repositories are read at once. Small enough not to look like a
/// scraper to the API, large enough that a sync of forty models is not forty
/// round trips in a row.
const CONCURRENCY: usize = 4;

/// Whether a repository belongs to a publisher on the allowlist.
///
/// The comparison ignores case because the organisation name is matched as an
/// identifier, and `Qwen` and `qwen` are the same account.
pub fn is_allowed(repo_id: &str) -> bool {
    let Some((publisher, _)) = repo_id.split_once('/') else {
        return false;
    };
    ALLOWED_PUBLISHERS
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(publisher))
}

/// Read the popular text-generation repositories this catalogue accepts.
pub async fn fetch(
    client: &reqwest::Client,
    limit: usize,
) -> Result<Fetched, SourceError> {
    let started = Instant::now();
    let url = format!(
        "https://huggingface.co/api/models\
         ?filter=text-generation&sort=downloads&direction=-1&limit={limit}"
    );
    let listed: Vec<Listed> = get_json(client, &url).await?;

    let selected: Vec<String> = listed
        .into_iter()
        .map(|entry| entry.id)
        .filter(|id| is_allowed(id))
        .collect();

    let mut fetched = Fetched::default();
    if selected.is_empty() {
        fetched.note = Some("no repository on the publisher allowlist was listed".to_string());
    }

    for chunk in selected.chunks(CONCURRENCY) {
        if started.elapsed() > SOURCE_BUDGET {
            fetched.note = Some(format!(
                "stopped after {SOURCE_BUDGET:?}; the rest of the list was not read"
            ));
            break;
        }
        let described =
            futures::future::join_all(chunk.iter().map(|id| describe(client, id))).await;
        for outcome in described {
            match outcome {
                Ok(candidate) => fetched.candidates.push(candidate),
                // A single repository that cannot be read — gated, removed
                // between list and detail, or malformed — is a count, not the
                // end of the source.
                Err(_) => fetched.failed += 1,
            }
        }
    }

    Ok(fetched)
}

/// Read one repository: the detail document and its config.
async fn describe(client: &reqwest::Client, repo_id: &str) -> Result<Candidate, SourceError> {
    let detail_url = format!("https://huggingface.co/api/models/{repo_id}?blobs=true");
    let repository: Repository = get_json(client, &detail_url).await?;

    // The config is display metadata (context, experts) and may be gated. Its
    // absence costs a field, not the row.
    let config_url = format!("https://huggingface.co/{repo_id}/raw/main/config.json");
    let config: Option<ModelConfig> = get_json(client, &config_url).await.ok();

    let (weights_bytes, artifact) = weights(&repository.siblings);
    let name = repository
        .id
        .split_once('/')
        .map(|(_, name)| name)
        .unwrap_or(&repository.id)
        .to_string();

    Ok(Candidate {
        source: "huggingface",
        source_id: repository.id.clone(),
        name,
        publisher: repository
            .author
            .clone()
            .or_else(|| repository.id.split_once('/').map(|(org, _)| org.to_string())),
        family_hint: None,
        license: license_of(repository.card_data.as_ref()),
        description: None,
        parameters: repository
            .safetensors
            .as_ref()
            .and_then(|safetensors| safetensors.total),
        expert_count: config.as_ref().and_then(ModelConfig::expert_count),
        context_length: config
            .as_ref()
            .and_then(|config| config.max_position_embeddings),
        quantization: dominant_dtype(repository.safetensors.as_ref()),
        weights_bytes,
        artifact,
        arch: None,
        source_uri: Some(format!("https://huggingface.co/{}", repository.id)),
        version: version_of(&repository),
    })
}

/// The row the list endpoint returns; only the id is needed from it.
#[derive(Debug, Deserialize)]
struct Listed {
    id: String,
}

/// A repository as `/api/models/{id}` describes it.
#[derive(Debug, Deserialize)]
struct Repository {
    id: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default, rename = "lastModified")]
    last_modified: Option<String>,
    #[serde(default)]
    sha: Option<String>,
    #[serde(default, rename = "cardData")]
    card_data: Option<CardData>,
    #[serde(default)]
    safetensors: Option<Safetensors>,
    #[serde(default)]
    siblings: Vec<Sibling>,
}

#[derive(Debug, Deserialize)]
struct CardData {
    #[serde(default)]
    license: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Safetensors {
    #[serde(default)]
    total: Option<i64>,
    /// Parameters per dtype, e.g. `{"BF16": 751632384}`. The dominant entry is
    /// the precision the publisher shipped.
    #[serde(default)]
    parameters: Option<std::collections::BTreeMap<String, i64>>,
}

#[derive(Debug, Deserialize)]
struct Sibling {
    rfilename: String,
    #[serde(default)]
    size: Option<i64>,
    #[serde(default)]
    lfs: Option<Lfs>,
}

#[derive(Debug, Deserialize)]
struct Lfs {
    #[serde(default)]
    size: Option<i64>,
}

/// The parts of `config.json` that matter here.
#[derive(Debug, Deserialize)]
struct ModelConfig {
    #[serde(default)]
    max_position_embeddings: Option<i64>,
    /// DeepSeek and Qwen mixture-of-experts configs.
    #[serde(default)]
    n_routed_experts: Option<i64>,
    #[serde(default)]
    num_experts: Option<i64>,
    /// Mixtral and Llama-4 configs.
    #[serde(default)]
    num_local_experts: Option<i64>,
}

impl ModelConfig {
    /// The expert count under whichever key the architecture uses.
    fn expert_count(&self) -> Option<i64> {
        self.n_routed_experts
            .or(self.num_experts)
            .or(self.num_local_experts)
            .filter(|count| *count > 1)
    }
}

/// What the repository's weight files weigh, and what kind they are.
///
/// Safetensors shards are parts of one model, so they are summed. GGUF files are
/// usually complete alternatives at different quantizations, so the largest
/// complete set is taken instead of the sum: adding `Q4_K_M` to `Q8_0` would
/// describe a model that does not exist. A GGUF split into `-00001-of-00003`
/// shards is one file in parts and is summed within its set.
fn weights(siblings: &[Sibling]) -> (Option<i64>, ArtifactKind) {
    let safetensors: i64 = siblings
        .iter()
        .filter(|sibling| sibling.rfilename.ends_with(".safetensors"))
        .filter_map(blob_size)
        .sum();
    if safetensors > 0 {
        return (Some(safetensors), ArtifactKind::Safetensors);
    }

    let gguf = largest_gguf_set(siblings);
    match gguf {
        0 => (None, ArtifactKind::Safetensors),
        bytes => (Some(bytes), ArtifactKind::Gguf),
    }
}

/// The size of one file, preferring the LFS entry because that is the stored
/// size rather than the size of the pointer.
fn blob_size(sibling: &Sibling) -> Option<i64> {
    sibling
        .lfs
        .as_ref()
        .and_then(|lfs| lfs.size)
        .or(sibling.size)
        .filter(|size| *size > 0)
}

/// The largest complete GGUF set, summing files that are shards of one set.
fn largest_gguf_set(siblings: &[Sibling]) -> i64 {
    let mut sets: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for sibling in siblings {
        if !sibling.rfilename.ends_with(".gguf") {
            continue;
        }
        let Some(size) = blob_size(sibling) else {
            continue;
        };
        // `...-00001-of-00003.gguf` belongs to the set named before it; every
        // other file is its own set.
        *sets.entry(gguf_set(&sibling.rfilename)).or_insert(0) += size;
    }
    sets.values().copied().max().unwrap_or(0)
}

/// The set a GGUF file belongs to: a split file drops its shard index, so the
/// parts of one quant add up and two quants stay apart.
fn gguf_set(filename: &str) -> String {
    let Some((left, _)) = filename.rsplit_once("-of-") else {
        return filename.to_string();
    };
    let Some((set, shard)) = left.rsplit_once('-') else {
        return filename.to_string();
    };
    if !shard.is_empty() && shard.chars().all(|character| character.is_ascii_digit()) {
        set.to_string()
    } else {
        filename.to_string()
    }
}

/// The license from the model card, which is a string or, for multi-license
/// releases, a list.
fn license_of(card: Option<&CardData>) -> Option<String> {
    let value = card?.license.as_ref()?;
    match value {
        serde_json::Value::String(license) => Some(license.clone()),
        serde_json::Value::Array(licenses) => licenses
            .iter()
            .filter_map(|license| license.as_str())
            .next()
            .map(|license| license.to_string()),
        _ => None,
    }
}

/// The precision with the most parameters, e.g. `BF16` or `F8_E4M3`.
fn dominant_dtype(safetensors: Option<&Safetensors>) -> Option<String> {
    let parameters = safetensors?.parameters.as_ref()?;
    parameters
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(dtype, _)| dtype.clone())
}

/// The revision this row describes: the commit, so a new release is an update
/// rather than a silent change to an installed model.
fn version_of(repository: &Repository) -> String {
    if let Some(sha) = repository.sha.as_deref().filter(|sha| sha.len() >= 12) {
        return sha[..12].to_string();
    }
    if let Some(modified) = repository.last_modified.as_deref() {
        return modified.to_string();
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sibling(name: &str, size: i64) -> Sibling {
        Sibling {
            rfilename: name.to_string(),
            size: Some(size),
            lfs: None,
        }
    }

    #[test]
    fn only_allowlisted_publishers_are_taken() {
        assert!(is_allowed("Qwen/Qwen3-8B"));
        assert!(is_allowed("meta-llama/Llama-3.1-8B-Instruct"));
        assert!(is_allowed("bartowski/Qwen3-8B-GGUF"));
        assert!(!is_allowed("somebody/Qwen3-8B-finetune"));
        assert!(!is_allowed("gpt2"));
    }

    #[test]
    fn the_allowlist_is_a_publisher_list_not_a_substring_match() {
        // A lookalike organisation must not slip through the comparison.
        assert!(!is_allowed("Qwen-Evil/Qwen3-8B"));
        assert!(!is_allowed("notQwen/Qwen3-8B"));
    }

    #[test]
    fn safetensors_shards_are_summed() {
        let (bytes, artifact) = weights(&[
            sibling("model-00001-of-00002.safetensors", 4_000),
            sibling("model-00002-of-00002.safetensors", 6_000),
            sibling("config.json", 900),
        ]);
        assert_eq!(bytes, Some(10_000));
        assert_eq!(artifact, ArtifactKind::Safetensors);
    }

    #[test]
    fn the_lfs_size_wins_over_the_pointer_size() {
        let sharded = Sibling {
            rfilename: "model.safetensors".to_string(),
            size: Some(134),
            lfs: Some(Lfs { size: Some(5_000) }),
        };
        assert_eq!(blob_size(&sharded), Some(5_000));
    }

    #[test]
    fn several_gguf_quants_are_alternatives_not_parts() {
        // Two complete quants of the same model must not add up to a model that
        // does not exist; the larger, complete one describes it.
        let (bytes, artifact) = weights(&[
            sibling("Qwen3-8B-Q4_K_M.gguf", 5_000),
            sibling("Qwen3-8B-Q8_0.gguf", 9_000),
        ]);
        assert_eq!(bytes, Some(9_000));
        assert_eq!(artifact, ArtifactKind::Gguf);
    }

    #[test]
    fn a_split_gguf_is_summed_within_its_set() {
        let (bytes, _) = weights(&[
            sibling("big-00001-of-00002.gguf", 4_000),
            sibling("big-00002-of-00002.gguf", 6_000),
        ]);
        assert_eq!(bytes, Some(10_000));
    }

    #[test]
    fn a_split_quant_and_a_whole_quant_stay_apart() {
        // Two quants, one of them split: the set that completes is the bigger
        // one, and its shards are the only files added together.
        let (bytes, _) = weights(&[
            sibling("model.Q4_K_M-00001-of-00002.gguf", 3_000),
            sibling("model.Q4_K_M-00002-of-00002.gguf", 3_000),
            sibling("model.Q8_0.gguf", 9_000),
        ]);
        assert_eq!(bytes, Some(9_000));
    }

    #[test]
    fn a_repository_with_no_weight_files_states_no_size() {
        let (bytes, _) = weights(&[sibling("README.md", 900)]);
        assert!(bytes.is_none());
    }

    #[test]
    fn the_license_is_read_from_the_card() {
        let card = CardData {
            license: Some(serde_json::json!("apache-2.0")),
        };
        assert_eq!(license_of(Some(&card)).as_deref(), Some("apache-2.0"));

        let multiple = CardData {
            license: Some(serde_json::json!(["mit", "apache-2.0"])),
        };
        assert_eq!(license_of(Some(&multiple)).as_deref(), Some("mit"));

        let missing = CardData { license: None };
        assert!(license_of(Some(&missing)).is_none());
    }

    #[test]
    fn the_dominant_precision_is_the_one_carried() {
        let safetensors = Safetensors {
            total: Some(1_000),
            parameters: Some(
                [("F8_E4M3".to_string(), 900), ("BF16".to_string(), 100)]
                    .into_iter()
                    .collect(),
            ),
        };
        assert_eq!(dominant_dtype(Some(&safetensors)).as_deref(), Some("F8_E4M3"));
    }

    #[test]
    fn the_expert_count_comes_from_whichever_key_the_config_uses() {
        for config in [
            ModelConfig {
                max_position_embeddings: None,
                n_routed_experts: Some(384),
                num_experts: None,
                num_local_experts: None,
            },
            ModelConfig {
                max_position_embeddings: None,
                n_routed_experts: None,
                num_experts: Some(384),
                num_local_experts: None,
            },
            ModelConfig {
                max_position_embeddings: None,
                n_routed_experts: None,
                num_experts: None,
                num_local_experts: Some(384),
            },
        ] {
            assert_eq!(config.expert_count(), Some(384));
        }
        // A dense model states no experts, and "1 expert" is not a mixture.
        let dense = ModelConfig {
            max_position_embeddings: Some(40960),
            n_routed_experts: None,
            num_experts: Some(1),
            num_local_experts: None,
        };
        assert_eq!(dense.expert_count(), None);
    }

    #[test]
    fn the_version_is_the_commit() {
        let repository = Repository {
            id: "Qwen/Qwen3-8B".to_string(),
            author: None,
            last_modified: Some("2025-07-26T03:46:27.000Z".to_string()),
            sha: Some("c1899de289a04d12100db370d81485cdf75e47ca".to_string()),
            card_data: None,
            safetensors: None,
            siblings: Vec::new(),
        };
        assert_eq!(version_of(&repository), "c1899de289a0");
    }
}
