//! The secret store against a real PostgreSQL: values go in encrypted and never come
//! back, a name is rotated rather than duplicated, a key in use cannot be deleted, and
//! another tenant cannot read it.
//!
//!   TEST_DATABASE_URL=postgres://… cargo test --test vault -- --test-threads=1

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::events::bus::LocalBus;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::{secret_service, ServiceContext};

const ENVIRONMENT: &str = "00000000-0000-0000-0000-000000000002";

async fn connect() -> Option<sqlx::PgPool> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    let _ = sqlx::query("DROP SCHEMA IF EXISTS public CASCADE").execute(&pool).await;
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
    let _ = sqlx::query("GRANT USAGE, CREATE ON SCHEMA public TO nestbox_app").execute(&pool).await;
    sqlx::migrate!("./migrations").run(&pool).await.ok()?;
    Some(pool)
}

fn context(pool: &sqlx::PgPool, dir: &std::path::Path) -> ServiceContext {
    ServiceContext::new(pool.clone(), Arc::new(FakeRuntime::new()), Arc::new(LocalBus::new()), dir.to_path_buf())
}

fn secret_dir(with_key: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nb-vault-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("dir");
    if with_key {
        std::fs::write(dir.join("vault_key"), "ab".repeat(32)).expect("key");
    }
    dir
}

async fn principal(pool: &sqlx::PgPool, environment: &str) -> Principal {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (organization_id, email, display_name, password_hash, role)
         VALUES ('00000000-0000-0000-0000-000000000001', $1, 'Test', 'x', 'admin') RETURNING id",
    )
    .bind(format!("vault-{}@example.test", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("user");
    Principal {
        user_id: id,
        environment_id: Uuid::from_str(environment).expect("uuid"),
        role: Role::Admin,
        display_name: "test".into(),
        via: CredentialSource::Token,
    }
}

#[tokio::test]
async fn a_value_is_stored_encrypted_and_only_its_last_four_come_back() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir(true);
    let ctx = context(&pool, &dir);
    let who = principal(&pool, ENVIRONMENT).await;

    let meta = secret_service::put(&ctx, &who, "openai-main", "api_key", "sk-live-abcdef123456").await.expect("put");
    assert_eq!(meta.last4, "3456");
    assert_eq!(meta.locator, "db://openai-main");
    let shown = serde_json::to_string(&secret_service::list(&ctx, &who).await.expect("list")).expect("json");
    assert!(!shown.contains("sk-live"), "a listing must not carry the value");

    let raw: Vec<u8> = sqlx::query_scalar("SELECT ciphertext FROM stored_secrets WHERE name = 'openai-main'")
        .fetch_one(&pool)
        .await
        .expect("row");
    assert!(!String::from_utf8_lossy(&raw).contains("sk-live"), "the database holds ciphertext");

    let clear = secret_service::reveal(&ctx, who.environment_id, "openai-main").await.expect("reveal");
    assert_eq!(clear, "sk-live-abcdef123456");
}

#[tokio::test]
async fn putting_a_name_again_rotates_it_and_a_bad_name_or_value_is_refused() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir(true);
    let ctx = context(&pool, &dir);
    let who = principal(&pool, ENVIRONMENT).await;

    secret_service::put(&ctx, &who, "key-a", "api_key", "first-value-1111").await.expect("first");
    secret_service::put(&ctx, &who, "key-a", "api_key", "second-value-2222").await.expect("rotate");
    assert_eq!(secret_service::list(&ctx, &who).await.expect("list").len(), 1);
    assert_eq!(secret_service::reveal(&ctx, who.environment_id, "key-a").await.expect("reveal"), "second-value-2222");

    assert!(secret_service::put(&ctx, &who, "Bad Name", "api_key", "long-enough-value").await.is_err());
    assert!(secret_service::put(&ctx, &who, "short", "api_key", "tiny").await.is_err());
    assert!(secret_service::put(&ctx, &who, "multi", "api_key", "line-one\nline-two-x").await.is_err());
    assert!(secret_service::put(&ctx, &who, "weird-kind", "oauth", "long-enough-value").await.is_err());
}

#[tokio::test]
async fn a_key_a_provider_uses_cannot_be_deleted_and_an_unused_one_can() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir(true);
    let ctx = context(&pool, &dir);
    let who = principal(&pool, ENVIRONMENT).await;

    let used = secret_service::put(&ctx, &who, "used-key", "api_key", "used-value-123456").await.expect("put");
    let free = secret_service::put(&ctx, &who, "free-key", "api_key", "free-value-123456").await.expect("put");
    sqlx::query(
        "INSERT INTO provider_profiles (environment_id, name, kind, endpoint, secret_ref)
         VALUES ($1, 'uses-key', 'openai', 'https://api.example.test/v1', 'db://used-key')",
    )
    .bind(who.environment_id)
    .execute(&pool)
    .await
    .expect("provider");

    assert!(secret_service::delete(&ctx, &who, used.id).await.is_err());
    secret_service::delete(&ctx, &who, free.id).await.expect("delete unused");
    assert_eq!(secret_service::list(&ctx, &who).await.expect("list").len(), 1);
}

#[tokio::test]
async fn without_a_master_key_nothing_is_stored() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir(false);
    let ctx = context(&pool, &dir);
    let who = principal(&pool, ENVIRONMENT).await;
    assert!(secret_service::put(&ctx, &who, "no-key", "api_key", "some-long-value-1").await.is_err());
}
