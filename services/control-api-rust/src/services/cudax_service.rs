//! CUDA-X catalogue and per-node installation.
//!
//! The service is deliberately thin: it resolves the node, compares the
//! component with the latest hardware probe, records the assessment, and asks
//! the node's runner to pull the declared images. It never refuses because of a
//! warning; `force` exists so a caller can be explicit, not because anything is
//! blocked otherwise.

use uuid::Uuid;

use crate::domain::auth::Principal;
use crate::domain::auth::Permission;
use crate::domain::cudax::{assess, CudaxComponent, CudaxState, LibraryAssessment, NodeLibraryInstall};
use crate::domain::node::Node;
use crate::error::{ApiError, ErrorCode};
use crate::services::access_control::require;
use crate::services::model_service;
use crate::services::ServiceContext;
use crate::store::postgres::{cudax as store, nodes};
use crate::store::TenantScope;

async fn resolve_node(
    ctx: &ServiceContext,
    principal: &Principal,
    node_ref: Option<&str>,
) -> Result<Node, ApiError> {
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let node = match node_ref.map(str::trim).filter(|value| !value.is_empty()) {
        None => nodes::local(&mut scope).await,
        Some(value) => match Uuid::parse_str(value) {
            Ok(id) => nodes::get(&mut scope, id).await,
            Err(_) => nodes::get_by_name(&mut scope, value).await,
        },
    }
    .map_err(|error| match error {
        crate::error::StoreError::NotFound => ApiError::not_found(
            ErrorCode::NODE_NOT_FOUND,
            format!("Node '{}' does not exist.", node_ref.unwrap_or("local")),
        ),
        other => ApiError::from(other),
    })?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(node)
}

/// The component catalog.
pub async fn components(
    ctx: &ServiceContext,
    principal: &Principal,
) -> Result<Vec<CudaxComponent>, ApiError> {
    require(principal, Permission::ModelRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let items = store::list_components(&mut scope)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(items)
}

/// Installations, optionally narrowed to one node.
pub async fn installs(
    ctx: &ServiceContext,
    principal: &Principal,
    node_ref: Option<&str>,
) -> Result<Vec<NodeLibraryInstall>, ApiError> {
    require(principal, Permission::ModelRead)?;
    let node = match node_ref {
        Some(_) => Some(resolve_node(ctx, principal, node_ref).await?),
        None => None,
    };
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let items = store::list_installs(&mut scope, node.map(|node| node.id))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(items)
}

/// Assign a component to a node and store the assessment the operator sees.
///
/// Assignment alone installs nothing. It is the explicit "this node should have
/// this library" statement, and it succeeds on hardware that cannot use it.
pub async fn assign(
    ctx: &ServiceContext,
    principal: &Principal,
    component_key: &str,
    node_ref: Option<&str>,
    force: bool,
) -> Result<NodeLibraryInstall, ApiError> {
    require(principal, Permission::NodeManage)?;
    let node = resolve_node(ctx, principal, node_ref).await?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let component = store::get_component(&mut scope, component_key)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::not_found(
                ErrorCode::CUDAX_COMPONENT_NOT_FOUND,
                format!("CUDA-X component '{component_key}' is not in the catalog."),
            )
        })?;

    let measured = model_service::hardware(ctx, principal, false, Some(&node.name)).await?;
    let assessment = assess(&component, &measured.profile);
    store::upsert_install(
        &mut scope,
        node.id,
        component.id,
        &component.version,
        force,
        &assessment,
        Some(principal.user_id),
    )
    .await
    .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "cudax.assign",
            subject_type: Some("cudax_component"),
            subject_id: Some(component.id),
            detail: &serde_json::json!({
                "component": component.key,
                "node": node.name,
                "warnings": assessment.warnings,
                "force": force,
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let installs = installs_for(ctx, principal, node.id, component.id).await?;
    installs.into_iter().next().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::CUDAX_COMPONENT_NOT_FOUND,
            "The assignment was written but could not be read back.",
        )
    })
}

async fn installs_for(
    ctx: &ServiceContext,
    principal: &Principal,
    node_id: Uuid,
    component_id: Uuid,
) -> Result<Vec<NodeLibraryInstall>, ApiError> {
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let items = store::list_installs(&mut scope, Some(node_id))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(items
        .into_iter()
        .filter(|install| install.component.id == component_id)
        .collect())
}

/// Pull the images a component declares and record the honest outcome.
///
/// A node with no GPU still ends `InstalledUnsupported` after a successful
/// pull: the software is there, the hardware is the missing part, and the
/// warning stays attached to the row. A component that ships only as host
/// packages cannot be installed by the runner and says so instead of pretending.
pub async fn install(
    ctx: &ServiceContext,
    principal: &Principal,
    component_key: &str,
    node_ref: Option<&str>,
    force: bool,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::NodeManage)?;
    let node = resolve_node(ctx, principal, node_ref).await?;
    let measured = model_service::hardware(ctx, principal, false, Some(&node.name)).await?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let component = store::get_component(&mut scope, component_key)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::not_found(
                ErrorCode::CUDAX_COMPONENT_NOT_FOUND,
                format!("CUDA-X component '{component_key}' is not in the catalog."),
            )
        })?;
    let assessment = assess(&component, &measured.profile);
    store::upsert_install(
        &mut scope,
        node.id,
        component.id,
        &component.version,
        force,
        &assessment,
        Some(principal.user_id),
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let install = installs_for(ctx, principal, node.id, component.id)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CUDAX_COMPONENT_NOT_FOUND,
                "The assignment was written but could not be read back.",
            )
        })?;

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    store::set_state(&mut scope, install.id, CudaxState::Installing, "", false)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let mut warnings: Vec<String> = assessment.warnings.clone();
    let mut pulled: Vec<String> = Vec::new();
    let mut failure: Option<String> = None;

    if component.container_images.is_empty() {
        warnings.push(format!(
            "{} is distributed as host packages only; this platform cannot install them inside \
             the runner. The assignment is recorded and nothing is pulled.",
            component.name
        ));
    } else {
        match ctx
            .runtime
            .install_libraries(&component.key, &component.container_images)
            .await
        {
            Ok(images) => pulled = images,
            Err(error) => failure = Some(error.to_string()),
        }
    }

    let state = match (&failure, warnings.is_empty()) {
        (Some(_), _) => CudaxState::Failed,
        (None, true) => CudaxState::Installed,
        (None, false) => CudaxState::InstalledUnsupported,
    };
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    store::set_state(
        &mut scope,
        install.id,
        state,
        failure.as_deref().unwrap_or(""),
        matches!(state, CudaxState::Installed | CudaxState::InstalledUnsupported),
    )
    .await
    .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "cudax.install",
            subject_type: Some("cudax_component"),
            subject_id: Some(component.id),
            detail: &serde_json::json!({
                "component": component.key,
                "node": node.name,
                "state": state.as_str(),
                "pulled_images": pulled,
                "warnings": warnings,
                "forced": force,
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(serde_json::json!({
        "component": component.key,
        "node": node.name,
        "state": state.as_str(),
        "force": force,
        "warnings": warnings,
        "pulled_images": pulled,
        "error": failure,
        "assessment": LibraryAssessment {
            supported: assessment.supported,
            warnings: assessment.warnings,
            detected_gpu: assessment.detected_gpu,
            detected_driver: assessment.detected_driver,
            detected_cuda: assessment.detected_cuda,
        },
    }))
}

/// Remove an assignment. Software already on the node is left in place.
pub async fn remove(
    ctx: &ServiceContext,
    principal: &Principal,
    component_key: &str,
    node_ref: Option<&str>,
) -> Result<serde_json::Value, ApiError> {
    require(principal, Permission::NodeManage)?;
    let node = resolve_node(ctx, principal, node_ref).await?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let install = store::list_installs(&mut scope, Some(node.id))
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .find(|install| install.component.key == component_key)
        .ok_or_else(|| {
            ApiError::not_found(
                ErrorCode::CUDAX_INSTALL_NOT_FOUND,
                format!("'{component_key}' is not assigned to node '{}'.", node.name),
            )
        })?;
    store::delete_install(&mut scope, install.id)
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "cudax.remove",
            subject_type: Some("cudax_component"),
            subject_id: Some(install.component.id),
            detail: &serde_json::json!({
                "component": install.component.key,
                "node": node.name,
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(serde_json::json!({ "component": component_key, "node": node.name, "removed": true }))
}
