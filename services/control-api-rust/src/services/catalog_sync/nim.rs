//! The NIM endpoint: which models NVIDIA hosts, and nothing else.
//!
//! `integrate.api.nvidia.com/v1/models` answers with ids and an owner. It states
//! no size, no parameter count and no license, so this source never adds a
//! catalogue row and never fills a number: it only sets `nim_available` on rows
//! that already exist, which is what lets the console say "also reachable as a
//! hosted NIM" next to a model the operator could install locally.
//!
//! Matching is deliberately strict. The endpoint writes
//! `meta/llama-3.1-8b-instruct`; Hugging Face writes
//! `meta-llama/Llama-3.1-8B-Instruct`. Both reduce to `llama318binstruct` once
//! the vendor prefix and the punctuation are gone, and that is the comparison.
//! A near match is not enough: a false badge would advertise a hosted endpoint
//! for a model NVIDIA does not serve.

use serde::Deserialize;

use crate::store::postgres::models::ModelIdentity;

use super::{get_json, SourceError};

/// The endpoint that lists the hosted models.
const MODELS_URL: &str = "https://integrate.api.nvidia.com/v1/models";

/// Read the ids NVIDIA hosts.
pub async fn available_ids(client: &reqwest::Client) -> Result<Vec<String>, SourceError> {
    let response: ModelList = get_json(client, MODELS_URL).await?;
    Ok(response
        .data
        .into_iter()
        .map(|model| model.id)
        .filter(|id| !id.is_empty())
        .collect())
}

#[derive(Debug, Deserialize)]
struct ModelList {
    #[serde(default)]
    data: Vec<ListedModel>,
}

#[derive(Debug, Deserialize)]
struct ListedModel {
    #[serde(default)]
    id: String,
}

/// Whether a NIM id names the same model as a catalogue row.
///
/// Both sides are reduced to the last path segment with only lowercase
/// alphanumerics, so `meta/llama-3.1-8b-instruct`, `meta-llama/Llama-3.1-8B-Instruct`
/// and `Llama 3.1 8B Instruct` all compare equal.
pub fn names_the_same_model(nim_id: &str, row: &ModelIdentity) -> bool {
    let wanted = normalize(nim_id);
    if wanted.is_empty() {
        return false;
    }
    normalize(&row.name) == wanted
        || row
            .source_id
            .as_deref()
            .is_some_and(|source_id| normalize(source_id) == wanted)
}

/// The last path segment with only lowercase ASCII alphanumerics.
pub fn normalize(value: &str) -> String {
    let tail = value.rsplit('/').next().unwrap_or(value);
    tail.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(name: &str, source_id: Option<&str>) -> ModelIdentity {
        ModelIdentity {
            key: "some-key".to_string(),
            name: name.to_string(),
            source_id: source_id.map(|id| id.to_string()),
        }
    }

    #[test]
    fn a_vendor_prefix_and_punctuation_do_not_prevent_a_match() {
        let row = identity("Llama 3.1 8B Instruct", Some("meta-llama/Llama-3.1-8B-Instruct"));
        assert!(names_the_same_model("meta/llama-3.1-8b-instruct", &row));
    }

    #[test]
    fn a_nemo_resource_matches_its_hugging_face_sibling() {
        let row = identity(
            "Llama 3.3 70B Instruct",
            Some("meta-llama/Llama-3.3-70B-Instruct"),
        );
        assert!(names_the_same_model("nvidia/llama-3.3-70b-instruct", &row));
    }

    #[test]
    fn a_different_size_is_a_different_model() {
        let row = identity("Llama 3.1 8B Instruct", Some("meta-llama/Llama-3.1-8B-Instruct"));
        assert!(!names_the_same_model("meta/llama-3.1-70b-instruct", &row));
    }

    #[test]
    fn a_lookalike_is_not_a_match() {
        let row = identity("Qwen3 8B", Some("Qwen/Qwen3-8B"));
        assert!(!names_the_same_model("qwen/qwen3-8b-instruct", &row));
        assert!(!names_the_same_model("qwen/qwen3-4b", &row));
    }

    #[test]
    fn an_empty_id_matches_nothing() {
        let row = identity("Qwen3 8B", Some("Qwen/Qwen3-8B"));
        assert!(!names_the_same_model("", &row));
        assert!(!names_the_same_model("///", &row));
    }

    #[test]
    fn the_ollama_tag_matches_the_hosted_id() {
        let row = identity("qwen3:8b", Some("qwen3:8b"));
        assert!(names_the_same_model("qwen/qwen3-8b", &row));
    }
}
