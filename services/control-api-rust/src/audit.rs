//! Audit-Log: wer hat was getan.
//!
//! Events liegen im State und werden über `/v1/events` gelesen. Sie enthalten
//! bewusst keine Secrets.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Ein einzelnes Audit-Ereignis.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuditEvent {
    pub id: String,
    /// ISO-8601 in UTC.
    pub time: String,
    /// z. B. `sandbox.created`, `provider.created`.
    pub kind: String,
    /// ID oder Name des betroffenen Objekts.
    pub subject: String,
    /// Freie Zusatzdaten, immer ohne Secrets.
    #[serde(default)]
    pub detail: serde_json::Value,
}

impl AuditEvent {
    /// Baut ein Event mit frischer ID und Zeitstempel.
    pub fn new(kind: &str, subject: &str, detail: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            time: chrono::Utc::now().to_rfc3339(),
            kind: kind.to_string(),
            subject: subject.to_string(),
            detail,
        }
    }
}
