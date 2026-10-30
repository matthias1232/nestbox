//! Tuning an installed model, and the honest answers for logs and metrics.
//!
//!   export TEST_DATABASE_URL="postgres://nestbox:test@127.0.0.1:55432/nestbox_cudax"
//!   cargo test --test model_tuning -- --test-threads=1

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::domain::model_install::ModelTuning;
use nestbox_control_api::events::bus::LocalBus;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::ServiceContext;
use nestbox_control_api::services::model_service::{self, InstallChange};

const ENVIRONMENT: &str = "00000000-0000-0000-0000-000000000002";
const LOCAL_NODE: &str = "00000000-0000-0000-0000-000000000003";

fn environment_id() -> Uuid {
    Uuid::from_str(ENVIRONMENT).unwrap_or_else(|_| Uuid::nil())
}

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
    let dir = std::env::temp_dir().join(format!("nestbox-tuning-test-{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn context(pool: &sqlx::PgPool, runtime: Arc<FakeRuntime>) -> ServiceContext {
    ServiceContext::new(pool.clone(), runtime, Arc::new(LocalBus::new()), secret_dir())
}

async fn user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (organization_id, email, display_name, password_hash, role)
         VALUES ('00000000-0000-0000-0000-000000000001', $1, 'Test', 'x', 'admin')
         RETURNING id",
    )
    .bind(format!("tuning-{}@example.test", Uuid::new_v4()))
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

/// An installed, switched-off model: editing it does not need a running server.
async fn installed_off(pool: &sqlx::PgPool, key: &str) -> Uuid {
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
            (environment_id, model_key, node_id, state, reserved_ram_bytes, enabled)
         VALUES ($1, $2, $3, 'installed', 1073741824, false) RETURNING id",
    )
    .bind(environment_id())
    .bind(key)
    .bind(Uuid::from_str(LOCAL_NODE).unwrap_or_else(|_| Uuid::nil()))
    .fetch_one(pool)
    .await
    .expect("insert install")
}

#[tokio::test]
async fn tuning_is_saved_on_a_stopped_model() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool, Arc::new(FakeRuntime::new()));
    let who = principal(user(&pool).await);
    let id = installed_off(&pool, "tune-a").await;

    let tuning = ModelTuning {
        context_size: Some(32_768),
        gpu_layers: Some(99),
        parallel: Some(4),
        cache_type_k: Some("q8_0".to_string()),
        metrics: Some(true),
        ..Default::default()
    };
    let answer = model_service::update_install(
        &ctx,
        &who,
        "tune-a",
        None,
        InstallChange {
            settings: Some(tuning.clone()),
            ..Default::default()
        },
    )
    .await
    .expect("save tuning");
    assert_eq!(answer["settings"]["context_size"], 32_768);

    let (stored,): (serde_json::Value,) =
        sqlx::query_as("SELECT settings FROM model_installs WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("read settings");
    assert_eq!(stored["gpu_layers"], 99);
    assert_eq!(stored["cache_type_k"], "q8_0");
}

#[tokio::test]
async fn impossible_tuning_is_refused_with_the_field() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool, Arc::new(FakeRuntime::new()));
    let who = principal(user(&pool).await);
    installed_off(&pool, "tune-b").await;

    let error = model_service::update_install(
        &ctx,
        &who,
        "tune-b",
        None,
        InstallChange {
            settings: Some(ModelTuning {
                context_size: Some(16),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await
    .expect_err("context below 128 must be refused");
    assert!(error.message.contains("context_size"), "{}", error.message);
}

#[tokio::test]
async fn metrics_of_a_model_without_a_server_say_why() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool, Arc::new(FakeRuntime::new()));
    let who = principal(user(&pool).await);
    installed_off(&pool, "metrics-a").await;

    let answer = model_service::metrics(&ctx, &who, "metrics-a", None, 10)
        .await
        .expect("metrics answer");
    assert_eq!(answer["available"], false);
    assert!(answer["reason"].as_str().is_some_and(|reason| !reason.is_empty()));
    assert_eq!(answer["series"].as_array().map(Vec::len), Some(0));
    assert_eq!(answer["consumers"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn logs_of_a_model_without_a_container_name_the_runtime() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool, Arc::new(FakeRuntime::new()));
    let who = principal(user(&pool).await);
    installed_off(&pool, "logs-a").await;

    let error = model_service::logs(&ctx, &who, "logs-a", None, 50)
        .await
        .expect_err("no container means no logs");
    assert!(error.message.contains("could not be read"), "{}", error.message);
}
