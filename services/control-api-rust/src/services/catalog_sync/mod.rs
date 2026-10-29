//! The catalog sync: build the catalogue from the repositories that publish the
//! models, and never over a curated number.
//!
//! The catalogue has two kinds of rows. A `pinned` row is written by hand (the
//! seed migrations) and carries figures somebody checked against the model card;
//! the sync never changes it. An unpinned row is what this module adds and keeps
//! current: a name, a size, a license and the gates derived from the size.
//!
//! Three repositories are read, each in its own module because each states a
//! different subset of the truth:
//!
//! * Hugging Face — parameters, weight files, license, context length;
//! * the Ollama registry — one GGUF quant with its exact layer sizes;
//! * the NGC catalogue — NVIDIA's packaged models with their artefact sizes.
//!
//! A fourth source, the NIM endpoint, states only which models NVIDIA serves.
//! It never adds a row and never invents a size; it sets `nim_available` on rows
//! that already exist.
//!
//! Rules this module keeps:
//!
//! * one failing source never aborts the others — its error goes into the job
//!   log and the other sources still run;
//! * every HTTP call has a timeout, and a source stops starting new work after
//!   its budget instead of holding the job open indefinitely;
//! * a row is never deleted. A model that disappears upstream keeps its row and
//!   its last `synced_at`, so the console does not silently lose history;
//! * a row without a stated size is written with null gates, and the classifier
//!   reports it as unknown rather than installable (ADR 0019, `derive`);
//! * the row is keyed by `(source, source_id)`, so a rename upstream updates the
//!   existing row instead of creating a second one.
//!
//! Who runs it: [`start`] is the manual path behind `POST /models/sync`; it
//! creates the `catalog_sync` job and runs it in the same request, so the caller
//! gets the counts rather than a job id that nothing executes. [`run`] is the
//! function the scheduler calls for the daily `catalog-sync` schedule, with the
//! job it just created. One implementation, two triggers.

pub mod derive;
pub mod huggingface;
pub mod ngc;
pub mod nim;
pub mod ollama;

use std::time::Duration;

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::job::JobKind;
use crate::error::ApiError;
use crate::store::TenantScope;
use crate::store::postgres::jobs;
use crate::store::postgres::models::{self as store, SyncedModel, Upserted};

/// How long any single HTTP request may take.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// How long one source may spend before it stops starting new requests.
///
/// The per-request timeout bounds one call; this bounds the source, so a
/// repository that answers slowly cannot turn a sync into an hour of waiting.
pub(crate) const SOURCE_BUDGET: Duration = Duration::from_secs(60);

/// How many repositories the Hugging Face list is asked for. The allowlist then
/// drops most of them, so the number fetched is larger than the number added.
const HUGGINGFACE_LIMIT: usize = 40;

/// How many Ollama tags are probed. The registry publishes no listing, so the
/// probes are a fixed list of library models in `ollama.rs`.
const OLLAMA_LIMIT: usize = 16;

/// How many NGC resources are kept after filtering.
const NGC_LIMIT: usize = 24;

/// What a source states about one model, before the gate is derived.
///
/// A `Candidate` is intentionally not a database row: the source does not know
/// about gates, keys or embeddings, and the row shape should not grow a field
/// every time a new repository is added.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The catalogue source, one of the values the `models.source` check allows.
    pub source: &'static str,
    /// The identifier the source itself uses, unique within that source.
    pub source_id: String,
    /// A display name, as the source publishes it.
    pub name: String,
    /// The publisher, as the source names it.
    pub publisher: Option<String>,
    /// The model family, when the artefact names one. Ollama's registry does;
    /// Hugging Face and NGC carry only the publisher.
    pub family_hint: Option<String>,
    pub license: Option<String>,
    pub description: Option<String>,
    /// Parameters, when the artefact states a count.
    pub parameters: Option<i64>,
    /// Experts, when the artefact states a count; only used for the label.
    pub expert_count: Option<i64>,
    pub context_length: Option<i64>,
    pub quantization: Option<String>,
    /// What the published files weigh, when the source states it.
    pub weights_bytes: Option<i64>,
    pub artifact: derive::ArtifactKind,
    /// The runtime the source itself names (NGC says `TENSOR_RT_PLAN`), when it
    /// names one. The file kind answers this otherwise.
    pub arch: Option<String>,
    pub source_uri: Option<String>,
    /// The revision the row describes, so a new release becomes an update.
    pub version: String,
}

/// What one source produced, including what it could not read.
#[derive(Debug, Default)]
pub struct Fetched {
    pub candidates: Vec<Candidate>,
    /// Repositories that were listed but could not be read. Counted rather than
    /// fatal: one gated repository must not lose the other thirty.
    pub failed: usize,
    /// Why the source stopped early, if it did.
    pub note: Option<String>,
}

/// A source that could not be read at all.
#[derive(Debug, Clone)]
pub struct SourceError {
    pub message: String,
}

impl SourceError {
    /// Build an error with a message safe to put in the job log.
    ///
    /// The message names the request, never a credential: none of these
    /// endpoints takes one, and the job log is shown to operators.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// The counts one source contributed, which is what the job log reports.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceReport {
    pub source: String,
    pub added: i64,
    pub updated: i64,
    /// Rows the source offered that were not written: no stated size, or a key
    /// another row already owns.
    pub skipped: i64,
    /// Rows left alone because they are curated by hand.
    pub pinned: i64,
    /// Rows that could not be written, or repositories that could not be read.
    pub failed: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl SourceReport {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            added: 0,
            updated: 0,
            skipped: 0,
            pinned: 0,
            failed: 0,
            error: None,
            note: None,
        }
    }

    /// The one line this source writes into the job log.
    ///
    /// The counts come first and in a fixed order so an operator (and a test)
    /// can read a source's outcome without parsing prose.
    fn log_line(&self) -> String {
        let mut line = format!(
            "{}: added {}, updated {}, skipped {}, pinned {}, failed {}",
            self.source, self.added, self.updated, self.skipped, self.pinned, self.failed
        );
        if let Some(error) = &self.error {
            line.push_str(&format!(" ({error})"));
        }
        if let Some(note) = &self.note {
            line.push_str(&format!(" [{note}]"));
        }
        line
    }
}

/// The result of one sync, which is both the API answer and the job's result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncSummary {
    pub job_id: Uuid,
    pub sources: Vec<SourceReport>,
    /// Rows the NIM endpoint confirmed, which is a fact about a remote listing
    /// and not a catalogue addition.
    pub nim_rows: i64,
}

/// Run a manual sync: create the job, then run it.
///
/// The job is created before the work starts so a failure halfway is still
/// attributable to a job id, and it runs inside the request so the operator sees
/// the counts. A scheduled run calls [`run`] with the job its schedule created.
pub async fn start(ctx: &ServiceContext, principal: &Principal) -> Result<SyncSummary, ApiError> {
    require(principal, Permission::ModelInstall)?;

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let job = jobs::create(
        &mut scope,
        JobKind::CatalogSync,
        &[],
        &serde_json::json!({ "trigger": "manual" }),
        None,
        Some(principal.user_id),
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    run(ctx, principal.environment_id, job.id).await
}

/// Execute a `catalog_sync` job that already exists.
///
/// This is the one implementation: the manual endpoint reaches it through
/// [`start`], and the scheduled runner reaches it with the job the
/// `catalog-sync` schedule produced. Sources run in a fixed order so the log
/// reads the same way every day.
pub async fn run(
    ctx: &ServiceContext,
    environment_id: Uuid,
    job_id: Uuid,
) -> Result<SyncSummary, ApiError> {
    let client = build_client()?;
    begin_job(ctx, environment_id, job_id).await?;

    let mut summary = SyncSummary {
        job_id,
        sources: Vec::new(),
        nim_rows: 0,
    };

    summary.sources.push(
        sync_source(
            ctx,
            environment_id,
            job_id,
            "huggingface",
            huggingface::fetch(&client, HUGGINGFACE_LIMIT).await,
        )
        .await,
    );
    summary.sources.push(
        sync_source(
            ctx,
            environment_id,
            job_id,
            "ollama",
            ollama::fetch(&client, OLLAMA_LIMIT).await,
        )
        .await,
    );
    summary.sources.push(
        sync_source(
            ctx,
            environment_id,
            job_id,
            "ngc",
            ngc::fetch(&client, NGC_LIMIT).await,
        )
        .await,
    );

    let (nim_rows, nim_note) = mark_nim_available(ctx, environment_id, &client).await;
    summary.nim_rows = nim_rows;
    let mut nim_line = format!("nim: {nim_rows} catalogue rows are available through NVIDIA NIM");
    if let Some(note) = &nim_note {
        nim_line.push_str(&format!(" [{note}]"));
    }
    job_log(
        ctx,
        environment_id,
        job_id,
        if nim_note.is_some() { "warn" } else { "info" },
        &nim_line,
    )
    .await?;

    finish_job(ctx, environment_id, job_id, &summary).await?;
    Ok(summary)
}

/// The HTTP client every source shares.
///
/// The timeout is on the client, so no call site can forget it.
fn build_client() -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent("nestbox-catalog-sync/1.0")
        .build()
        .map_err(|error| {
            ApiError::internal("The catalog sync could not create an HTTP client.")
                .with_internal(error.to_string())
        })
}

/// Read JSON from a public endpoint, with the sync's timeout.
///
/// Every call site in this module goes through here, so the timeout cannot be
/// forgotten at one of them. The error names the request and the outcome; none
/// of these endpoints takes a credential, and the job log is shown to operators.
pub(crate) async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, SourceError> {
    get_json_accepting(client, url, None).await
}

/// [`get_json`] with an `Accept` header, which the Ollama registry needs to
/// select a manifest media type rather than an image index.
pub(crate) async fn get_json_accepting<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    accept: Option<&str>,
) -> Result<T, SourceError> {
    let mut request = client.get(url).timeout(REQUEST_TIMEOUT);
    if let Some(accept) = accept {
        request = request.header(reqwest::header::ACCEPT, accept);
    }
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            SourceError::new(format!("{url}: timed out after {REQUEST_TIMEOUT:?}"))
        } else {
            SourceError::new(format!("{url}: {error}"))
        }
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(SourceError::new(format!("{url}: HTTP {status}")));
    }

    response
        .json::<T>()
        .await
        .map_err(|error| SourceError::new(format!("{url}: {error}")))
}

/// Write everything one source offered, and report the counts.
async fn sync_source(
    ctx: &ServiceContext,
    environment_id: Uuid,
    job_id: Uuid,
    source: &'static str,
    fetched: Result<Fetched, SourceError>,
) -> SourceReport {
    let mut report = SourceReport::new(source);

    let fetched = match fetched {
        Ok(fetched) => fetched,
        Err(error) => {
            report.error = Some(error.message);
            report.failed = 1;
            let _ = job_log(
                ctx,
                environment_id,
                job_id,
                "error",
                &report.log_line(),
            )
            .await;
            return report;
        }
    };

    report.failed = fetched.failed as i64;
    report.note = fetched.note;

    for candidate in &fetched.candidates {
        // A repository that states no size still yields a row: it is written
        // with null gates, and `classify` reports `unknown` rather than reading
        // the missing requirement as zero (ADR 0019).
        let row = row_for(candidate);

        match write_row(ctx, environment_id, &row).await {
            Ok(Upserted::Inserted) => report.added += 1,
            Ok(Upserted::Updated) => report.updated += 1,
            Ok(Upserted::SkippedPinned) => report.pinned += 1,
            Ok(Upserted::SkippedKeyTaken) => report.skipped += 1,
            Err(error) => {
                report.failed += 1;
                tracing::warn!(
                    source,
                    source_id = candidate.source_id,
                    error = %error.message,
                    "catalog sync could not write a row"
                );
            }
        }
    }

    let level = if report.failed > 0 { "warn" } else { "info" };
    let _ = job_log(ctx, environment_id, job_id, level, &report.log_line()).await;
    report
}

/// A write failure for one row, kept per row so the rest of the source lands.
struct RowError {
    message: String,
}

/// Upsert one row in its own transaction.
async fn write_row(
    ctx: &ServiceContext,
    environment_id: Uuid,
    row: &SyncedModel,
) -> Result<Upserted, RowError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(|error| RowError {
            message: error.to_string(),
        })?;
    let outcome = store::upsert_synced(&mut scope, row)
        .await
        .map_err(|error| RowError {
            message: error.to_string(),
        })?;
    scope.commit().await.map_err(|error| RowError {
        message: error.to_string(),
    })?;
    Ok(outcome)
}

/// Turn what a source stated into the row it may write.
///
/// A source that states no size yields null gates rather than a refusal: the row
/// is still worth listing, and `classify` reports it as `unknown` instead of
/// installable (ADR 0019). The gate figures themselves are never estimated.
fn row_for(candidate: &Candidate) -> SyncedModel {
    let requirements = derive::requirements(candidate.weights_bytes, candidate.artifact);
    let gate = requirements.gate;
    let artifact = artifact_label(candidate.artifact);
    let family = candidate
        .family_hint
        .clone()
        .or_else(|| candidate.publisher.clone())
        .unwrap_or_else(|| candidate.source.to_string())
        .to_lowercase();

    SyncedModel {
        source: candidate.source.to_string(),
        source_id: candidate.source_id.clone(),
        key: key_for(candidate.source, &candidate.source_id),
        name: candidate.name.clone(),
        // No repository publishes a "family" for every model, so the stated
        // family label wins, then the publisher, then the source. All three are
        // stated; none is inferred from the name.
        family,
        parameters: parameters_text(candidate.parameters, candidate.expert_count),
        context_length: candidate.context_length,
        quantization: candidate.quantization.clone(),
        weights_bytes: candidate.weights_bytes,
        min_vram_bytes: gate.map(|gate| gate.min_vram_bytes),
        recommended_vram_bytes: gate.map(|gate| gate.recommended_vram_bytes),
        min_gpu_count: gate.map_or(1, |gate| gate.min_gpu_count),
        min_ram_bytes: requirements.min_ram_bytes,
        recommended_ram_bytes: requirements.recommended_ram_bytes,
        min_cpu_cores: None,
        recommended_cpu_cores: None,
        runtime: requirements.runtime.as_str().to_string(),
        arch: Some(candidate.arch.clone().unwrap_or_else(|| artifact.clone())),
        // The artefact's precision label where the source states one, otherwise
        // the file kind, which is all that is known.
        artifact_format: Some(candidate.quantization.clone().unwrap_or(artifact)),
        publisher: candidate.publisher.clone(),
        license: candidate.license.clone(),
        description: candidate.description.clone(),
        unified_memory_only: gate.is_some_and(|gate| gate.unified_memory_only),
        is_cpu_only: false,
        source_uri: candidate.source_uri.clone(),
        version: candidate.version.clone(),
    }
}

/// The label a file kind is stored under in `arch` and `artifact_format`.
fn artifact_label(artifact: derive::ArtifactKind) -> String {
    match artifact {
        derive::ArtifactKind::Safetensors => "safetensors",
        derive::ArtifactKind::Gguf => "gguf",
    }
    .to_string()
}

/// A stable catalogue key from a source and its own identifier.
///
/// The key is visible in URLs (`/models/{key}`) and in the pack catalog, so it
/// is lowercase ASCII with single dashes: `huggingface-qwen-qwen3-0-6b`. It is
/// derived, never random, so a second sync of the same model finds the same row.
pub fn key_for(source: &str, source_id: &str) -> String {
    let mut key = String::with_capacity(source.len() + source_id.len() + 2);
    key.push_str(source);
    key.push('-');
    let mut last_was_dash = true;
    for character in source_id.chars() {
        if character.is_ascii_alphanumeric() {
            key.push(character.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            key.push('-');
            last_was_dash = true;
        }
    }
    while key.ends_with('-') {
        key.pop();
    }
    key
}

/// The parameters column: the count the artefact states, or `unknown`.
///
/// `unknown` is deliberately not a number. A guessed parameter count would be
/// indistinguishable from a stated one once it is in the catalogue.
pub fn parameters_text(parameters: Option<i64>, experts: Option<i64>) -> String {
    let Some(parameters) = parameters.filter(|count| *count > 0) else {
        return "unknown".to_string();
    };
    let count = human_parameters(parameters);
    match experts.filter(|count| *count > 1) {
        Some(experts) => format!("{count} MoE ({experts} experts)"),
        None => count,
    }
}

/// A parameter count as a model card writes it: `0.75B`, `8B`, `763B`, `1.6T`.
fn human_parameters(parameters: i64) -> String {
    const MILLION: f64 = 1_000_000.0;
    const BILLION: f64 = 1_000_000_000.0;
    const TRILLION: f64 = 1_000_000_000_000.0;
    let value = parameters as f64;
    if value >= TRILLION {
        format!("{}T", trimmed(value / TRILLION, 2))
    } else if value >= 10.0 * BILLION {
        format!("{}B", trimmed(value / BILLION, 0))
    } else if value >= 100.0 * MILLION {
        // A sub-billion model is written as a fraction of a billion on every
        // model card (`0.75B`, not `751M`).
        format!("{}B", trimmed(value / BILLION, 2))
    } else if value >= MILLION {
        format!("{}M", trimmed(value / MILLION, 0))
    } else {
        parameters.to_string()
    }
}

/// Format with at most `precision` decimals and no trailing zeros.
fn trimmed(value: f64, precision: usize) -> String {
    let mut text = format!("{value:.precision$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

/// Mark the rows NVIDIA's NIM endpoint serves.
///
/// The endpoint lists ids and nothing else, so this step sets one boolean and
/// states no size. It writes `nim_available` (a fact about a remote listing, not
/// a curated figure) and leaves every number and every `updated_at` alone.
async fn mark_nim_available(
    ctx: &ServiceContext,
    environment_id: Uuid,
    client: &reqwest::Client,
) -> (i64, Option<String>) {
    let ids = match nim::available_ids(client).await {
        Ok(ids) => ids,
        Err(error) => return (0, Some(error.message)),
    };

    let mut scope = match TenantScope::begin(&ctx.pool, environment_id).await {
        Ok(scope) => scope,
        Err(error) => return (0, Some(error.to_string())),
    };

    let identities = match store::identities(&mut scope).await {
        Ok(identities) => identities,
        Err(error) => return (0, Some(error.to_string())),
    };

    let keys: Vec<String> = identities
        .iter()
        .filter(|row| ids.iter().any(|id| nim::names_the_same_model(id, row)))
        .map(|row| row.key.clone())
        .collect();

    let marked = match store::set_nim_available(&mut scope, &keys).await {
        Ok(marked) => marked,
        Err(error) => return (0, Some(error.to_string())),
    };
    if let Err(error) = scope.commit().await {
        return (0, Some(error.to_string()));
    }
    (marked as i64, None)
}

/// Put the job into `running` and say so in its log.
async fn begin_job(
    ctx: &ServiceContext,
    environment_id: Uuid,
    job_id: Uuid,
) -> Result<(), ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    jobs::start(&mut scope, job_id).await.map_err(ApiError::from)?;
    jobs::log(&mut scope, job_id, "info", "catalog sync started")
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)
}

/// Finish the job with the counts as its result.
async fn finish_job(
    ctx: &ServiceContext,
    environment_id: Uuid,
    job_id: Uuid,
    summary: &SyncSummary,
) -> Result<(), ApiError> {
    let result = serde_json::to_value(summary).unwrap_or(serde_json::Value::Null);
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    jobs::finish(&mut scope, job_id, Some(&result))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)
}

/// Append one line to the job log.
async fn job_log(
    ctx: &ServiceContext,
    environment_id: Uuid,
    job_id: Uuid,
    level: &str,
    message: &str,
) -> Result<(), ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    jobs::log(&mut scope, job_id, level, message)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_stable_and_url_safe() {
        assert_eq!(
            key_for("huggingface", "Qwen/Qwen3-0.6B"),
            "huggingface-qwen-qwen3-0-6b"
        );
        assert_eq!(
            key_for("huggingface", "Qwen/Qwen3-0.6B"),
            key_for("huggingface", "Qwen/Qwen3-0.6B")
        );
        assert_eq!(key_for("ollama", "qwen3:8b"), "ollama-qwen3-8b");
    }

    #[test]
    fn punctuation_collapses_into_single_dashes() {
        assert_eq!(key_for("ngc", "nvidia/nemo/llama-3_3-70b"), "ngc-nvidia-nemo-llama-3-3-70b");
        assert_eq!(key_for("ngc", "a//b"), "ngc-a-b");
        assert!(!key_for("ngc", "///").ends_with('-'));
    }

    #[test]
    fn a_stated_parameter_count_is_readable() {
        assert_eq!(parameters_text(Some(751_632_384), None), "0.75B");
        assert_eq!(parameters_text(Some(8_030_261_248), None), "8.03B");
        assert_eq!(parameters_text(Some(763_000_000_000), None), "763B");
        assert_eq!(parameters_text(Some(1_600_000_000_000), None), "1.6T");
    }

    #[test]
    fn an_expert_count_labels_a_mixture() {
        assert_eq!(
            parameters_text(Some(763_000_000_000), Some(384)),
            "763B MoE (384 experts)"
        );
    }

    #[test]
    fn a_missing_parameter_count_is_unknown_not_zero() {
        assert_eq!(parameters_text(None, None), "unknown");
        assert_eq!(parameters_text(Some(0), None), "unknown");
    }

    #[test]
    fn a_candidate_without_a_size_is_written_with_no_gate() {
        // Written, not refused: the classifier has an `unknown` verdict for
        // exactly this row, so the catalogue can list the model without
        // claiming it fits anywhere.
        let row = row_for(&candidate(None));
        assert!(row.min_vram_bytes.is_none());
        assert!(row.recommended_vram_bytes.is_none());
        assert!(row.min_ram_bytes.is_none());
        assert_eq!(row.min_gpu_count, 1);
        assert!(!row.unified_memory_only);
        assert_eq!(row.parameters, "8B");
    }

    #[test]
    fn a_candidate_with_a_size_gets_its_gate_from_that_size() {
        let candidate = candidate(Some(8 * 1024 * 1024 * 1024));
        let row = row_for(&candidate);
        assert_eq!(row.min_vram_bytes, Some(14 * 1024 * 1024 * 1024));
        assert_eq!(row.runtime, "cuda");
        assert_eq!(row.source, "huggingface");
        assert_eq!(row.key, "huggingface-qwen-qwen3-8b");
        assert_eq!(row.parameters, "8B");
    }

    #[test]
    fn the_log_line_states_the_counts_in_a_fixed_order() {
        let mut report = SourceReport::new("huggingface");
        report.added = 3;
        report.updated = 2;
        report.skipped = 1;
        report.pinned = 4;
        report.failed = 1;
        assert_eq!(
            report.log_line(),
            "huggingface: added 3, updated 2, skipped 1, pinned 4, failed 1"
        );
    }

    fn candidate(weights_bytes: Option<i64>) -> Candidate {
        Candidate {
            source: "huggingface",
            source_id: "Qwen/Qwen3-8B".to_string(),
            name: "Qwen3-8B".to_string(),
            publisher: Some("Qwen".to_string()),
            family_hint: None,
            license: Some("apache-2.0".to_string()),
            description: None,
            parameters: Some(8_000_000_000),
            expert_count: None,
            context_length: Some(40960),
            quantization: Some("BF16".to_string()),
            weights_bytes,
            artifact: derive::ArtifactKind::Safetensors,
            arch: None,
            source_uri: Some("https://huggingface.co/Qwen/Qwen3-8B".to_string()),
            version: "abc123".to_string(),
        }
    }
}
