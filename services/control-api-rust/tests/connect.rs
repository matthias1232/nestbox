//! How to reach a sandbox, against a real PostgreSQL.
//!
//! The connect view is one read of the node's address and hostname, the
//! sandbox's published ports, and the key state. These tests prove it says why
//! nothing can be dialled, dials the hostname once one is set, and that setting
//! the endpoint is validated and needs the node permission.
//!
//!   export TEST_DATABASE_URL="postgres://core@localhost/nestbox_connect?host=/tmp/opencode&port=55432"
//!   cargo test --test connect -- --test-threads=1

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::events::bus::LocalBus;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::{access_service, ServiceContext};

const ENVIRONMENT: &str = "00000000-0000-0000-0000-000000000002";
const CLUSTER: &str = "00000000-0000-0000-0000-000000000004";
const LOCAL_NODE: &str = "00000000-0000-0000-0000-000000000003";

fn uuid(value: &str) -> Uuid {
    Uuid::from_str(value).unwrap_or_else(|_| Uuid::nil())
}

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
    let _ = sqlx::query("GRANT USAGE, CREATE ON SCHEMA public TO nestbox_app")
        .execute(&pool)
        .await;
    sqlx::migrate!("./migrations").run(&pool).await.ok()?;
    Some(pool)
}

fn context(pool: &sqlx::PgPool) -> ServiceContext {
    ServiceContext::new(
        pool.clone(),
        Arc::new(FakeRuntime::new()),
        Arc::new(LocalBus::new()),
        PathBuf::from(std::env::temp_dir()),
    )
}

/// The signed-in operator. The audit trail references a real user, so the
/// principal needs a row.
async fn user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (organization_id, email, display_name, password_hash, role)
         VALUES ('00000000-0000-0000-0000-000000000001', $1, 'Test', 'x', 'admin')
         RETURNING id",
    )
    .bind(format!("stack-{}@example.test", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("insert user")
}

fn principal_of(user_id: Uuid) -> Principal {
    Principal {
        user_id,
        environment_id: uuid(ENVIRONMENT),
        role: Role::Admin,
        display_name: "test".into(),
        via: CredentialSource::Token,
    }
}

async fn sandbox(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO sandboxes (environment_id, cluster_id, node_id, name, image, status)
         VALUES ($1, $2, $3, $4, 'ghcr.io/nestbox/base@sha256:0000', 'running')
         RETURNING id",
    )
    .bind(uuid(ENVIRONMENT))
    .bind(uuid(CLUSTER))
    .bind(uuid(LOCAL_NODE))
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("insert sandbox")
}

#[tokio::test]
async fn a_node_without_an_address_says_so_and_a_set_address_is_dialled() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool);
    let principal = principal_of(user(&pool).await);
    let id = sandbox(&pool, "hermes").await;
    sqlx::query(
        "INSERT INTO forwards (environment_id, sandbox_id, local_port, target_port, purpose, label, state)
         VALUES ($1, $2, 22101, 22, 'ssh', 'SSH', 'active'), ($1, $2, 22102, 8080, 'agent', 'Hermes', 'active')",
    )
    .bind(uuid(ENVIRONMENT))
    .bind(id)
    .execute(&pool)
    .await
    .expect("forwards");

    let before = access_service::connect(&ctx, &principal, id).await.expect("connect");
    assert!(before.host.is_none());
    assert!(before.hint.expect("hint").contains("127.0.0.1"));
    assert_eq!(before.ports.len(), 2);

    access_service::set_node_endpoint(
        &ctx,
        &principal,
        uuid(LOCAL_NODE),
        Some("192.168.1.20"),
        Some("Box.Example.NET"),
    )
    .await
    .expect("set");

    let after = access_service::connect(&ctx, &principal, id).await.expect("connect");
    assert_eq!(after.host.as_deref(), Some("box.example.net"));
    assert_eq!(after.address.as_deref(), Some("192.168.1.20"));
    assert_eq!(after.ports[1].dial.as_deref(), Some("box.example.net:22102"));
    assert!(after.ssh_config.expect("config").contains("Port 22101"));
}

#[tokio::test]
async fn a_bad_address_or_hostname_is_refused_and_a_viewer_may_not_set_one() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool);
    let principal = principal_of(user(&pool).await);
    let node = uuid(LOCAL_NODE);

    assert!(access_service::set_node_endpoint(&ctx, &principal, node, Some("not-an-ip"), None).await.is_err());
    assert!(access_service::set_node_endpoint(&ctx, &principal, node, None, Some("bad host")).await.is_err());
    assert!(access_service::set_node_endpoint(&ctx, &principal, node, None, Some("-x.example.net")).await.is_err());

    let viewer = Principal { role: Role::Viewer, ..principal };
    assert!(access_service::set_node_endpoint(&ctx, &viewer, node, Some("10.0.0.1"), None).await.is_err());
}
