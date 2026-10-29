//! Nodes: the machines sandboxes and models run on.
//!
//! A node's capacity is not a stored field. It is the last hardware profile of
//! that node minus what its model installs reserve, and this module is the one
//! place where those facts meet. That is what gives the console a
//! single answer to "how much VRAM is free on node-a" instead of a number per
//! screen, and what lets a second model be refused against what is *left*
//! rather than against the card's nominal size.
//!
//! Two rules keep the number honest:
//!
//! * A node nobody has measured reports zeros, not an estimate. An invented
//!   total would make a model look installable on a machine no one looked at.
//! * Free memory never drops below zero. "0 GiB free" is a fact a UI can draw;
//!   "-3 GiB free" is a bug report.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::model::HardwareProfile;
use crate::domain::node::{Node, NodeBudget};
use crate::domain::pagination::{ListParams, Page};
use crate::domain::sandbox::SandboxSummary;
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::store::TenantScope;
use crate::store::postgres::model_installs;
use crate::store::postgres::models as hardware_store;
use crate::store::postgres::nodes as store;

/// Longest accepted display name. Long enough for "gpu-box-01 (RTX 4090)",
/// short enough that a table column still reads.
const MAX_DISPLAY_NAME_CHARS: usize = 120;

/// Longest accepted description.
const MAX_DESCRIPTION_CHARS: usize = 500;

/// A node with the numbers a console draws it with.
///
/// The node row, its budget, and whether the budget came from a measurement are
/// answered together on purpose: a caller that has to fetch the profile
/// separately would sooner or later show a budget without saying it is a guess.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct NodeView {
    #[serde(flatten)]
    pub node: Node,
    /// Total, reserved, and free memory, plus the counts.
    pub budget: NodeBudget,
    /// False when the node has never been measured; `budget` is then zeros.
    pub hardware_known: bool,
    /// When the measurement behind `budget` was taken.
    pub detected_at: Option<DateTime<Utc>>,
}

/// What the installed models of one node hold.
///
/// A struct rather than three positional numbers because the difference between
/// RAM and VRAM at a call site is a byte count either way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reservations {
    pub ram_bytes: i64,
    pub vram_bytes: i64,
    /// How many installs contributed, so the budget can say "3 models".
    pub installs: i64,
}

/// Every node with its budget and sandbox count.
pub async fn list(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<NodeView>, ApiError> {
    require(principal, Permission::NodeRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;

    let nodes = store::list(&mut scope).await.map_err(ApiError::from)?;
    // One query for all counts instead of one per node: the number of nodes is
    // small today and should not become the reason the list is slow.
    let counts = store::sandbox_counts(&mut scope)
        .await
        .map_err(ApiError::from)?;

    let mut views = Vec::with_capacity(nodes.len());
    for node in nodes {
        let sandboxes = counts.get(&node.id).copied().unwrap_or(0);
        views.push(view(&mut scope, node, sandboxes).await?);
    }

    scope.commit().await.map_err(ApiError::from)?;
    Ok(views)
}

/// One node with its budget.
pub async fn get(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
) -> Result<NodeView, ApiError> {
    require(principal, Permission::NodeRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;

    let node = find(&mut scope, id).await?;
    let sandboxes = store::sandbox_count(&mut scope, id)
        .await
        .map_err(ApiError::from)?;
    let view = view(&mut scope, node, sandboxes).await?;

    scope.commit().await.map_err(ApiError::from)?;
    Ok(view)
}

/// The sandboxes of this environment that sit on one node.
///
/// This is the sandbox list with one filter, not a second list shape: paging,
/// facets, and the merge with what the runtime reports all behave exactly as
/// they do on `GET /sandboxes`, so the two cannot drift apart.
pub async fn sandboxes(
    ctx: &ServiceContext,
    principal: &Principal,
    node_id: Uuid,
) -> Result<Page<SandboxSummary>, ApiError> {
    require(principal, Permission::NodeRead)?;

    // A node that does not exist is a 404, not an empty page: an empty page is
    // what "this node has no sandboxes" looks like.
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let node = find(&mut scope, node_id).await?;
    scope.commit().await.map_err(ApiError::from)?;

    // The store reads this key as a bound uuid, so the name never becomes SQL.
    let params = ListParams {
        filters: [("node_id".to_string(), node.id.to_string())]
            .into_iter()
            .collect(),
        ..ListParams::default()
    };
    super::sandbox_service::list(ctx, principal, &params).await
}

/// Resolve a node reference from a query parameter: an id or a name.
///
/// A reference that matches neither is `404 node.not_found` rather than an empty
/// list, because a filter naming a node that does not exist is a typo and a
/// silently empty page would hide it.
pub async fn resolve(
    ctx: &ServiceContext,
    principal: &Principal,
    reference: &str,
) -> Result<Node, ApiError> {
    require(principal, Permission::NodeRead)?;
    let reference = reference.trim();

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;

    let node = match Uuid::parse_str(reference) {
        Ok(id) => find(&mut scope, id).await?,
        Err(_) => store::get_by_name(&mut scope, reference)
            .await
            .map_err(|error| match error {
                StoreError::NotFound => ApiError::not_found(
                    ErrorCode::NODE_NOT_FOUND,
                    format!("No node named '{reference}'."),
                ),
                other => ApiError::from(other),
            })?,
    };

    scope.commit().await.map_err(ApiError::from)?;
    Ok(node)
}

/// Rename or describe a node.
///
/// Nothing operational changes here: `kind`, `runner_url`, and the status come
/// from the node and its runner, never from a request body.
pub async fn update_display(
    ctx: &ServiceContext,
    principal: &Principal,
    id: Uuid,
    display_name: &str,
    description: Option<&str>,
) -> Result<NodeView, ApiError> {
    require(principal, Permission::NodeManage)?;

    let display_name = display_name.trim();
    if display_name.is_empty() {
        return Err(ApiError::validation("A node needs a display name."));
    }
    if display_name.chars().count() > MAX_DISPLAY_NAME_CHARS {
        return Err(ApiError::validation(format!(
            "A display name may be at most {MAX_DISPLAY_NAME_CHARS} characters."
        )));
    }
    let description = description.map(str::trim).filter(|text| !text.is_empty());
    if let Some(text) = description {
        if text.chars().count() > MAX_DESCRIPTION_CHARS {
            return Err(ApiError::validation(format!(
                "A description may be at most {MAX_DESCRIPTION_CHARS} characters."
            )));
        }
    }

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;

    let node = store::update_display(&mut scope, id, display_name, description)
        .await
        .map_err(|error| match error {
            StoreError::NotFound => ApiError::not_found(
                ErrorCode::NODE_NOT_FOUND,
                format!("No node with id '{id}'."),
            ),
            other => ApiError::from(other),
        })?;

    // Nodes are global, the audit trail is per environment: the event records
    // who renamed the machine from the point of view of their own tenant.
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "node.update_display",
            subject_type: Some("node"),
            subject_id: Some(node.id),
            detail: &serde_json::json!({
                "name": node.name,
                "display_name": display_name,
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;

    let sandboxes = store::sandbox_count(&mut scope, id)
        .await
        .map_err(ApiError::from)?;
    let view = view(&mut scope, node, sandboxes).await?;

    scope.commit().await.map_err(ApiError::from)?;
    Ok(view)
}

/// Compose a node's budget from its last measurement and its reservations.
///
/// Pure so the arithmetic can be tested without a database. Three inputs, three
/// facts, and no fallback between them: without a profile the answer is zeros.
pub fn budget_from(
    profile: Option<&HardwareProfile>,
    reserved: Reservations,
    sandboxes: i64,
) -> NodeBudget {
    let Some(profile) = profile else {
        return NodeBudget {
            sandboxes: count_i32(sandboxes),
            ..NodeBudget::default()
        };
    };

    // On a unified-memory machine (DGX Spark, GB10) one pool is both RAM and
    // VRAM, so the VRAM total is that pool. Adding the discrete cards to it
    // would count the same bytes twice.
    let vram_total_bytes = if profile.is_unified() {
        profile.addressable_memory_bytes()
    } else {
        profile.total_vram_bytes
    };
    let ram_total_bytes = profile.addressable_ram_bytes();

    let mut budget = NodeBudget {
        ram_total_bytes,
        ram_free_bytes: ram_total_bytes,
        vram_total_bytes,
        vram_free_bytes: vram_total_bytes,
        cpu_cores: profile.cores(),
        gpu_count: profile.gpu_count(),
        sandboxes: count_i32(sandboxes),
        ..NodeBudget::default()
    };

    // `reserve` is the one place that clamps at zero, so composition goes
    // through it instead of subtracting by hand somewhere else.
    budget.reserve(reserved.ram_bytes, reserved.vram_bytes);
    // `reserve` counts the call, not the installs behind it.
    budget.installs = count_i32(reserved.installs);
    budget
}

/// What a node has left, for a caller that is about to place something on it.
///
/// A node that has never been measured reports zeros; the caller decides whether
/// that means "refuse" or "allow, and say the capacity is unknown".
pub async fn free_budget(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
) -> Result<NodeBudget, ApiError> {
    let profile = hardware_store::latest_hardware(scope, node_id)
        .await
        .map_err(ApiError::from)?;
    let (ram_bytes, vram_bytes, installs) = model_installs::reserved_on(scope, node_id)
        .await
        .map_err(ApiError::from)?;
    Ok(budget_from(
        profile.as_ref(),
        Reservations {
            ram_bytes,
            vram_bytes,
            installs,
        },
        0,
    ))
}

/// One node with its budget, reading the two facts its row does not carry.
async fn view(
    scope: &mut TenantScope<'_>,
    node: Node,
    sandboxes: i64,
) -> Result<NodeView, ApiError> {
    let profile = hardware_store::latest_hardware(scope, node.id)
        .await
        .map_err(ApiError::from)?;
    let measured = profile.is_some();
    let detected_at = profile.as_ref().map(|profile| profile.detected_at);
    let mut budget = free_budget(scope, node.id).await?;
    // The one number `free_budget` leaves out, because it is not capacity.
    budget.sandboxes = count_i32(sandboxes);

    Ok(NodeView {
        hardware_known: measured,
        detected_at,
        budget,
        node,
    })
}

/// Read one node by id, mapping "gone" to the error code a client branches on.
async fn find(scope: &mut TenantScope<'_>, id: Uuid) -> Result<Node, ApiError> {
    store::get(scope, id)
        .await
        .map_err(|error| match error {
            StoreError::NotFound => ApiError::not_found(
                ErrorCode::NODE_NOT_FOUND,
                format!("No node with id '{id}'."),
            ),
            other => ApiError::from(other),
        })
}

/// A count as `i32`, saturating rather than wrapping or panicking.
///
/// The counters are informational; a node with more than two billion sandboxes
/// is not a case worth an error, and a wrapped negative would draw wrong.
fn count_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::GpuDevice;

    const GIB: i64 = 1024 * 1024 * 1024;

    fn profile(ram: i64, vram: i64, cores: i32) -> HardwareProfile {
        HardwareProfile {
            id: Uuid::nil(),
            environment_id: Uuid::nil(),
            node_id: Uuid::nil(),
            detected_at: Utc::now(),
            hostname: Some("vm200".to_string()),
            gpus: if vram > 0 {
                vec![GpuDevice {
                    index: 0,
                    name: "NVIDIA RTX 4090".to_string(),
                    memory_bytes: vram,
                    free_bytes: Some(vram),
                }]
            } else {
                Vec::new()
            },
            total_vram_bytes: vram,
            unified_memory_bytes: None,
            unified_memory_source: None,
            cuda_version: None,
            driver_version: None,
            detection_note: None,
            cpu_model: Some("Intel(R) Core(TM) i7-2600".to_string()),
            cpu_cores: Some(cores),
            cpu_threads: Some(cores),
            system_ram_bytes: Some(ram),
        }
    }

    #[test]
    fn a_reservation_reduces_what_is_free() {
        let hardware = profile(16 * GIB, 24 * GIB, 4);
        let budget = budget_from(
            Some(&hardware),
            Reservations {
                ram_bytes: 6 * GIB,
                vram_bytes: 8 * GIB,
                installs: 2,
            },
            3,
        );

        assert_eq!(budget.ram_total_bytes, 16 * GIB);
        assert_eq!(budget.ram_reserved_bytes, 6 * GIB);
        assert_eq!(budget.ram_free_bytes, 10 * GIB);
        assert_eq!(budget.vram_total_bytes, 24 * GIB);
        assert_eq!(budget.vram_reserved_bytes, 8 * GIB);
        assert_eq!(budget.vram_free_bytes, 16 * GIB);
        assert_eq!(budget.installs, 2);
        assert_eq!(budget.sandboxes, 3);
        assert_eq!(budget.cpu_cores, 4);
        assert_eq!(budget.gpu_count, 1);
    }

    #[test]
    fn free_never_goes_negative() {
        // A smaller node than the reservations assume: the answer is "nothing
        // left", not a negative number a bar chart would draw backwards.
        let hardware = profile(8 * GIB, 0, 4);
        let budget = budget_from(
            Some(&hardware),
            Reservations {
                ram_bytes: 12 * GIB,
                vram_bytes: GIB,
                installs: 1,
            },
            0,
        );

        assert_eq!(budget.ram_free_bytes, 0);
        assert_eq!(budget.ram_reserved_bytes, 12 * GIB);
        assert_eq!(budget.vram_total_bytes, 0);
        assert_eq!(budget.vram_free_bytes, 0);
    }

    #[test]
    fn no_measurement_reports_zeros_instead_of_a_guess() {
        let budget = budget_from(
            None,
            Reservations {
                ram_bytes: 4 * GIB,
                vram_bytes: 2 * GIB,
                installs: 1,
            },
            2,
        );

        assert_eq!(budget.ram_total_bytes, 0);
        assert_eq!(budget.ram_free_bytes, 0);
        assert_eq!(budget.vram_total_bytes, 0);
        assert_eq!(budget.vram_free_bytes, 0);
        assert_eq!(budget.cpu_cores, 0);
        assert_eq!(budget.gpu_count, 0);
        // What the environment owns is still reported: it comes from the
        // sandbox table, not from a measurement.
        assert_eq!(budget.sandboxes, 2);
    }

    #[test]
    fn a_unified_pool_is_the_vram_total_rather_than_the_sum_of_cards() {
        let mut hardware = profile(128 * GIB, 0, 20);
        hardware.total_vram_bytes = 0;
        hardware.unified_memory_bytes = Some(120 * GIB);

        let budget = budget_from(Some(&hardware), Reservations::default(), 0);

        assert_eq!(budget.ram_total_bytes, 120 * GIB);
        assert_eq!(budget.vram_total_bytes, 120 * GIB);
        assert_eq!(budget.gpu_count, 1);
    }
}
