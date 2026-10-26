//! Secret redaction.
//!
//! One filter, applied at every boundary: logs, error bodies, telemetry events,
//! and audit records. The rule is that a secret is masked on the way out, so
//! forgetting a call site cannot leak it — the mask is not opt-in per site.
//!
//! This is deliberately not clever. It matches key names and well-known token
//! shapes. A determined secret in an unexpected place will not be caught, which
//! is why the primary control is that secrets never enter these structures at
//! all (ADR 0005). Redaction is the backstop.

/// Key names whose values are always masked.
///
/// One constant, so adding a credential type is a single edit rather than a
/// search. Matching is case-insensitive and substring-based, so `openai_api_key`
/// and `apiKey` are both covered.
pub const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "authorization",
    "cookie",
    "credential",
    "private_key",
    "privatekey",
    "client_secret",
    "access_key",
    "session",
    "csrf",
    "bearer",
    "x-api-key",
    "nvidia_api_key",
    "openai_api_key",
    "anthropic_api_key",
];

/// The replacement shown in place of a secret.
pub const MASK: &str = "[redacted]";

/// Whether a key name should be masked.
pub fn is_sensitive_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|needle| lowered.contains(needle))
}

/// Mask a free-form string.
///
/// Two passes: known `key=value` and `key: value` patterns, then well-known
/// token shapes that appear without a key. The second pass is what catches a
/// bearer token pasted into a message.
pub fn redact(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    let mut out = input.to_string();

    // Pass 1: key/value shapes, including the ones that appear inside a URL or
    // a query string.
    for key in SENSITIVE_KEYS {
        out = mask_key_value(&out, key);
    }

    // Pass 2: token shapes that carry their own prefix.
    out = mask_prefixed_tokens(&out);

    out
}

/// Replace the value following `key`, `key=`, or `key: `.
fn mask_key_value(input: &str, key: &str) -> String {
    let lowered = input.to_ascii_lowercase();
    let mut result = String::with_capacity(input.len());
    let mut cursor = 0usize;

    while let Some(relative) = lowered[cursor..].find(key) {
        let start = cursor + relative;
        let after_key = start + key.len();
        // Determine the separator that follows the key.
        let rest = &input[after_key..];
        let sep_len = if rest.starts_with("=\"") {
            2
        } else if rest.starts_with('=') || rest.starts_with(':') {
            1
        } else {
            // The key appears as a bare word; leave it and continue past it.
            result.push_str(&input[cursor..after_key]);
            cursor = after_key;
            continue;
        };

        let value_start = after_key + sep_len;
        // A quoted value ends at the quote; an unquoted one at whitespace,
        // comma, semicolon, or ampersand.
        let quoted = input[after_key..].starts_with("=\"");
        let value_slice = &input[value_start..];
        let value_end = if quoted {
            value_slice.find('"').map(|i| value_start + i).unwrap_or(input.len())
        } else {
            value_slice
                .find(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '&')
                .map(|i| value_start + i)
                .unwrap_or(input.len())
        };

        result.push_str(&input[cursor..value_start]);
        result.push_str(MASK);
        cursor = value_end;
    }

    result.push_str(&input[cursor..]);
    result
}

/// Mask tokens that carry a recognizable prefix.
fn mask_prefixed_tokens(input: &str) -> String {
    const PREFIXES: &[&str] = &[
        "sk-",         // OpenAI and compatible
        "nvapi-",      // NVIDIA
        "ghp_",        // GitHub personal access token
        "gho_",        // GitHub OAuth
        "xoxb-",       // Slack
        "Bearer ",     // Authorization header value
        "-----BEGIN",  // PEM private key
    ];

    let mut out = input.to_string();
    for prefix in PREFIXES {
        while let Some(index) = out.find(prefix) {
            let tail = &out[index + prefix.len()..];
            let len = tail
                .find(|c: char| {
                    c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}'
                })
                .unwrap_or(tail.len());
            if len < 8 {
                // Too short to be a credential; avoid mangling ordinary words.
                break;
            }
            let end = index + prefix.len() + len;
            out.replace_range(index + prefix.len()..end, MASK);
        }
    }
    out
}

/// Mask sensitive values in a JSON document, recursively.
///
/// Used for telemetry detail, audit detail, and any error context that might
/// carry a user-supplied body.
pub fn redact_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, item) in map {
                if is_sensitive_key(key) {
                    out.insert(key.clone(), serde_json::Value::String(MASK.to_string()));
                } else {
                    out.insert(key.clone(), redact_json(item));
                }
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(redact_json).collect())
        }
        serde_json::Value::String(text) => serde_json::Value::String(redact(text)),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_value_pair_is_masked() {
        let out = redact("api_key=sk-abcdefghijklmnop");
        assert!(!out.contains("abcdefghijklmnop"), "{out}");
        assert!(out.contains(MASK));
    }

    #[test]
    fn a_quoted_json_value_is_masked() {
        let out = redact(r#"{"token": "abcdef123456"}"#);
        assert!(!out.contains("abcdef123456"), "{out}");
    }

    #[test]
    fn a_bearer_token_is_masked_even_without_a_key() {
        let out = redact("Authorization: Bearer abcdefghijklmnopqrst");
        assert!(!out.contains("abcdefghijklmnopqrst"), "{out}");
    }

    #[test]
    fn a_prefixed_token_is_masked() {
        let out = redact("using nvapi-1234567890abcdef now");
        assert!(!out.contains("1234567890abcdef"), "{out}");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        // A redactor that mangles normal messages would be switched off by the
        // first person who had to read a log.
        let text = "sandbox hermes-01 started in 412 ms";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn a_short_word_after_a_prefix_is_not_mangled() {
        let text = "the disk-usage check";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn nested_json_is_redacted_at_every_depth() {
        let value = serde_json::json!({
            "sandbox": "a",
            "nested": { "password": "hunter2", "list": [ { "api_key": "k" } ] }
        });
        let out = redact_json(&value);
        let text = out.to_string();
        assert!(!text.contains("hunter2"), "{text}");
        assert_eq!(out["sandbox"], "a");
    }

    #[test]
    fn sensitive_key_matching_is_case_insensitive() {
        assert!(is_sensitive_key("API_KEY"));
        assert!(is_sensitive_key("NvidiaApiKey"));
        assert!(!is_sensitive_key("sandbox_name"));
    }
}
