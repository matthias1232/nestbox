//! What is installed inside a sandbox, as a snapshot.
//!
//! The inventory comes from one fixed script in the runner image: no
//! parameters, no user input, and no way for the agent inside the container to
//! change what it prints. This module stores the result and answers with the
//! snapshot plus its age, because "Hermes 0.22.5" is only useful with "as of
//! four minutes ago".
//!
//! A value of an environment variable never enters the snapshot: the script
//! prints names, and the type (`domain::access::Inventory`) has no field that
//! could hold a value. That is the first control. The second is [`sanitize`],
//! which re-derives every field from the runner's document by allow-list: an
//! entry shaped like `KEY=value` keeps the key and drops the value, and every
//! surviving string goes through the central redactor. A `sk-…` that reaches this
//! module by any route is masked before it is stored.
//!
//! A snapshot is stored, not streamed: taking one runs a script in the container,
//! and the console asks for it when the tab opens rather than on a timer.

use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use super::{metrics_service, require, ServiceContext};
use crate::domain::access::Inventory;
use crate::domain::auth::{Permission, Principal};
use crate::domain::metrics::InventoryProbe;
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::security::redaction;
use crate::store::TenantScope;
use crate::store::postgres::metrics as store;
use crate::store::postgres::sandboxes as sandbox_store;

/// How many packages a snapshot may carry. A fat image has thousands; this is
/// enough for the search box and small enough to stay one row.
const PACKAGE_LIMIT: usize = 4_000;
/// How many listening sockets to keep.
const LISTENING_LIMIT: usize = 200;
/// How many mounts to keep. The probe script already caps at 40.
const MOUNT_LIMIT: usize = 40;
/// How many environment variable names to keep.
const ENV_KEY_LIMIT: usize = 500;
/// How many entries an `os` or tool map may carry.
const MAP_LIMIT: usize = 60;
/// How long one string field may be, so one verbose banner cannot bloat a row.
const STRING_LIMIT: usize = 500;

/// The tools inside a sandbox that are agents, not the runtime they use.
///
/// The split is what gives the table's two columns a meaning: `agents` answers
/// "which agent, which version", and `tool_versions` answers "what does it run
/// on". Both come from the same probe section.
const AGENT_TOOLS: &[&str] = &[
    "hermes",
    "openclaw",
    "openchamber",
    "opencode",
    "claude",
    "codex",
];

/// A snapshot as the console shows it: the facts and how old they are.
///
/// Both fields are optional together: a sandbox that was created a minute ago
/// has no snapshot, and "not taken yet" is a true answer where an empty document
/// would read as "nothing is installed".
#[derive(Debug, Clone, Serialize)]
pub struct InventoryView {
    pub inventory: Option<Inventory>,
    pub age_seconds: Option<i64>,
}

/// The last snapshot for one sandbox, with its age.
pub async fn load(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<InventoryView, ApiError> {
    require(principal, Permission::SandboxRead)?;

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let inventory = store::read_inventory(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;

    // An unknown sandbox is a 404 even when it has no snapshot; a known one with
    // no snapshot is a 200 with a null document.
    if inventory.is_none() {
        sandbox_store::get(&mut scope, sandbox_id)
            .await
            .map_err(sandbox_error)?;
    }
    scope.commit().await.map_err(ApiError::from)?;

    Ok(match inventory {
        Some(inventory) => {
            let age = (Utc::now() - inventory.taken_at).num_seconds().max(0);
            InventoryView {
                inventory: Some(inventory),
                age_seconds: Some(age),
            }
        }
        None => InventoryView {
            inventory: None,
            age_seconds: None,
        },
    })
}

/// Take a new snapshot and store it.
///
/// Write permission rather than read: the probe runs a script in the container,
/// and "who asked the sandbox what it has" is an operator action even though it
/// changes nothing inside it.
pub async fn refresh(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<InventoryView, ApiError> {
    require(principal, Permission::SandboxWrite)?;
    take_snapshot(ctx, principal.environment_id, sandbox_id).await
}

/// Ask the runner for a snapshot and store it under one sandbox.
///
/// Public so a job or a test can take a snapshot without a principal; the
/// permission check belongs to [`refresh`], not here.
pub async fn take_snapshot(
    ctx: &ServiceContext,
    environment_id: Uuid,
    sandbox_id: Uuid,
) -> Result<InventoryView, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = sandbox_store::get(&mut scope, sandbox_id)
        .await
        .map_err(sandbox_error)?;
    scope.commit().await.map_err(ApiError::from)?;

    // No transaction is open while the runner works: the probe can take a
    // second, and holding a connection for it would let a slow container
    // exhaust the pool.
    let probe = ctx
        .runtime
        .inventory(&sandbox.name)
        .await
        .map_err(metrics_service::runner_error)?;

    let snapshot = sanitize(sandbox_id, &probe);
    store_snapshot(ctx, environment_id, &snapshot).await?;

    Ok(InventoryView {
        inventory: Some(snapshot),
        age_seconds: Some(0),
    })
}

/// Store a snapshot that was already taken.
///
/// Separate from [`take_snapshot`] so a snapshot can be stored without a runner
/// — a replay, a test, or a node that answered before the control plane
/// restarted.
pub async fn store_snapshot(
    ctx: &ServiceContext,
    environment_id: Uuid,
    snapshot: &Inventory,
) -> Result<(), ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    store::upsert_inventory(&mut scope, snapshot)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// Turn the runner's document into the stored snapshot, by allow-list.
///
/// This is the security boundary of the feature, and it is a rebuild rather than
/// a copy: only the keys named here survive, so a section the runner adds cannot
/// arrive in the database unreviewed, and a value the runner should not have
/// printed cannot ride along in a field nobody looked at.
pub fn sanitize(sandbox_id: Uuid, probe: &InventoryProbe) -> Inventory {
    let facts = &probe.facts;
    let tools = string_map(facts.get("agents"));

    let agents = tools
        .iter()
        .filter(|(name, _)| AGENT_TOOLS.contains(&name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let tool_versions = tools
        .iter()
        .filter(|(name, _)| !AGENT_TOOLS.contains(&name.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    Inventory {
        sandbox_id,
        taken_at: probe.taken_at,
        os: object_from(string_map(facts.get("os"))),
        packages: packages(facts.get("packages")),
        agents: object_from(agents),
        tool_versions: object_from(tool_versions),
        listening: listening(facts.get("listening")),
        env_keys: env_names(facts.get("env_keys")),
        mounts: mounts(facts.get("mounts")),
        image: image(facts.get("image")),
        note: probe
            .note
            .as_deref()
            .map(|note| truncate(&redaction::redact(note), STRING_LIMIT)),
    }
}

/// The string entries of a JSON object, redacted, capped, and sorted by key.
fn string_map(value: Option<&serde_json::Value>) -> Vec<(String, String)> {
    let Some(map) = value.and_then(|value| value.as_object()) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = map
        .iter()
        .filter_map(|(key, item)| {
            let text = item.as_str().map(|text| redact_text(text))?;
            Some((truncate(key, STRING_LIMIT), text))
        })
        .take(MAP_LIMIT)
        .collect();
    out.sort();
    out
}

/// A string map back to a JSON object.
fn object_from(entries: Vec<(String, String)>) -> serde_json::Value {
    let mut map = serde_json::Map::with_capacity(entries.len());
    for (key, value) in entries {
        map.insert(key, serde_json::Value::String(value));
    }
    serde_json::Value::Object(map)
}

/// Package name and version pairs, capped and redacted.
fn packages(value: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return serde_json::Value::Array(Vec::new());
    };

    let mut out = Vec::with_capacity(items.len().min(PACKAGE_LIMIT));
    for item in items.iter().take(PACKAGE_LIMIT) {
        let name = item
            .get("name")
            .and_then(|name| name.as_str())
            .unwrap_or_default();
        let version = item
            .get("version")
            .and_then(|version| version.as_str())
            .unwrap_or_default();
        out.push(serde_json::json!({
            "name": truncate(&redaction::redact(name), STRING_LIMIT),
            "version": truncate(&redaction::redact(version), STRING_LIMIT),
        }));
    }
    serde_json::Value::Array(out)
}

/// Listening sockets as `address` and `protocol`.
fn listening(value: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return serde_json::Value::Array(Vec::new());
    };

    let mut out = Vec::with_capacity(items.len().min(LISTENING_LIMIT));
    for item in items.iter().take(LISTENING_LIMIT) {
        let address = item
            .get("address")
            .and_then(|address| address.as_str())
            .unwrap_or_default();
        let protocol = item
            .get("protocol")
            .and_then(|protocol| protocol.as_str())
            .unwrap_or("tcp");
        out.push(serde_json::json!({
            "address": truncate(&redaction::redact(address), STRING_LIMIT),
            "protocol": truncate(&redaction::redact(protocol), 16),
        }));
    }
    serde_json::Value::Array(out)
}

/// Mounts as `target`, `fstype`, and `source`.
fn mounts(value: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return serde_json::Value::Array(Vec::new());
    };

    let mut out = Vec::with_capacity(items.len().min(MOUNT_LIMIT));
    for item in items.iter().take(MOUNT_LIMIT) {
        let field = |key: &str| {
            item.get(key)
                .and_then(|value| value.as_str())
                .map(|text| truncate(&redaction::redact(text), STRING_LIMIT))
                .unwrap_or_default()
        };
        out.push(serde_json::json!({
            "target": field("target"),
            "fstype": field("fstype"),
            "source": field("source"),
        }));
    }
    serde_json::Value::Array(out)
}

/// The image identity, which the runner read from the container rather than
/// from a probe: reference, id, creation time, and the `nestbox.*` labels.
fn image(value: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(image) = value else {
        return serde_json::json!({});
    };
    let text = |key: &str| {
        image
            .get(key)
            .and_then(|value| value.as_str())
            .map(|text| truncate(&redaction::redact(text), STRING_LIMIT))
            .unwrap_or_default()
    };
    let labels = image
        .get("labels")
        .and_then(|labels| labels.as_object())
        .map(|labels| {
            let mut kept = serde_json::Map::new();
            for (key, value) in labels {
                // Only the product's own labels: an image's other labels are
                // build metadata nobody asked for and may name a private registry.
                if !key.starts_with("nestbox.") {
                    continue;
                }
                if let Some(value) = value.as_str() {
                    kept.insert(key.clone(), serde_json::Value::String(redact_text(value)));
                }
            }
            serde_json::Value::Object(kept)
        })
        .unwrap_or_else(|| serde_json::json!({}));

    serde_json::json!({
        "reference": text("reference"),
        "id": text("id"),
        "created": text("created"),
        "labels": labels,
    })
}

/// Environment variable *names*.
///
/// An entry that arrived as `KEY=value` keeps `KEY`: the name is the fact the
/// console shows, and the value is the one thing this feature must never store.
/// An entry that is not a POSIX name at all (a bare token, say) is dropped
/// entirely rather than repaired.
fn env_names(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return Vec::new();
    };

    let mut names: Vec<String> = Vec::new();
    for item in items {
        let Some(text) = item.as_str() else {
            continue;
        };
        let name = text
            .split(['=', ':'])
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if is_environment_name(&name) && !names.contains(&name) {
            names.push(name);
        }
        if names.len() >= ENV_KEY_LIMIT {
            break;
        }
    }
    names.sort();
    names
}

/// Whether a string is a POSIX environment variable name.
///
/// The strictness is the point: `sk-live-…` has a hyphen and a value's shape, so
/// it fails here and never becomes a row in `env_keys`.
fn is_environment_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 128 {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Redact a free-form string and cap its length on a character boundary.
fn redact_text(text: &str) -> String {
    truncate(&redaction::redact(text), STRING_LIMIT)
}

/// Cut a string to at most `limit` characters, never inside a character.
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect()
}

/// Map a store failure for a missing sandbox.
fn sandbox_error(error: StoreError) -> ApiError {
    match error {
        StoreError::NotFound => {
            ApiError::not_found(ErrorCode::SANDBOX_NOT_FOUND, "No such sandbox.")
        }
        other => ApiError::from(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(facts: serde_json::Value) -> InventoryProbe {
        InventoryProbe {
            taken_at: Utc::now(),
            facts,
            note: None,
        }
    }

    #[test]
    fn an_environment_value_never_reaches_the_snapshot() {
        // The script prints names, but a value that arrives anyway — a probe that
        // was changed, a message that was pasted — must not be stored.
        let snapshot = sanitize(
            Uuid::nil(),
            &probe(serde_json::json!({
                "env_keys": ["PATH", "OPENAI_API_KEY=sk-live-abcdefghijklmnop", "sk-live-abcdefghijklmnop"],
            })),
        );

        assert_eq!(snapshot.env_keys, vec!["OPENAI_API_KEY", "PATH"]);
        let json = serde_json::to_string(&snapshot).expect("the snapshot serialises");
        assert!(!json.contains("sk-live"), "{json}");
        assert!(!json.contains("abcdefghijklmnop"), "{json}");
    }

    #[test]
    fn a_secret_shaped_string_is_masked_wherever_it_appears() {
        let snapshot = sanitize(
            Uuid::nil(),
            &probe(serde_json::json!({
                "agents": { "hermes": "0.22.5 (sk-live-abcdefghijklmnop)" },
                "mounts": [{ "target": "/home/nestbox", "fstype": "bind", "source": "/srv/sk-live-abcdefghijklmnop" }],
                "os": { "NAME": "Ubuntu", "TOKEN": "ghp_abcdefghijklmnop" },
            })),
        );

        let json = serde_json::to_string(&snapshot).expect("the snapshot serialises");
        assert!(!json.contains("sk-live"), "{json}");
        assert!(!json.contains("abcdefghijklmnop"), "{json}");
        // The key survives, the value does not: that is what makes the tab useful.
        assert!(snapshot.os["TOKEN"]
            .as_str()
            .unwrap_or_default()
            .contains(redaction::MASK));
        assert!(snapshot.agents["hermes"]
            .as_str()
            .unwrap_or_default()
            .starts_with("0.22.5"));
    }

    #[test]
    fn agents_and_tools_are_separate_facts() {
        let snapshot = sanitize(
            Uuid::nil(),
            &probe(serde_json::json!({
                "agents": { "hermes": "0.22.5", "node": "v22.11.0", "git": "2.43.0" },
            })),
        );

        assert_eq!(snapshot.agents["hermes"], "0.22.5");
        assert!(snapshot.agents.get("node").is_none());
        assert_eq!(snapshot.tool_versions["node"], "v22.11.0");
        assert_eq!(snapshot.tool_versions["git"], "2.43.0");
    }

    #[test]
    fn a_partial_probe_is_stored_as_partial_rather_than_empty_and_fine() {
        let mut partial = probe(serde_json::json!({ "env_keys": ["PATH"] }));
        partial.note = Some("the probe exited with status 127".into());
        let snapshot = sanitize(Uuid::nil(), &partial);

        assert_eq!(
            snapshot.note.as_deref(),
            Some("the probe exited with status 127")
        );
        assert!(snapshot.packages.as_array().is_some_and(Vec::is_empty));
    }

    #[test]
    fn a_long_field_is_cut_on_a_character_boundary() {
        let long = "ü".repeat(STRING_LIMIT + 10);
        let cut = truncate(&long, STRING_LIMIT);
        assert_eq!(cut.chars().count(), STRING_LIMIT);
        // Still valid UTF-8, which a byte slice would not guarantee.
        assert!(cut.chars().all(|c| c == 'ü'));
    }
}
