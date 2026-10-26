//! Response headers and CORS.
//!
//! The console renders user-supplied names, destinations, and hostnames. A
//! content security policy is what stops a rendering mistake from becoming code
//! execution in an operator's session, which holds credentials for the whole
//! control plane.
//!
//! Headers are applied by a layer, so they are present on every response
//! including errors and streams. A CSP that appears only on success is not a CSP.

use axum::http::{HeaderName, HeaderValue};
use tower_http::set_header::SetResponseHeaderLayer;

/// The policy the console is written against.
///
/// `'self'` only: no inline script, no inline style, no third-party origin. The
/// console ships its own bundle and its own styles, so a stricter policy costs
/// nothing. `frame-ancestors 'none'` blocks clickjacking; `object-src 'none'`
/// removes a legacy plugin surface.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; \
     script-src 'self'; \
     style-src 'self'; \
     img-src 'self' data:; \
     font-src 'self'; \
     connect-src 'self'; \
     object-src 'none'; \
     base-uri 'none'; \
     form-action 'self'; \
     frame-ancestors 'none'";

/// Build the header layers applied to every response.
///
/// Returns a vector of layers so the caller can attach them in one place rather
/// than remembering each one at each router.
pub fn security_headers() -> Vec<SetResponseHeaderLayer<HeaderValue>> {
    vec![
        header(
            "content-security-policy",
            HeaderValue::from_static(CONTENT_SECURITY_POLICY),
        ),
        header("x-content-type-options", HeaderValue::from_static("nosniff")),
        header("x-frame-options", HeaderValue::from_static("DENY")),
        header("referrer-policy", HeaderValue::from_static("no-referrer")),
        header(
            "permissions-policy",
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ),
        // HSTS is only meaningful over TLS. Sending it on plain HTTP is
        // harmless but misleading, so the caller enables it when TLS is on.
        header(
            "cross-origin-opener-policy",
            HeaderValue::from_static("same-origin"),
        ),
        header(
            "cross-origin-resource-policy",
            HeaderValue::from_static("same-origin"),
        ),
    ]
}

/// The HSTS layer, applied only when the console is served over TLS.
pub fn hsts_header() -> SetResponseHeaderLayer<HeaderValue> {
    header(
        "strict-transport-security",
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    )
}

fn header(name: &'static str, value: HeaderValue) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(HeaderName::from_static(name), value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_denies_inline_script_and_framing() {
        // The two properties that matter most: no inline code, not frameable.
        assert!(CONTENT_SECURITY_POLICY.contains("script-src 'self'"));
        assert!(!CONTENT_SECURITY_POLICY.contains("unsafe-inline"));
        assert!(CONTENT_SECURITY_POLICY.contains("frame-ancestors 'none'"));
        assert!(CONTENT_SECURITY_POLICY.contains("object-src 'none'"));
    }

    #[test]
    fn every_header_name_is_valid() {
        // A malformed header name would panic at layer construction, which is
        // exactly the kind of failure that escapes to production.
        for name in [
            "content-security-policy",
            "x-content-type-options",
            "x-frame-options",
            "referrer-policy",
            "permissions-policy",
            "cross-origin-opener-policy",
            "cross-origin-resource-policy",
            "strict-transport-security",
        ] {
            assert!(
                HeaderName::from_static(name).as_str() == name,
                "{name} is not a valid header name"
            );
        }
    }

    #[test]
    fn security_headers_are_a_non_empty_set() {
        assert!(security_headers().len() >= 5);
    }
}
