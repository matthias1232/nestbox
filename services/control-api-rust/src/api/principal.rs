//! The authentication extractor.
//!
//! This is the choke point for identity, and it is deliberately the only place
//! that reads a credential. Putting it in one extractor means a new route
//! cannot accidentally skip authentication: a handler that wants a principal
//! asks for this type, and the type does the work.
//!
//! CSRF is enforced here too, for the same reason. A mutating request that
//! arrived with a session cookie must carry a matching CSRF token, and the
//! check belongs with the identity that makes it necessary.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method};

use crate::domain::auth::Principal;
use crate::error::{ApiError, ErrorCode};
use crate::security::session::{CSRF_HEADER, SESSION_COOKIE};
use crate::services::auth_service;
use crate::state::AppState;

/// A request that carried a valid credential.
pub struct Auth {
    pub principal: Principal,
    /// Present for a session request; `None` for a token request.
    pub csrf_hash: Option<String>,
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;

        if let Some(token) = bearer_token(headers) {
            let principal = auth_service::principal_from_token(state.pool(), &token).await?;
            // A token request is not a browser request, so CSRF does not apply:
            // there is no ambient credential for a cross-site form to ride on.
            return Ok(Self {
                principal,
                csrf_hash: None,
            });
        }

        let session_token = cookie_value(headers, SESSION_COOKIE).ok_or_else(|| {
            ApiError::unauthenticated("Authentication is required for this request.")
        })?;

        let (mut principal, csrf_hash) = auth_service::principal_from_session(
            state.pool(),
            &session_token,
            state.config.session_idle_seconds,
        )
        .await?;

        // A browser may name the environment it is working in. It is honoured
        // only if the user is actually a member, which the service checks.
        if let Some(requested) = environment_header(headers) {
            if requested != principal.environment_id {
                principal = auth_service::principal_for_environment(
                    state.pool(),
                    principal.user_id,
                    &principal.display_name,
                    requested,
                    principal.via,
                )
                .await?;
            }
        }

        if is_mutating(&parts.method) {
            let provided = headers
                .get(CSRF_HEADER)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    ApiError::new(
                        axum::http::StatusCode::FORBIDDEN,
                        ErrorCode::CSRF_REQUIRED,
                        "A CSRF token is required for this request.",
                    )
                })?;
            auth_service::verify_csrf(provided, &csrf_hash)?;
        }

        Ok(Self {
            principal,
            csrf_hash: Some(csrf_hash),
        })
    }
}

/// Whether a method can change state, and therefore needs CSRF protection.
fn is_mutating(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// Read a bearer token from either supported header form.
///
/// `Authorization: Bearer` is the modern form; `Token` is what the published
/// contract documents, so both are accepted rather than breaking a client.
fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = raw.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") || scheme.eq_ignore_ascii_case("token") {
        let token = token.trim();
        if token.is_empty() {
            None
        } else {
            Some(token.to_string())
        }
    } else {
        None
    }
}

/// The environment a browser is working in.
fn environment_header(headers: &HeaderMap) -> Option<uuid::Uuid> {
    headers
        .get("x-nestbox-environment")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| uuid::Uuid::parse_str(value.trim()).ok())
}

/// Read one cookie without pulling in a cookie crate.
///
/// The parsing is small and total: a malformed header yields `None` rather than
/// an error, because a browser with a broken cookie should be asked to log in,
/// not shown a 500.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let pair = pair.trim();
        if let Some((key, value)) = pair.split_once('=') {
            if key.trim() == name {
                let value = value.trim();
                if value.is_empty() {
                    return None;
                }
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn a_cookie_is_found_among_several() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_static("theme=dark; nestbox_session=abc123; other=1"),
        );
        assert_eq!(
            cookie_value(&headers, SESSION_COOKIE),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn a_missing_cookie_is_none() {
        let headers = HeaderMap::new();
        assert_eq!(cookie_value(&headers, SESSION_COOKIE), None);
    }

    #[test]
    fn a_malformed_cookie_header_does_not_panic() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_static(";;;=;garbage"),
        );
        assert_eq!(cookie_value(&headers, SESSION_COOKIE), None);
    }

    #[test]
    fn both_token_schemes_are_accepted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Token abc"),
        );
        assert_eq!(bearer_token(&headers), Some("abc".into()));

        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer xyz"),
        );
        assert_eq!(bearer_token(&headers), Some("xyz".into()));
    }

    #[test]
    fn an_unknown_scheme_is_not_treated_as_a_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );
        assert_eq!(bearer_token(&headers), None);
    }

    #[test]
    fn only_state_changing_methods_need_csrf() {
        assert!(is_mutating(&Method::POST));
        assert!(is_mutating(&Method::DELETE));
        assert!(!is_mutating(&Method::GET));
        assert!(!is_mutating(&Method::HEAD));
    }
}

/// Read the session token from a header map, for the logout path.
///
/// Logout must work without a valid session (it is idempotent), so it cannot use
/// the `Auth` extractor, which rejects an absent credential.
pub fn session_token_from(headers: &HeaderMap) -> Option<String> {
    cookie_value(headers, SESSION_COOKIE)
}
