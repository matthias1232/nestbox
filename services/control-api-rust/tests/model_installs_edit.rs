//! Editing an installed model: reserved RAM and the on/off switch, against a
//! real PostgreSQL.
//!
//!   export TEST_DATABASE_URL="postgres://core@localhost/nestbox_installs?host=/tmp/opencode&port=55432"
//!   cargo test --test model_installs_edit -- --test-threads=1

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::events::bus::LocalBus;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::ServiceContext;
use nestbox_control_api::services::model_service::{self, InstallChange};

const ENVIRONMENT: &str = "00000000-0000-0000-0000-000000000002";
const LOCAL_NODE: &str = "00000000-0000-0000-0000-000000000003";

fn environment_id() -> Uuid {
    Uuid::from_str(ENVIRONMENT).unwrap_or_else(|_| Uuid::nil())
}

/// A fresh schema with the migrations applied, so the seed is exercised.
async fn connect() -> Option<sqlx::PgPool> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    let _ = sqlx::query("DROP SCHEMA IF EXISTS public CASCADE")
        .execute(&pool)
        .await;
    let _ = sqlx::query("CREATE SCHEMA public").execute(&pool).await;
    let _ = sqlx::query(
        "DO $$BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'nestbox_app') THEN
                CREATE ROLE nestbox_app LOGIN;
            END IF;
        END$$",
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query("GRANT USAGE, CREATE ON SCHEMA public TO nestbox_app")
        .execute(&pool)
        .await;
    sqlx::migrate!("./migrations").run(&pool).await.ok()?;
    Some(pool)
}

fn secret_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nestbox-preset-test-{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn context(pool: &sqlx::PgPool, secret_dir: &Path) -> ServiceContext {
    ServiceContext::new(
        pool.clone(),
        Arc::new(FakeRuntime::new()),
        Arc::new(LocalBus::new()),
        secret_dir.to_path_buf(),
    )
}

/// The signed-in operator. `policies.created_by` is a real foreign key, so the
/// principal needs a row: a test that used a nil uuid would only prove that the
/// database rejects it.
async fn user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (organization_id, email, display_name, password_hash, role)
         VALUES ('00000000-0000-0000-0000-000000000001', $1, 'Test', 'x', 'admin')
         RETURNING id",
    )
    .bind(format!("preset-{}@example.test", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("insert user")
}

fn principal(user_id: Uuid) -> Principal {
    Principal {
        user_id,
        environment_id: environment_id(),
        role: Role::Admin,
        display_name: "test".into(),
        via: CredentialSource::Token,
    }
}


/// An installed, running model on the seeded node, as the install pipeline
/// leaves it.
async fn installed(pool: &sqlx::PgPool, key: &str, ram: i64) -> Uuid {
    sqlx::query(
        "INSERT INTO models (key, name, version, parameters, state)
         VALUES ($1, $1, '1', '1B', 'not_installed')
         ON CONFLICT (key) DO NOTHING",
    )
    .bind(key)
    .execute(pool)
    .await
    .ok();
    sqlx::query_scalar(
        "INSERT INTO model_installs
            (environment_id, model_key, node_id, state, reserved_ram_bytes)
         VALUES ($1, $2, $3, 'installed', $4) RETURNING id",
    )
    .bind(environment_id())
    .bind(key)
    .bind(Uuid::from_str(LOCAL_NODE).unwrap_or_else(|_| Uuid::nil()))
    .bind(ram)
    .fetch_one(pool)
    .await
    .expect("insert install")
}

async fn row(pool: &sqlx::PgPool, id: Uuid) -> (bool, i64) {
    sqlx::query_as("SELECT enabled, reserved_ram_bytes FROM model_installs WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("read install")
}

const GIB: i64 = 1024 * 1024 * 1024;

#[tokio::test]
async fn switching_off_frees_the_reservation_and_keeps_the_figure() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir();
    let ctx = context(&pool, &dir);
    let p = principal(user(&pool).await);
    let id = installed(&pool, "edit-model-a", 4 * GIB).await;

    let answer = model_service::update_install(
        &ctx, &p, "edit-model-a", None,
        InstallChange { enabled: Some(false), ..Default::default() },
    )
    .await
    .expect("switch off");
    assert_eq!(answer["enabled"], false);
    assert_eq!(row(&pool, id).await, (false, 4 * GIB));

    let reserved: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(reserved_ram_bytes),0)::bigint FROM model_installs
          WHERE node_id = $1 AND state <> 'failed' AND enabled",
    )
    .bind(Uuid::from_str(LOCAL_NODE).unwrap_or_else(|_| Uuid::nil()))
    .fetch_one(&pool)
    .await
    .expect("sum");
    assert_eq!(reserved, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_switched_off_model_takes_any_figure_without_a_budget_check() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir();
    let ctx = context(&pool, &dir);
    let p = principal(user(&pool).await);
    let id = installed(&pool, "edit-model-b", 4 * GIB).await;
    model_service::update_install(
        &ctx, &p, "edit-model-b", None,
        InstallChange { enabled: Some(false), ..Default::default() },
    )
    .await
    .expect("off");

    model_service::update_install(
        &ctx, &p, "edit-model-b", None,
        InstallChange { reserved_ram_bytes: Some(6 * GIB), ..Default::default() },
    )
    .await
    .expect("edit while off");
    assert_eq!(row(&pool, id).await, (false, 6 * GIB));

    // Below the floor is refused, not stored.
    let refused = model_service::update_install(
        &ctx, &p, "edit-model-b", None,
        InstallChange { reserved_ram_bytes: Some(1024), ..Default::default() },
    )
    .await;
    assert!(refused.is_err());
    assert_eq!(row(&pool, id).await, (false, 6 * GIB));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_unknown_install_is_a_404() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir();
    let ctx = context(&pool, &dir);
    let p = principal(user(&pool).await);
    let refused = model_service::update_install(
        &ctx, &p, "not-installed", None,
        InstallChange { enabled: Some(false), ..Default::default() },
    )
    .await
    .expect_err("missing");
    assert_eq!(refused.status.as_u16(), 404);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_catalogue_minimum_is_a_floor_not_a_warning() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir();
    let ctx = context(&pool, &dir);
    let p = principal(user(&pool).await);
    let id = installed(&pool, "edit-model-c", 4 * GIB).await;
    sqlx::query("UPDATE models SET min_ram_bytes = $2 WHERE key = $1")
        .bind("edit-model-c")
        .bind(3 * GIB)
        .execute(&pool)
        .await
        .expect("set minimum");

    let refused = model_service::update_install(
        &ctx, &p, "edit-model-c", None,
        InstallChange { reserved_ram_bytes: Some(2 * GIB), ..Default::default() },
    )
    .await
    .expect_err("below the minimum");
    assert_eq!(refused.status.as_u16(), 422);
    assert_eq!(row(&pool, id).await.1, 4 * GIB);

    let view = model_service::installs(&ctx, &p, None).await.expect("list");
    let mine = view.iter().find(|i| i.model_key == "edit-model-c").expect("listed");
    assert_eq!(mine.min_ram_bytes, 3 * GIB);
    let _ = std::fs::remove_dir_all(&dir);
}
