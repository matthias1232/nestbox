//! Baselines, agent packs, and the tool catalog as data.
//!
//! Three tables answer "what can go into a sandbox, and how" — the same rule as
//! the model catalog (ADR 0013, ADR 0020). Nothing here decides what a pack
//! needs: the row does. The code keeps only the rule that cannot live in a
//! column, `AgentPack::supports`, and applies it in one place so the catalog
//! endpoints and the sandbox service cannot disagree about compatibility.
//!
//! There is no write endpoint in this phase: a pack is curated with the
//! release, so a change to one is a migration rather than an admin action.

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::pack::{AgentPack, Baseline, PackDefaults, ToolForBaseline};
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::store::TenantScope;
use crate::store::postgres::agent_packs as store;

/// Every baseline, recommended first.
///
/// The order comes from the store rather than being re-sorted here: the
/// console's default choice and the API's first row must be the same baseline,
/// and two sorts would eventually disagree.
pub async fn list_baselines(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<Baseline>, ApiError> {
    require(principal, Permission::RecipeRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let baselines = store::list_baselines(&mut scope)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(baselines)
}

/// The enabled agent packs, optionally narrowed to those a baseline supports.
///
/// An unknown baseline is a `404`, not an empty list: an empty list reads as
/// "no pack fits this OS", which is a lie when the baseline does not exist.
pub async fn list_agent_packs(
    ctx: &ServiceContext,
    principal: &Principal,
    baseline: Option<&str>,
) -> Result<Vec<AgentPack>, ApiError> {
    require(principal, Permission::RecipeRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let wanted = match requested_baseline(baseline) {
        Some(key) => Some(baseline_by_key(&mut scope, key).await?),
        None => None,
    };
    let packs = store::list_packs(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(match wanted {
        Some(baseline) => packs
            .into_iter()
            .filter(|pack| pack.supports(&baseline.key))
            .collect(),
        None => packs,
    })
}

/// The tool catalog, optionally resolved for one baseline's package manager.
///
/// Without a baseline every tool shows both of its names and claims no
/// resolution. With one, `package` is the name a build for that baseline would
/// install, and a tool with no name for that manager is marked unavailable
/// rather than silently dropped — a builder that offers it would produce a
/// Dockerfile line that fails at build time.
pub async fn tool_catalog(
    ctx: &ServiceContext,
    principal: &Principal,
    baseline: Option<&str>,
) -> Result<Vec<ToolForBaseline>, ApiError> {
    require(principal, Permission::RecipeRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let manager = match requested_baseline(baseline) {
        Some(key) => Some(baseline_by_key(&mut scope, key).await?.package_manager),
        None => None,
    };
    let tools = store::list_tools(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(match manager {
        Some(manager) => tools
            .into_iter()
            .map(|tool| ToolForBaseline::resolved(tool, manager))
            .collect(),
        None => tools.into_iter().map(ToolForBaseline::unresolved).collect(),
    })
}

/// The image, command, ports, environment slots, and home paths a pack runs
/// with.
///
/// This is what replaced the `match` that named four packs by hand (ADR 0020).
/// It takes the caller's transaction instead of opening its own, because the
/// sandbox service resolves the pack inside the transaction that writes the
/// sandbox: a pack edited between the read and the write would otherwise leave
/// a row that no longer matches its catalog entry.
///
/// An unknown key is `404 pack.not_found` rather than a fallback image; a pack
/// that does not support `baseline_key` is `409 pack.incompatible` naming both.
/// `baseline_key` is `None` only where no baseline is recorded — a plain
/// sandbox has none — and then compatibility is not claimed rather than
/// assumed.
pub async fn pack_defaults(
    scope: &mut TenantScope<'_>,
    pack_key: &str,
    baseline_key: Option<&str>,
) -> Result<PackDefaults, ApiError> {
    let pack = match store::get_pack(scope, pack_key).await {
        Ok(pack) => pack,
        Err(StoreError::NotFound) => return Err(unknown_pack(pack_key)),
        Err(error) => return Err(ApiError::from(error)),
    };
    defaults_from(pack, baseline_key)
}

/// The baseline named by a query parameter, treating an empty value as absent.
fn requested_baseline(baseline: Option<&str>) -> Option<&str> {
    baseline.map(str::trim).filter(|key| !key.is_empty())
}

/// One baseline by key, or a `404 baseline.not_found` naming it.
async fn baseline_by_key(scope: &mut TenantScope<'_>, key: &str) -> Result<Baseline, ApiError> {
    store::get_baseline(scope, key).await.map_err(|error| match error {
        StoreError::NotFound => ApiError::not_found(
            ErrorCode::BASELINE_NOT_FOUND,
            format!("No baseline with key '{key}'."),
        ),
        other => ApiError::from(other),
    })
}

/// The refusal for a key that is not in `agent_packs`.
fn unknown_pack(pack_key: &str) -> ApiError {
    ApiError::not_found(
        ErrorCode::PACK_NOT_FOUND,
        format!("No agent pack with key '{pack_key}'."),
    )
}

/// Turn a pack row into the defaults a sandbox is created from, refusing a
/// pack the baseline cannot carry.
///
/// Pure so the refusal table is testable without a database; the row itself is
/// the whole input.
fn defaults_from(pack: AgentPack, baseline_key: Option<&str>) -> Result<PackDefaults, ApiError> {
    if let Some(baseline_key) = baseline_key.filter(|key| !pack.supports(key)) {
        return Err(ApiError::conflict(
            ErrorCode::PACK_INCOMPATIBLE,
            format!(
                "Pack '{}' does not support baseline '{baseline_key}'; it supports {}.",
                pack.key,
                supported_baselines(&pack.base_os)
            ),
        ));
    }

    Ok(PackDefaults {
        pack_key: pack.key,
        image: pack.default_image,
        command: pack.default_command,
        ports: pack.ports,
        env_slots: pack.env_slots,
        home_paths: pack.home_paths,
    })
}

/// How to read a pack's `base_os` list in a refusal message.
fn supported_baselines(baselines: &[String]) -> String {
    if baselines.is_empty() {
        "every baseline".to_string()
    } else {
        baselines.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::pack::InstallKind;

    /// A pack row with everything a lookup needs.
    fn pack(key: &str, base_os: &[&str], command: &[&str], env_slots: &[&str]) -> AgentPack {
        AgentPack {
            key: key.into(),
            name: key.into(),
            vendor: None,
            description: None,
            base_os: base_os.iter().map(|key| (*key).to_string()).collect(),
            install_kind: InstallKind::None,
            install_ref: None,
            home_paths: vec![".home".into()],
            requires_gpu: false,
            needs_inference: !env_slots.is_empty(),
            default_image: format!("nestbox-{key}:local"),
            default_command: command.iter().map(|part| (*part).to_string()).collect(),
            ports: vec![8080],
            env_slots: env_slots.iter().map(|slot| (*slot).to_string()).collect(),
            enabled: true,
        }
    }

    /// The seeded shape: hermes is limited to the Ubuntu-family baselines,
    /// ubuntu-dev and git run everywhere.
    fn catalog() -> Vec<AgentPack> {
        vec![
            pack(
                "hermes",
                &["ubuntu-24.04", "debian-12"],
                &["sleep", "infinity"],
                &["OPENAI_BASE_URL", "OPENAI_API_KEY"],
            ),
            pack("ubuntu-dev", &[], &["sleep", "infinity"], &[]),
            pack("openclaw", &["ubuntu-24.04"], &[], &[]),
        ]
    }

    /// What the service does, over an in-memory catalog, so the refusal table
    /// runs without a database.
    fn lookup(
        catalog: &[AgentPack],
        pack_key: &str,
        baseline_key: Option<&str>,
    ) -> Result<PackDefaults, ApiError> {
        let pack = catalog
            .iter()
            .find(|pack| pack.key == pack_key)
            .cloned()
            .ok_or_else(|| unknown_pack(pack_key))?;
        defaults_from(pack, baseline_key)
    }

    #[test]
    fn the_lookup_table_refuses_what_the_tables_refuse() {
        // (pack key, baseline, the code it must produce, or None for success)
        let cases: &[(&str, Option<&str>, Option<ErrorCode>)] = &[
            ("hermes", Some("ubuntu-24.04"), None),
            ("hermes", Some("debian-12"), None),
            ("hermes", None, None),
            ("hermes", Some("rocky-9"), Some(ErrorCode::PACK_INCOMPATIBLE)),
            ("openclaw", Some("rocky-9"), Some(ErrorCode::PACK_INCOMPATIBLE)),
            ("ubuntu-dev", Some("rocky-9"), None),
            ("does-not-exist", None, Some(ErrorCode::PACK_NOT_FOUND)),
            ("does-not-exist", Some("ubuntu-24.04"), Some(ErrorCode::PACK_NOT_FOUND)),
        ];

        for (pack_key, baseline, expected) in cases {
            let outcome = lookup(&catalog(), pack_key, *baseline);
            match expected {
                None => assert!(
                    outcome.is_ok(),
                    "{pack_key} on {baseline:?} should resolve, got {:?}",
                    outcome.err().map(|error| error.code.0)
                ),
                Some(code) => {
                    let error = outcome.err().unwrap_or_else(|| {
                        panic!("{pack_key} on {baseline:?} should be refused with {}", code.0)
                    });
                    assert_eq!(error.code, *code, "{pack_key} on {baseline:?}");
                }
            }
        }
    }

    #[test]
    fn a_resolved_pack_carries_the_whole_row() {
        let defaults = lookup(&catalog(), "hermes", Some("debian-12")).expect("hermes fits debian");
        assert_eq!(defaults.pack_key, "hermes");
        assert_eq!(defaults.image, "nestbox-hermes:local");
        assert_eq!(defaults.command, vec!["sleep", "infinity"]);
        assert_eq!(defaults.ports, vec![8080]);
        assert_eq!(defaults.env_slots, vec!["OPENAI_BASE_URL", "OPENAI_API_KEY"]);
        assert_eq!(defaults.home_paths, vec![".home"]);
    }

    #[test]
    fn an_unknown_pack_does_not_fall_back_to_a_default_image() {
        let error = lookup(&catalog(), "nope", None).expect_err("unknown key");
        assert_eq!(error.code, ErrorCode::PACK_NOT_FOUND);
        assert!(error.message.contains("'nope'"));
    }

    #[test]
    fn an_incompatible_pack_names_the_baseline_and_what_it_supports() {
        let error = lookup(&catalog(), "hermes", Some("rocky-9")).expect_err("incompatible");
        assert_eq!(error.code, ErrorCode::PACK_INCOMPATIBLE);
        assert!(
            error.message.contains("rocky-9") && error.message.contains("ubuntu-24.04"),
            "message should name both sides: {}",
            error.message
        );
    }

    #[test]
    fn an_empty_query_value_means_no_baseline() {
        assert_eq!(requested_baseline(None), None);
        assert_eq!(requested_baseline(Some("")), None);
        assert_eq!(requested_baseline(Some("   ")), None);
        assert_eq!(
            requested_baseline(Some(" ubuntu-24.04 ")),
            Some("ubuntu-24.04")
        );
    }

    #[test]
    fn a_pack_that_supports_no_baseline_says_so_plainly() {
        assert_eq!(supported_baselines(&[]), "every baseline");
    }
}
