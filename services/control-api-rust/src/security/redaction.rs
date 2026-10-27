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

        // The match must be a whole word, or `token` would also fire inside
        // `tokenizer` and mangle an innocent field name.
        let previous_is_boundary =
            start == 0 || !input.as_bytes()[start - 1].is_ascii_alphanumeric();
        if !previous_is_boundary {
            result.push_str(&input[cursor..after_key]);
            cursor = after_key;
            continue;
        }

        // Skip an optional closing quote and spaces: `"token": "x"` is as common
        // as `token=x`, and both must be caught.
        let rest = &input[after_key..];
        let bytes = rest.as_bytes();
        let mut index = 0usize;
        if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
            index += 1;
        }
        while index < bytes.len() && bytes[index] == b' ' {
            index += 1;
        }

        if index >= bytes.len() || (bytes[index] != b':' && bytes[index] != b'=') {
            result.push_str(&input[cursor..after_key]);
            cursor = after_key;
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index] == b' ' {
            index += 1;
        }

        // An Authorization header carries its scheme before the credential.
        // Keeping the scheme readable and masking only what follows makes the
        // log line still legible while removing the secret.
        let mut value_start = after_key + index;
        let lowered_tail = input[value_start..].to_ascii_lowercase();
        for scheme in ["bearer ", "basic ", "token "] {
            if lowered_tail.starts_with(scheme) {
                value_start += scheme.len();
                break;
            }
        }

        let quoted = value_start < input.len()
            && (input.as_bytes()[value_start] == b'"' || input.as_bytes()[value_start] == b'\'');
        let quote = if quoted { Some(input.as_bytes()[value_start]) } else { None };
        let inner_start = if quoted { value_start + 1 } else { value_start };

        let tail = &input[inner_start..];
        let value_end = match quote {
            Some(mark) => tail
                .find(mark as char)
                .map(|offset| inner_start + offset)
                .unwrap_or(input.len()),
            None => tail
                .find(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '&' || c == '}')
                .map(|offset| inner_start + offset)
                .unwrap_or(input.len()),
        };

        result.push_str(&input[cursor..value_start]);
        result.push_str(MASK);

        // Skip past the closing quote when the value was quoted, so the output
        // does not end up with an unbalanced pair.
        cursor = if quoted {
            (value_end + 1).min(input.len())
        } else {
            value_end
        };
        if cursor <= start {
            // Guarantee progress; a zero-width match would loop forever.
            cursor = after_key;
        }
    }

    result.push_str(&input[cursor..]);
    result
}

/// Mask tokens that carry a recognizable prefix.
fn mask_prefixed_tokens(input: &str) -> String {
    const PREFIXES: &[&str] = &[
        "sk-",        // OpenAI and compatible
        "nvapi-",     // NVIDIA
        "ghp_",       // GitHub personal access token
        "gho_",       // GitHub OAuth
        "xoxb-",      // Slack
        "Bearer ",    // Authorization header value
        "-----BEGIN", // PEM block: masked to the end, because a key in a log is a breach
    ];

    // A PEM block is masked from its header to the end of the string. Partial
    // masking of a key would leave usable material behind.
    if let Some(index) = input.find("-----BEGIN") {
        let mut out = String::with_capacity(index + MASK.len());
        out.push_str(&input[..index]);
        out.push_str(MASK);
        return out;
    }

    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    loop {
        // The earliest prefix wins, so masking is stable regardless of the order
        // of the list above.
        let mut best: Option<(usize, &str)> = None;
        for prefix in PREFIXES {
            if let Some(index) = rest.find(prefix) {
                if best.is_none_or(|(current, _)| index < current) {
                    best = Some((index, prefix));
                }
            }
        }

        let Some((index, prefix)) = best else {
            out.push_str(rest);
            return out;
        };

        out.push_str(&rest[..index + prefix.len()]);
        let after = &rest[index + prefix.len()..];

        let length = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}')
            .unwrap_or(after.len());

        if length < 8 {
            // Too short to be a credential. Emit it literally and continue past
            // the prefix, so the same position cannot match a second time.
            out.push_str(&after[..length]);
            rest = &after[length..];
            continue;
        }

        out.push_str(MASK);
        rest = &after[length..];
    }
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
