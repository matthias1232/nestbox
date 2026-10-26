//! Einheitliches Fehlerformat der API.
//!
//! Jeder Fehler wird zu genau einem JSON-Envelope:
//!
//! ```json
//! {"error": {"code": "sandbox.not_found", "message": "…", "request_id": "…"}}
//! ```
//!
//! `code` ist stabil und für Clients gedacht, `message` ist für Menschen.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

/// Fehler, die eine API-Operation zurückgeben kann.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{resource} '{id}' wurde nicht gefunden")]
    NotFound { resource: &'static str, id: String },

    #[error("{resource} '{name}' existiert bereits")]
    Conflict { resource: &'static str, name: String },

    #[error("{0}")]
    Invalid(String),

    #[error("Runtime nicht erreichbar: {0}")]
    Runtime(String),

    #[error("Interner Fehler: {0}")]
    Internal(String),
}

impl ApiError {
    /// Stabiler, maschinenlesbarer Fehlercode.
    pub fn code(&self) -> String {
        match self {
            Self::NotFound { resource, .. } => format!("{resource}.not_found"),
            Self::Conflict { resource, .. } => format!("{resource}.conflict"),
            Self::Invalid(_) => "request.invalid".to_string(),
            Self::Runtime(_) => "runtime.unavailable".to_string(),
            Self::Internal(_) => "internal.error".to_string(),
        }
    }

    /// HTTP-Status zum Fehler.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Runtime(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Körper einer Fehlerantwort.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

/// Umschlag um den Fehlerkörper.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.code(),
                message: self.to_string(),
            },
        };
        (self.status(), Json(body)).into_response()
    }
}

/// Bequeme Umwandlung: ein `Result` mit `String`-Fehler wird zu `Internal`.
impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self::Internal(error.to_string())
    }
}

/// Bequeme Umwandlung für Dateisystemfehler.
impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        Self::Internal(error.to_string())
    }
}
