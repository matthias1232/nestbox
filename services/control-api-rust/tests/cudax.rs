//! CUDA-X assignment and installation against a real PostgreSQL.
//!
//! These prove what a unit test cannot: that the catalogue is seeded, that an
//! assignment on a node without a GPU is stored with its warnings, that the
//! install asks the runner for exactly the declared images, and that the row
//! ends `installed_unsupported` instead of pretending the node can use it.
//!
//!   export TEST_DATABASE_URL="postgres://core@localhost/nestbox_cudax?host=/tmp/opencode&port=55432"
//!   cargo test --test cudax -- --test-threads=1

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::events::bus::LocalBus;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::ServiceContext;
use nestbox_control_api::services::cudax_service;

const ENVIRONMENT: &str = "00000000-0000-0000-0000-000000000002";

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
    let dir = std::env::temp_dir().join(format!("nestbox-cudax-test-{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn context(pool: &sqlx::PgPool, runtime: Arc<FakeRuntime>) -> ServiceContext {
    ServiceContext::new(
        pool.clone(),
        runtime,
        Arc::new(LocalBus::new()),
        secret_dir(),
    )
}

async fn user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (organization_id, email, display_name, password_hash, role)
         VALUES ('00000000-0000-0000-0000-000000000001', $1, 'Test', 'x', 'admin')
         RETURNING id",
    )
    .bind(format!("cudax-{}@example.test", Uuid::new_v4()))
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

#[tokio::test]
async fn the_seeded_catalogue_is_available() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool, Arc::new(FakeRuntime::new()));
    let who = principal(user(&pool).await);
    let components = cudax_service::components(&ctx, &who)
        .await
        .expect("catalogue");
    assert!(components.iter().any(|item| item.key == "cuda-toolkit"));
    assert!(components.iter().any(|item| item.key == "nccl" && item.requires_gpu));
}

#[tokio::test]
async fn assigning_without_a_gpu_warns_and_still_records_the_row() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, runtime.clone());
    let who = principal(user(&pool).await);

    let install = cudax_service::assign(&ctx, &who, "nccl", None, true)
        .await
        .expect("assign");
    assert!(!install.assessment.supported);
    assert!(install
        .assessment
        .warnings
        .iter()
        .any(|warning| warning.contains("No NVIDIA GPU")));
    assert!(install.force_install);

    let listed = cudax_service::installs(&ctx, &who, None).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].component.key, "nccl");
}

#[tokio::test]
async fn installing_pulls_the_declared_images_and_lands_unsupported() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, runtime.clone());
    let who = principal(user(&pool).await);

    // vLLM declares one image and needs a GPU the fake node does not have.
    let answer = cudax_service::install(&ctx, &who, "vllm", None, true)
        .await
        .expect("install");
    assert_eq!(answer["state"], "installed_unsupported");
    assert_eq!(
        runtime.cudax_installs(),
        vec![(
            "vllm".to_string(),
            vec!["vllm/vllm-openai:latest".to_string()]
        )]
    );
    let listed = cudax_service::installs(&ctx, &who, None).await.expect("list");
    assert_eq!(listed[0].state.as_str(), "installed_unsupported");
    assert!(listed[0].installed_at.is_some());
}

#[tokio::test]
async fn removing_the_assignment_deletes_only_the_row() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool, Arc::new(FakeRuntime::new()));
    let who = principal(user(&pool).await);

    cudax_service::assign(&ctx, &who, "dcgm", None, false)
        .await
        .expect("assign");
    let answer = cudax_service::remove(&ctx, &who, "dcgm", None)
        .await
        .expect("remove");
    assert_eq!(answer["removed"], true);
    let listed = cudax_service::installs(&ctx, &who, None).await.expect("list");
    assert!(listed.is_empty());
}
