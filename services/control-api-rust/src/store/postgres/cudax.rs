//! CUDA-X catalog and per-node installations.
//!
//! Both tables are global like `nodes`; a component is a fact about software,
//! not about a tenant, and an install belongs to a node.

use sqlx::Row;
use uuid::Uuid;

use super::super::TenantScope;
use crate::domain::cudax::{
    CudaxComponent, CudaxState, LibraryAssessment, NodeLibraryInstall,
};
use crate::error::StoreError;

const COMPONENT_COLUMNS: &str = "id, key, name, category, version, description, apt_packages, \
     container_images, requires_gpu, min_driver, min_cuda, docs_url";

fn row_to_component(row: &sqlx::postgres::PgRow) -> Result<CudaxComponent, StoreError> {
    Ok(CudaxComponent {
        id: row.try_get("id")?,
        key: row.try_get("key")?,
        name: row.try_get("name")?,
        category: row.try_get("category")?,
        version: row.try_get("version")?,
        description: row.try_get("description")?,
        apt_packages: row.try_get("apt_packages")?,
        container_images: row.try_get("container_images")?,
        requires_gpu: row.try_get("requires_gpu")?,
        min_driver: row.try_get("min_driver")?,
        min_cuda: row.try_get("min_cuda")?,
        docs_url: row.try_get("docs_url")?,
    })
}

/// Every active component, grouped by category then name.
pub async fn list_components(scope: &mut TenantScope<'_>) -> Result<Vec<CudaxComponent>, StoreError> {
    let sql = format!(
        "SELECT {COMPONENT_COLUMNS} FROM cudax_components WHERE is_active ORDER BY category, name"
    );
    let rows = sqlx::query(&sql).fetch_all(scope.connection()).await?;
    rows.iter().map(row_to_component).collect()
}

/// One component by its stable key.
pub async fn get_component(
    scope: &mut TenantScope<'_>,
    key: &str,
) -> Result<Option<CudaxComponent>, StoreError> {
    let sql = format!("SELECT {COMPONENT_COLUMNS} FROM cudax_components WHERE key = $1");
    let row = sqlx::query(&sql)
        .bind(key)
        .fetch_optional(scope.connection())
        .await?;
    row.as_ref().map(row_to_component).transpose()
}

/// Add a component to the catalog. Keys are unique; an existing key is updated.
pub async fn upsert_component(
    scope: &mut TenantScope<'_>,
    component: &CudaxComponent,
) -> Result<CudaxComponent, StoreError> {
    let sql = format!(
        "INSERT INTO cudax_components
            (key, name, category, version, description, apt_packages, container_images,
             requires_gpu, min_driver, min_cuda, docs_url)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         ON CONFLICT (key) DO UPDATE SET
            name = EXCLUDED.name, category = EXCLUDED.category, version = EXCLUDED.version,
            description = EXCLUDED.description, apt_packages = EXCLUDED.apt_packages,
            container_images = EXCLUDED.container_images, requires_gpu = EXCLUDED.requires_gpu,
            min_driver = EXCLUDED.min_driver, min_cuda = EXCLUDED.min_cuda,
            docs_url = EXCLUDED.docs_url, updated_at = now()
         RETURNING {COMPONENT_COLUMNS}"
    );
    let row = sqlx::query(&sql)
        .bind(&component.key)
        .bind(&component.name)
        .bind(&component.category)
        .bind(&component.version)
        .bind(&component.description)
        .bind(&component.apt_packages)
        .bind(&component.container_images)
        .bind(component.requires_gpu)
        .bind(&component.min_driver)
        .bind(&component.min_cuda)
        .bind(&component.docs_url)
        .fetch_one(scope.connection())
        .await?;
    row_to_component(&row)
}

/// One install row, resolved against its node and component.
pub async fn list_installs(
    scope: &mut TenantScope<'_>,
    node_id: Option<Uuid>,
) -> Result<Vec<NodeLibraryInstall>, StoreError> {
    let sql = format!(
        "SELECT i.id, i.node_id, n.name AS node_name, i.version, i.state, i.force_install,
                i.assessment, i.last_error, i.installed_at, i.created_at, i.updated_at,
                c.id AS component_id, c.key AS component_key, c.name AS component_name,
                c.category AS component_category, c.version AS component_version,
                c.description AS component_description, c.apt_packages AS component_apt_packages,
                c.container_images AS component_container_images,
                c.requires_gpu AS component_requires_gpu, c.min_driver AS component_min_driver,
                c.min_cuda AS component_min_cuda, c.docs_url AS component_docs_url
           FROM node_cudax_installs i
           JOIN nodes n ON n.id = i.node_id
           JOIN cudax_components c ON c.id = i.component_id
          WHERE ($1::uuid IS NULL OR i.node_id = $1)
          ORDER BY n.name, c.category, c.name"
    );
    let rows = sqlx::query(&sql)
        .bind(node_id)
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_install).collect()
}

fn row_to_install(row: &sqlx::postgres::PgRow) -> Result<NodeLibraryInstall, StoreError> {
    let state_raw: String = row.try_get("state")?;
    let assessment: serde_json::Value = row.try_get("assessment")?;
    Ok(NodeLibraryInstall {
        id: row.try_get("id")?,
        node_id: row.try_get("node_id")?,
        node_name: row.try_get("node_name")?,
        version: row.try_get("version")?,
        state: CudaxState::parse(&state_raw).unwrap_or(CudaxState::Failed),
        force_install: row.try_get("force_install")?,
        assessment: serde_json::from_value(assessment).unwrap_or_else(|_| {
            LibraryAssessment::none(String::new())
        }),
        last_error: {
            let value: String = row.try_get("last_error")?;
            if value.is_empty() { None } else { Some(value) }
        },
        installed_at: row.try_get("installed_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        component: CudaxComponent {
            id: row.try_get("component_id")?,
            key: row.try_get("component_key")?,
            name: row.try_get("component_name")?,
            category: row.try_get("component_category")?,
            version: row.try_get("component_version")?,
            description: row.try_get("component_description")?,
            apt_packages: row.try_get("component_apt_packages")?,
            container_images: row.try_get("component_container_images")?,
            requires_gpu: row.try_get("component_requires_gpu")?,
            min_driver: row.try_get("component_min_driver")?,
            min_cuda: row.try_get("component_min_cuda")?,
            docs_url: row.try_get("component_docs_url")?,
        },
    })
}

/// Assign a component to a node, or refresh what the previous assignment says.
pub async fn upsert_install(
    scope: &mut TenantScope<'_>,
    node_id: Uuid,
    component_id: Uuid,
    version: &str,
    force_install: bool,
    assessment: &LibraryAssessment,
    created_by: Option<Uuid>,
) -> Result<(), StoreError> {
    let sql = "INSERT INTO node_cudax_installs
                  (node_id, component_id, version, state, force_install, assessment, created_by)
               VALUES ($1, $2, $3, 'planned', $4, $5, $6)
               ON CONFLICT (node_id, component_id, version) DO UPDATE SET
                  force_install = EXCLUDED.force_install,
                  assessment = EXCLUDED.assessment,
                  state = CASE WHEN node_cudax_installs.state = 'installed'
                               THEN 'installed' ELSE 'planned' END,
                  last_error = '', updated_at = now()";
    sqlx::query(sql)
        .bind(node_id)
        .bind(component_id)
        .bind(version)
        .bind(force_install)
        .bind(serde_json::to_value(assessment).unwrap_or_default())
        .bind(created_by)
        .execute(scope.connection())
        .await?;
    Ok(())
}

/// Move one installation to a new state.
pub async fn set_state(
    scope: &mut TenantScope<'_>,
    install_id: Uuid,
    state: CudaxState,
    error: &str,
    installed: bool,
) -> Result<(), StoreError> {
    let sql = "UPDATE node_cudax_installs
                  SET state = $2, last_error = $3,
                      installed_at = CASE WHEN $4 THEN now() ELSE installed_at END,
                      updated_at = now()
                WHERE id = $1";
    sqlx::query(sql)
        .bind(install_id)
        .bind(state.as_str())
        .bind(error)
        .bind(installed)
        .execute(scope.connection())
        .await?;
    Ok(())
}

/// Remove the assignment entirely; the software on the node is not touched.
pub async fn delete_install(
    scope: &mut TenantScope<'_>,
    install_id: Uuid,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM node_cudax_installs WHERE id = $1")
        .bind(install_id)
        .execute(scope.connection())
        .await?;
    Ok(())
}

/// One install row by id.
pub async fn get_install(
    scope: &mut TenantScope<'_>,
    install_id: Uuid,
) -> Result<Option<NodeLibraryInstall>, StoreError> {
    let sql = format!(
        "SELECT i.id, i.node_id, n.name AS node_name, i.version, i.state, i.force_install,
                i.assessment, i.last_error, i.installed_at, i.created_at, i.updated_at,
                c.id AS component_id, c.key AS component_key, c.name AS component_name,
                c.category AS component_category, c.version AS component_version,
                c.description AS component_description, c.apt_packages AS component_apt_packages,
                c.container_images AS component_container_images,
                c.requires_gpu AS component_requires_gpu, c.min_driver AS component_min_driver,
                c.min_cuda AS component_min_cuda, c.docs_url AS component_docs_url
           FROM node_cudax_installs i
           JOIN nodes n ON n.id = i.node_id
           JOIN cudax_components c ON c.id = i.component_id
          WHERE i.id = $1"
    );
    let row = sqlx::query(&sql)
        .bind(install_id)
        .fetch_optional(scope.connection())
        .await?;
    row.as_ref().map(row_to_install).transpose()
}
