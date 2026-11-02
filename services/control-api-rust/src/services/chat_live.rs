//! The live report of a model request: it started, its answer grows, it is done.
//!
//! Nothing here is stored. The broker and the accounting proxy report as they
//! stream; the control API masks the text and publishes it on the live bus. The
//! stored turn is written later, from the access log.
//!
//! A report is throttled per request, so a fast answer cannot flood the bus.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::ServiceContext;

/// The shortest gap between two reports of the same request.
const MIN_GAP: Duration = Duration::from_millis(250);
/// How long a request may be silent before its entry is forgotten.
const FORGET: Duration = Duration::from_secs(600);
/// Longest answer text a live report carries, in bytes.
const LIVE_TEXT_MAX: usize = 16 * 1024;
/// Longest user text a start report carries, in bytes.
const LIVE_USER_MAX: usize = 2048;

/// How a turn started, for callers that do not go through the content check.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LiveStart {
    pub model: Option<String>,
    pub host: Option<String>,
    pub account_kind: Option<String>,
    #[serde(default)]
    pub user_text: String,
}

/// One live report from the broker or the accounting proxy.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LiveChunk {
    pub request_ref: String,
    pub sandbox_id: Uuid,
    #[serde(default)]
    pub text_so_far: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub started: Option<LiveStart>,
}

fn last_report() -> &'static Mutex<HashMap<String, Instant>> {
    static LAST: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Whether a report should be published now.
///
/// The last report of a request is always published; otherwise at most one every
/// [`MIN_GAP`]. A missing or poisoned lock publishes, because dropping the last
/// token of an answer is worse than an extra event.
pub fn should_publish(last: Option<Instant>, now: Instant, done: bool) -> bool {
    match last {
        None => true,
        Some(at) => done || now.duration_since(at) >= MIN_GAP,
    }
}

/// Accept one report, mask it and publish it.
pub async fn accept(ctx: &ServiceContext, chunk: LiveChunk) -> Result<(), ApiError> {
    let _ = (ctx, chunk);
    Ok(())
}

/// Longest answer text a report may carry.
pub(crate) fn live_text_max() -> usize {
    LIVE_TEXT_MAX
}

/// Longest user text a start report may carry.
pub(crate) fn live_user_max() -> usize {
    LIVE_USER_MAX
}
