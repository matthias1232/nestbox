//! The NGC catalogue: NVIDIA's own packaged models, with their artefact sizes.
//!
//! The search endpoint takes a JSON query and returns everything that matches,
//! including test organisations with names like
//! `0615409268808334/qa-auto-test-team/qa_tgt_model_...`. Unfiltered results are
//! therefore useless, so the response is restricted to `orgName = nvidia` here
//! rather than in the query string: the API's own filter syntax is undocumented
//! and versioned, while the field is in every result.
//!
//! NGC states a size (`latestVersionSizeInBytes`) and a precision label, but no
//! parameter count. That is enough for an honest gate — the gate is derived from
//! bytes — and not enough for the parameters column, which stays `unknown`
//! instead of being parsed out of a model name.
//!
//! The search is fuzzy, so a handful of family queries are used and the results
//! are deduplicated: one resource appears under several queries.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::derive::ArtifactKind;
use super::{Candidate, Fetched, SourceError};

/// The organisation whose models are taken. Everything else in the response is
/// a test artefact or another tenant's private model.
const ORG: &str = "nvidia";

/// The family queries. NGC has no "list all language models" endpoint, and an
/// empty query answers with hundreds of unrelated resources.
const QUERIES: &[&str] = &["llama", "nemotron", "mistral", "qwen", "deepseek", "gemma", "phi"];

/// How many results to ask for per query before the organisation filter.
const PAGE_SIZE: usize = 10;

/// Read the NVIDIA-published models the queries find.
pub async fn fetch(
    client: &reqwest::Client,
    limit: usize,
) -> Result<Fetched, SourceError> {
    let searches =
        futures::future::join_all(QUERIES.iter().map(|query| search(client, query))).await;

    let mut fetched = Fetched::default();
    let mut found: BTreeMap<String, Resource> = BTreeMap::new();
    for outcome in searches {
        match outcome {
            Ok(resources) => {
                for resource in resources {
                    found.entry(resource.resource_id.clone()).or_insert(resource);
                }
            }
            // One query failing is not the source failing: the other families
            // still answer.
            Err(_) => fetched.failed += 1,
        }
    }

    let mut resources: Vec<Resource> = found
        .into_values()
        .filter(|resource| resource.org_is_nvidia())
        .filter(|resource| resource.size_bytes().is_some())
        .collect();

    // The most used first, so a truncated catalogue keeps the models an operator
    // is most likely to look for.
    resources.sort_by(|left, right| {
        right
            .popularity()
            .partial_cmp(&left.popularity())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    resources.truncate(limit);

    for resource in &resources {
        fetched.candidates.push(candidate(resource));
    }

    Ok(fetched)
}

/// One family query.
async fn search(client: &reqwest::Client, query: &str) -> Result<Vec<Resource>, SourceError> {
    let url = "https://api.ngc.nvidia.com/v2/search/catalog/resources/MODEL";
    // `q` is a JSON document, not a string; the client percent-encodes it.
    let response: SearchResponse = client
        .get(url)
        .query(&[
            (
                "q",
                serde_json::json!({ "query": query, "pageSize": PAGE_SIZE }).to_string(),
            ),
            ("pageSize", PAGE_SIZE.to_string()),
        ])
        .timeout(super::REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| SourceError::new(format!("{url}: {error}")))?
        .json::<SearchResponse>()
        .await
        .map_err(|error| SourceError::new(format!("{url}: {error}")))?;

    Ok(response
        .results
        .into_iter()
        .flat_map(|group| group.resources)
        .collect())
}

/// Turn a resource into what the catalogue needs.
fn candidate(resource: &Resource) -> Candidate {
    Candidate {
        source: "ngc",
        source_id: resource.resource_id.clone(),
        name: resource.display_name(),
        publisher: Some(resource.label("publisher").unwrap_or_else(|| "NVIDIA".to_string())),
        family_hint: None,
        license: None,
        description: resource.description.clone(),
        // NGC states a size and a precision, never a parameter count. A count
        // parsed out of a display name ("Llama 3.3 70B") would be a guess about
        // a specific repository, so the column says `unknown`.
        parameters: None,
        expert_count: None,
        context_length: None,
        quantization: resource.precision(),
        weights_bytes: resource.size_bytes(),
        artifact: ArtifactKind::Safetensors,
        arch: resource.arch(),
        source_uri: resource.catalogue_url(),
        version: resource.version(),
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<Group>,
}

#[derive(Debug, Deserialize)]
struct Group {
    #[serde(default)]
    resources: Vec<Resource>,
}

/// One catalogue resource. Only the fields this source reads are declared.
#[derive(Debug, Deserialize)]
struct Resource {
    #[serde(default, rename = "resourceId")]
    resource_id: String,
    #[serde(default, rename = "orgName")]
    org_name: Option<String>,
    #[serde(default, rename = "teamName")]
    team_name: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "displayName")]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    attributes: Vec<Attribute>,
    #[serde(default)]
    labels: Vec<Label>,
    #[serde(default, rename = "weightPopular")]
    weight_popular: Option<f64>,
    #[serde(default, rename = "dateModified")]
    date_modified: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Attribute {
    #[serde(default)]
    key: String,
    #[serde(default)]
    value: String,
}

#[derive(Debug, Deserialize)]
struct Label {
    #[serde(default)]
    key: String,
    #[serde(default)]
    values: Vec<String>,
}

impl Resource {
    /// Whether this is one of NVIDIA's own resources.
    fn org_is_nvidia(&self) -> bool {
        self.org_name
            .as_deref()
            .is_some_and(|org| org.eq_ignore_ascii_case(ORG))
    }

    /// The artefact size, which is the only figure NGC publishes for a model.
    fn size_bytes(&self) -> Option<i64> {
        self.attribute("latestVersionSizeInBytes")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|bytes| *bytes > 0)
    }

    /// A named attribute, e.g. `format`.
    fn attribute(&self, key: &str) -> Option<String> {
        self.attributes
            .iter()
            .find(|attribute| attribute.key == key)
            .map(|attribute| attribute.value.clone())
            .filter(|value| !value.is_empty())
    }

    /// The first value of a named label, e.g. `publisher` or `precision`.
    fn label(&self, key: &str) -> Option<String> {
        self.labels
            .iter()
            .find(|label| label.key == key)
            .and_then(|label| label.values.first())
            .cloned()
            .filter(|value| !value.is_empty())
    }

    /// The precision label, when it names a precision rather than "OTHER".
    fn precision(&self) -> Option<String> {
        let precision = self.label("precision")?;
        let normalised = precision.to_lowercase();
        if matches!(normalised.as_str(), "other" | "n/a" | "na" | "none" | "unknown") {
            return None;
        }
        Some(normalised)
    }

    /// The runtime the artefact needs, from NGC's own `format` field.
    fn arch(&self) -> Option<String> {
        let format = self.attribute("format")?.to_uppercase();
        if format.contains("TENSOR_RT") || format.contains("TRT") {
            Some("tensorrt".to_string())
        } else {
            None
        }
    }

    fn display_name(&self) -> String {
        self.display_name
            .clone()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.name.clone())
    }

    fn popularity(&self) -> f64 {
        self.weight_popular.unwrap_or(0.0)
    }

    fn version(&self) -> String {
        self.attribute("latestVersionIdStr")
            .or_else(|| self.date_modified.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// The public catalogue page, built from the resource id.
    ///
    /// The API groups a resource as `org/team/name`; a resource without a team
    /// has no page this code can name, so no link is stored rather than a
    /// guessed one.
    fn catalogue_url(&self) -> Option<String> {
        let org = self.org_name.as_deref()?;
        let team = self.team_name.as_deref()?;
        let name = self.resource_id.rsplit('/').next()?;
        Some(format!(
            "https://catalog.ngc.nvidia.com/orgs/{org}/teams/{team}/models/{name}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(org: &str, size: Option<&str>, popularity: f64) -> Resource {
        resource_with_format(org, size, popularity, "NEMO")
    }

    fn resource_with_format(
        org: &str,
        size: Option<&str>,
        popularity: f64,
        format: &str,
    ) -> Resource {
        Resource {
            resource_id: format!("{org}/nemo/llama-3_3-70b-instruct"),
            org_name: Some(org.to_string()),
            team_name: Some("nemo".to_string()),
            name: "llama-3_3-70b-instruct".to_string(),
            display_name: Some("Llama 3.3 70B Instruct".to_string()),
            description: None,
            attributes: size
                .map(|size| {
                    vec![
                        Attribute {
                            key: "latestVersionSizeInBytes".to_string(),
                            value: size.to_string(),
                        },
                        Attribute {
                            key: "format".to_string(),
                            value: format.to_string(),
                        },
                    ]
                })
                .unwrap_or_default(),
            labels: vec![Label {
                key: "precision".to_string(),
                values: vec!["BF16".to_string()],
            }],
            weight_popular: Some(popularity),
            date_modified: None,
        }
    }

    #[test]
    fn a_test_organisation_is_not_nvidias_catalogue() {
        assert!(resource("nvidia", Some("141126111923"), 1.0).org_is_nvidia());
        assert!(!resource("0615409268808334", Some("1048576"), 1.0).org_is_nvidia());
    }

    #[test]
    fn the_stated_size_is_the_one_the_gate_uses() {
        let found = resource("nvidia", Some("141126111923"), 1.0);
        assert_eq!(found.size_bytes(), Some(141_126_111_923));
    }

    #[test]
    fn a_resource_without_a_size_states_none() {
        assert!(resource("nvidia", None, 1.0).size_bytes().is_none());
        assert!(resource("nvidia", Some("0"), 1.0).size_bytes().is_none());
        assert!(resource("nvidia", Some("not-a-number"), 1.0)
            .size_bytes()
            .is_none());
    }

    #[test]
    fn the_precision_label_is_lowercased_and_other_is_not_a_precision() {
        assert_eq!(
            resource("nvidia", Some("1"), 1.0).precision().as_deref(),
            Some("bf16")
        );
        let mut other = resource("nvidia", Some("1"), 1.0);
        other.labels = vec![Label {
            key: "precision".to_string(),
            values: vec!["OTHER".to_string()],
        }];
        assert!(other.precision().is_none());
    }

    #[test]
    fn a_tensorrt_plan_names_its_runtime() {
        let plan = resource_with_format("nvidia", Some("6180149388"), 1.0, "TENSOR_RT_PLAN");
        assert_eq!(plan.arch().as_deref(), Some("tensorrt"));
        assert!(resource("nvidia", Some("1"), 1.0).arch().is_none());
    }

    #[test]
    fn a_resource_id_becomes_a_catalogue_link() {
        let found = resource("nvidia", Some("1"), 1.0);
        assert_eq!(
            found.catalogue_url().as_deref(),
            Some("https://catalog.ngc.nvidia.com/orgs/nvidia/teams/nemo/models/llama-3_3-70b-instruct")
        );
    }
}
