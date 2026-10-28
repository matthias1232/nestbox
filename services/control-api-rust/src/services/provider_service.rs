//! Provider profiles: registration, credential resolution, and a live probe.
//!
//! Why this exists as a service rather than a console-only feature: the sandbox
//! creator needs the same answer to "which endpoint and credential should this
//! workload use". Keeping the rule here means a sandbox created from `curl` and
//! one created from the console get the same environment, which is what makes
//! the two paths trustworthy.

use std::collections::BTreeMap;
use std::time::Duration;

use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::model_install::ModelInstall;
use crate::domain::node::Node;
use crate::domain::provider::{ProviderKind, ProviderProfile, ProviderStatus};
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::security::secrets;
use crate::store::TenantScope;
use crate::store::postgres::providers as store;

/// What a probe learned about a provider.
///
/// Deliberately does not include the response body: it can contain provider
/// headers and, in a misconfigured endpoint, somebody else's catalogue.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct ProbeResult {
    pub reachable: bool,
    pub credential_configured: bool,
    pub status: Option<u16>,
    pub model_count: Option<usize>,
    /// A short, redacted explanation when the probe failed.
    pub detail: String,
}

/// All provider profiles in the environment.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<ProviderProfile>, ApiError> {
    require(principal, Permission::ProviderRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let profiles = store::list(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(profiles)
}

/// One provider profile.
pub async fn get(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<ProviderProfile, ApiError> {
    require(principal, Permission::ProviderRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let profile = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(profile)
}

/// What a create request carries.
pub struct NewProvider {
    pub name: String,
    pub kind: ProviderKind,
    pub endpoint: String,
    pub model: Option<String>,
    /// A locator, never a value.
    pub secret_ref: Option<String>,
    pub labels: serde_json::Value,
}

/// Create the provider profile that makes an installed model selectable.
///
/// Called from the install path, so the profile exists the moment the install
/// starts. Two things follow from that: the endpoint does not answer yet, which
/// is why the status is `pending`, and the profile is not handed to a sandbox
/// until a probe has seen it answer (the inference environment filters on
/// `reachable`).
///
/// Nothing is downloaded and no model server is started in this phase; the
/// install reserves memory and records intent, the server follows in phase 2.
pub async fn create_for_install(
    scope: &mut TenantScope<'_>,
    install: &ModelInstall,
    node: &Node,
) -> Result<ProviderProfile, StoreError> {
    // A container name is a DNS label, so the dots and colons of a model key
    // become hyphens: `qwen3-1.7b` serves at `model-qwen3-1-7b`.
    let host = install.model_key.replace(['.', ':', '/'], "-").to_lowercase();
    let name = format!("local/{}@{}", install.model_key, node.name);
    // 8080 is the port `model_server_images` starts the server on. The profile
    // is created before the container exists, so the number lives in two
    // places; the install probe is what proves they agree.
    let endpoint = format!("http://model-{host}:8080/v1");
    store::insert_pending_for_install(
        scope,
        &name,
        &endpoint,
        Some(&install.model_key),
        install.node_id,
        install.id,
    )
    .await
}

/// Remove the profile an uninstall leaves behind.
///
/// A profile that a sandbox still selects is not deleted silently: the caller
/// checks for use first and refuses with `provider.in_use`. This function is the
/// mechanical removal once that check has passed.
pub async fn delete_for_install(
    scope: &mut TenantScope<'_>,
    profile_id: Uuid,
) -> Result<(), ApiError> {
    ensure_not_in_use(scope, profile_id).await?;
    store::delete(scope, profile_id).await.map_err(ApiError::from)
}

/// Refuse to remove a profile a sandbox still selects.
///
/// A sandbox that points at a deleted profile starts with an environment naming
/// an endpoint nobody maintains, so the refusal names the sandboxes instead of
/// leaving them to fail later. Both delete paths call this: an operator deleting
/// a profile and an uninstall removing what it created face the same rule.
async fn ensure_not_in_use(
    scope: &mut TenantScope<'_>,
    profile_id: Uuid,
) -> Result<(), ApiError> {
    let names = store::sandboxes_using(scope, profile_id)
        .await
        .map_err(ApiError::from)?;
    if names.is_empty() {
        return Ok(());
    }
    Err(ApiError::conflict(
        ErrorCode::PROVIDER_IN_USE,
        format!("The provider profile is used by {}.", names.join(", ")),
    )
    .with_details(serde_json::json!({ "sandboxes": names })))
}

/// Register a provider profile.
///
/// The endpoint scheme is validated here rather than only by the table's check
/// constraint, so the operator gets a message that names the field instead of a
/// database error.
pub async fn create(
    ctx: &ServiceContext,
    principal: &Principal,
    request: NewProvider,
) -> Result<ProviderProfile, ApiError> {
    require(principal, Permission::ProviderWrite)?;

    let name = request.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ApiError::validation(
            "A provider name must be between 1 and 120 characters.",
        ));
    }
    let endpoint = request.endpoint.trim();
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return Err(ApiError::validation(
            "A provider endpoint must start with http:// or https://.",
        ));
    }
    if let Some(locator) = request.secret_ref.as_deref() {
        validate_locator(locator)?;
    }

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let profile = store::insert(
        &mut scope,
        store::NewProviderProfile {
            name,
            kind: request.kind,
            endpoint,
            model: request.model.as_deref(),
            secret_ref: request.secret_ref.as_deref(),
            labels: &request.labels,
        },
    )
    .await
    .map_err(ApiError::from)?;

    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "provider.create",
            subject_type: Some("provider"),
            subject_id: Some(profile.id),
            detail: &serde_json::json!({ "name": profile.name, "kind": profile.kind.as_str() }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(profile)
}

/// Change a provider profile.
pub async fn update(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    expected_version: i64,
    endpoint: String,
    model: Option<String>,
    secret_ref: Option<String>,
    labels: serde_json::Value,
) -> Result<ProviderProfile, ApiError> {
    require(principal, Permission::ProviderWrite)?;

    let endpoint = endpoint.trim().to_string();
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return Err(ApiError::validation(
            "A provider endpoint must start with http:// or https://.",
        ));
    }
    if let Some(locator) = secret_ref.as_deref() {
        validate_locator(locator)?;
    }

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let profile = store::update(
        &mut scope,
        id,
        expected_version,
        &endpoint,
        model.as_deref(),
        secret_ref.as_deref(),
        &labels,
    )
    .await
    .map_err(|error| match error {
        crate::error::StoreError::VersionConflict => ApiError::conflict(
            ErrorCode::CONFLICT,
            "This provider was changed by someone else; reload and try again.",
        ),
        crate::error::StoreError::NotFound => ApiError::not_found(
            ErrorCode::PROVIDER_NOT_FOUND,
            "No such provider profile.",
        ),
        other => ApiError::from(other),
    })?;

    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "provider.update",
            subject_type: Some("provider"),
            subject_id: Some(profile.id),
            detail: &serde_json::json!({ "name": profile.name, "version": profile.version }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(profile)
}

/// Remove a provider profile.
pub async fn delete(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::ProviderWrite)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let profile = store::get(&mut scope, id).await.map_err(ApiError::from)?;
    ensure_not_in_use(&mut scope, id).await?;
    store::delete(&mut scope, id).await.map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "provider.delete",
            subject_type: Some("provider"),
            subject_id: Some(id),
            detail: &serde_json::json!({ "name": profile.name }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// Ask the provider whether it answers and whether the credential works.
///
/// The probe is a plain `GET <endpoint>/models`, which every OpenAI-compatible
/// server implements. A failure is reported as a value, not as an error: the
/// operator asked a question and gets an answer.
pub async fn probe(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<ProbeResult, ApiError> {
    require(principal, Permission::ProviderRead)?;
    let profile = get(ctx, principal, id).await?;
    let result = measure(ctx, &profile).await;

    // Persist the answer, because the inference environment filters on it: a
    // status that is only in this response would leave a sandbox pointed at an
    // endpoint the probe just found dead.
    let status = if result.reachable {
        ProviderStatus::Reachable
    } else {
        ProviderStatus::Unreachable
    };
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::set_status(&mut scope, id, status)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(result)
}

/// Ask the endpoint whether it answers.
///
/// No database access, so what "reachable" means is decided in one place. An
/// HTTP answer counts as reachable even when it is a 401: the server is up and
/// the credential is the problem, which is a different fix from "nothing is
/// listening".
pub(crate) async fn measure(ctx: &ServiceContext, profile: &ProviderProfile) -> ProbeResult {
    let credential = match profile.secret_ref.as_deref() {
        Some(locator) => match secrets::resolve(locator, &ctx.secret_dir) {
            Ok(value) => Some(value),
            Err(error) => {
                return ProbeResult {
                    reachable: false,
                    credential_configured: true,
                    status: None,
                    model_count: None,
                    detail: error.message().to_string(),
                };
            }
        },
        None => None,
    };

    let url = format!("{}/models", profile.endpoint.trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return ProbeResult {
                reachable: false,
                credential_configured: profile.credential_configured,
                status: None,
                model_count: None,
                detail: format!("The probe client could not start: {error}"),
            };
        }
    };

    let mut request = client.get(&url);
    if let Some(value) = credential.as_deref() {
        request = request.bearer_auth(value);
    }

    match request.send().await {
        Ok(response) => {
            let status = response.status();
            if !status.is_success() {
                return ProbeResult {
                    reachable: true,
                    credential_configured: profile.credential_configured,
                    status: Some(status.as_u16()),
                    model_count: None,
                    detail: format!(
                        "The provider answered {status}, so the credential or the endpoint is wrong."
                    ),
                };
            }
            let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
            let count = body
                .get("data")
                .and_then(|value| value.as_array())
                .map(|items| items.len());
            ProbeResult {
                reachable: true,
                credential_configured: profile.credential_configured,
                status: Some(status.as_u16()),
                model_count: count,
                detail: "The provider answered and the credential was accepted.".to_string(),
            }
        }
        Err(error) => ProbeResult {
            reachable: false,
            credential_configured: profile.credential_configured,
            status: None,
            model_count: None,
            detail: format!("The provider could not be reached: {error}"),
        },
    }
}

/// The environment variables a sandbox needs to talk to an inference provider.
///
/// Only a pack that needs inference calls this. The rule about *which* profile
/// is used lives here: the environment's default is the first
/// OpenAI-compatible profile by name, so the choice is data and can be changed
/// without a deployment.
///
/// The credential is resolved at this point and never stored. If it cannot be
/// resolved, the sandbox is still created — without inference — because a
/// missing key should not block a workload that does not strictly need one.
pub async fn inference_environment(
    ctx: &ServiceContext,
    environment_id: Uuid,
) -> Result<BTreeMap<String, String>, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, environment_id)
        .await
        .map_err(ApiError::from)?;
    let profiles = store::list(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let Some(profile) = select_inference_profile(&profiles) else {
        return Ok(BTreeMap::new());
    };

    let mut environment = BTreeMap::new();
    let base_url = profile.endpoint.trim_end_matches('/').to_string();
    environment.insert("OPENAI_BASE_URL".to_string(), base_url.clone());
    environment.insert("OPENAI_API_BASE".to_string(), base_url);
    if let Some(model) = profile.model.clone() {
        environment.insert("HERMES_MODEL".to_string(), model);
    }
    environment.insert("HERMES_PROVIDER".to_string(), "openai".to_string());

    if let Some(locator) = profile.secret_ref.as_deref() {
        match secrets::resolve(locator, &ctx.secret_dir) {
            Ok(value) => {
                environment.insert("OPENAI_API_KEY".to_string(), value);
            }
            Err(error) => {
                // Logged without the locator: even a path can disclose layout.
                tracing::warn!(
                    provider = %profile.name,
                    reason = error.message(),
                    "provider credential could not be resolved; sandbox gets no inference key"
                );
            }
        }
    }

    Ok(environment)
}

/// Reject a locator this phase cannot resolve, at write time.
fn validate_locator(locator: &str) -> Result<(), ApiError> {
    if !locator.starts_with("file://") {
        return Err(ApiError::new(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::SECRET_UNRESOLVABLE,
            "Only file:// secret locators are supported in this phase.",
        ));
    }
    if locator.len() <= "file://".len() {
        return Err(ApiError::validation(
            "A file:// locator needs an absolute path.",
        ));
    }
    Ok(())
}


/// Which profile a sandbox should be given, from those the environment has.
///
/// The rule, in one place: prefer an OpenAI-compatible profile that has
/// answered, then any profile that has answered; never a pending one. A
/// pending profile is an install whose server does not exist yet, and handing
/// it to an agent would send requests to a hostname that does not resolve.
pub fn select_inference_profile(profiles: &[ProviderProfile]) -> Option<&ProviderProfile> {
    let speaks_openai = |profile: &&ProviderProfile| {
        matches!(
            profile.kind,
            ProviderKind::OpenaiCompatible | ProviderKind::Openai | ProviderKind::Omniroute
        )
    };
    profiles
        .iter()
        .find(|profile| profile.status == ProviderStatus::Reachable && speaks_openai(profile))
        .or_else(|| {
            profiles
                .iter()
                .find(|profile| profile.status == ProviderStatus::Reachable)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn profile(name: &str, kind: ProviderKind, status: ProviderStatus) -> ProviderProfile {
        ProviderProfile {
            id: Uuid::new_v4(),
            environment_id: Uuid::nil(),
            name: name.to_string(),
            kind,
            endpoint: "http://example.invalid/v1".to_string(),
            model: None,
            secret_ref: None,
            credential_configured: false,
            credential_expiry: None,
            labels: serde_json::json!({}),
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            status,
            node_id: None,
            model_install_id: None,
            last_probed_at: None,
        }
    }

    #[test]
    fn a_pending_profile_never_reaches_a_sandbox() {
        let profiles = vec![profile(
            "local/qwen@local",
            ProviderKind::LocalOpenaiCompatible,
            ProviderStatus::Pending,
        )];
        assert!(select_inference_profile(&profiles).is_none());
    }

    #[test]
    fn an_openai_compatible_profile_wins_over_a_reachable_other() {
        let profiles = vec![
            profile("nvidia", ProviderKind::Nvidia, ProviderStatus::Reachable),
            profile(
                "omniroute",
                ProviderKind::Omniroute,
                ProviderStatus::Reachable,
            ),
        ];
        let chosen = select_inference_profile(&profiles).expect("a profile");
        assert_eq!(chosen.name, "omniroute");
    }

    #[test]
    fn a_reachable_profile_is_better_than_nothing_even_when_it_is_not_openai_shaped() {
        let profiles = vec![
            profile("local/x@local", ProviderKind::LocalOpenaiCompatible, ProviderStatus::Pending),
            profile("anthropic", ProviderKind::Anthropic, ProviderStatus::Reachable),
        ];
        let chosen = select_inference_profile(&profiles).expect("a profile");
        assert_eq!(chosen.name, "anthropic");
    }

    #[test]
    fn an_unreachable_profile_is_never_chosen() {
        let profiles = vec![profile(
            "omniroute",
            ProviderKind::Omniroute,
            ProviderStatus::Unreachable,
        )];
        assert!(select_inference_profile(&profiles).is_none());
    }

    #[test]
    fn the_status_round_trips_through_its_wire_name() {
        for status in [
            ProviderStatus::Pending,
            ProviderStatus::Reachable,
            ProviderStatus::Unreachable,
        ] {
            assert_eq!(ProviderStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(ProviderStatus::parse("something"), None);
    }
}
