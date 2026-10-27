//! Baselines, agent packs, and the tool catalog.
//!
//! All three tables are global catalog data; no environment filter applies.

use sqlx::Row;

use super::super::TenantScope;
use crate::domain::pack::{AgentPack, Baseline, InstallKind, PackageManager, Tool};
use crate::error::StoreError;

fn row_to_baseline(row: &sqlx::postgres::PgRow) -> Result<Baseline, StoreError> {
    let pm_raw: String = row.try_get("package_manager")?;
    Ok(Baseline {
        key: row.try_get("key")?,
        name: row.try_get("name")?,
        recommended: row.try_get("recommended")?,
        base_image: row.try_get("base_image")?,
        package_manager: PackageManager::parse(&pm_raw).unwrap_or(PackageManager::Apt),
        default_tools: row.try_get("default_tools")?,
        openshell_ready: row.try_get("openshell_ready")?,
        notes: row.try_get("notes")?,
    })
}

const BASELINE_COLUMNS: &str =
    "key, name, recommended, base_image, package_manager, default_tools, openshell_ready, notes";

/// Every baseline, recommended first.
pub async fn list_baselines(scope: &mut TenantScope<'_>) -> Result<Vec<Baseline>, StoreError> {
    let sql = format!("SELECT {BASELINE_COLUMNS} FROM baselines ORDER BY recommended DESC, name");
    let rows = sqlx::query(&sql).fetch_all(scope.connection()).await?;
    rows.iter().map(row_to_baseline).collect()
}

/// One baseline by key.
pub async fn get_baseline(scope: &mut TenantScope<'_>, key: &str) -> Result<Baseline, StoreError> {
    let sql = format!("SELECT {BASELINE_COLUMNS} FROM baselines WHERE key = $1");
    let row = sqlx::query(&sql)
        .bind(key)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_baseline(&row)
}

const PACK_COLUMNS: &str = "key, name, vendor, description, base_os, install_kind, install_ref, \
     home_paths, requires_gpu, needs_inference, default_image, default_command, ports, \
     env_slots, enabled";

fn row_to_pack(row: &sqlx::postgres::PgRow) -> Result<AgentPack, StoreError> {
    let kind_raw: String = row.try_get("install_kind")?;
    Ok(AgentPack {
        key: row.try_get("key")?,
        name: row.try_get("name")?,
        vendor: row.try_get("vendor")?,
        description: row.try_get("description")?,
        base_os: row.try_get("base_os")?,
        install_kind: InstallKind::parse(&kind_raw).unwrap_or(InstallKind::None),
        install_ref: row.try_get("install_ref")?,
        home_paths: row.try_get("home_paths")?,
        requires_gpu: row.try_get("requires_gpu")?,
        needs_inference: row.try_get("needs_inference")?,
        default_image: row.try_get("default_image")?,
        default_command: row.try_get("default_command")?,
        ports: row.try_get("ports")?,
        env_slots: row.try_get("env_slots")?,
        enabled: row.try_get("enabled")?,
    })
}

/// Every enabled pack.
pub async fn list_packs(scope: &mut TenantScope<'_>) -> Result<Vec<AgentPack>, StoreError> {
    let sql = format!("SELECT {PACK_COLUMNS} FROM agent_packs WHERE enabled ORDER BY name");
    let rows = sqlx::query(&sql).fetch_all(scope.connection()).await?;
    rows.iter().map(row_to_pack).collect()
}

/// One pack by key, enabled or not.
pub async fn get_pack(scope: &mut TenantScope<'_>, key: &str) -> Result<AgentPack, StoreError> {
    let sql = format!("SELECT {PACK_COLUMNS} FROM agent_packs WHERE key = $1");
    let row = sqlx::query(&sql)
        .bind(key)
        .fetch_optional(scope.connection())
        .await?
        .ok_or(StoreError::NotFound)?;
    row_to_pack(&row)
}

const TOOL_COLUMNS: &str = "key, name, \"group\", description, apt_name, rpm_name";

fn row_to_tool(row: &sqlx::postgres::PgRow) -> Result<Tool, StoreError> {
    Ok(Tool {
        key: row.try_get("key")?,
        name: row.try_get("name")?,
        group: row.try_get("group")?,
        description: row.try_get("description")?,
        apt_name: row.try_get("apt_name")?,
        rpm_name: row.try_get("rpm_name")?,
    })
}

/// The whole tool catalog, grouped order.
pub async fn list_tools(scope: &mut TenantScope<'_>) -> Result<Vec<Tool>, StoreError> {
    let sql = format!("SELECT {TOOL_COLUMNS} FROM tool_catalog ORDER BY \"group\", name");
    let rows = sqlx::query(&sql).fetch_all(scope.connection()).await?;
    rows.iter().map(row_to_tool).collect()
}

/// Tools by key, in the order asked for; unknown keys are skipped.
pub async fn tools_by_key(
    scope: &mut TenantScope<'_>,
    keys: &[String],
) -> Result<Vec<Tool>, StoreError> {
    let sql = format!("SELECT {TOOL_COLUMNS} FROM tool_catalog WHERE key = ANY($1)");
    let rows = sqlx::query(&sql)
        .bind(keys)
        .fetch_all(scope.connection())
        .await?;
    rows.iter().map(row_to_tool).collect()
}
