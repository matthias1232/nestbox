//! Which sandboxes belong together, against a real PostgreSQL.
//!
//! The list groups a lead and its members, so the rules that keep that grouping
//! readable are proven here: a member joins a lead of this environment, a stack
//! is one level deep, a lead cannot become a member, and deleting a lead frees
//! its members instead of deleting them.
//!
//!   export TEST_DATABASE_URL="postgres://core@localhost/nestbox_stacks?host=/tmp/opencode&port=55432"
//!   cargo test --test sandbox_stacks -- --test-threads=1

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::events::bus::LocalBus;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::{sandbox_service, ServiceContext};

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
async fn a_member_joins_a_lead_and_leaves_again() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool);
    let principal = principal_of(user(&pool).await);
    let lead = sandbox(&pool, "hermes").await;
    let member = sandbox(&pool, "hermes-model").await;

    let joined = sandbox_service::set_stack(&ctx, &principal, member, Some(lead))
        .await
        .expect("join");
    assert_eq!(joined.parent_id, Some(lead));

    let left = sandbox_service::set_stack(&ctx, &principal, member, None)
        .await
        .expect("leave");
    assert_eq!(left.parent_id, None);
}

#[tokio::test]
async fn a_stack_is_one_level_deep_and_never_its_own_lead() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool);
    let principal = principal_of(user(&pool).await);
    let lead = sandbox(&pool, "lead").await;
    let member = sandbox(&pool, "member").await;
    let third = sandbox(&pool, "third").await;
    sandbox_service::set_stack(&ctx, &principal, member, Some(lead))
        .await
        .expect("join");

    // A member cannot lead.
    assert!(sandbox_service::set_stack(&ctx, &principal, third, Some(member)).await.is_err());
    // A lead with members cannot become a member.
    assert!(sandbox_service::set_stack(&ctx, &principal, lead, Some(third)).await.is_err());
    // Nobody leads themselves.
    assert!(sandbox_service::set_stack(&ctx, &principal, third, Some(third)).await.is_err());
    // A lead that does not exist is refused, not stored.
    assert!(
        sandbox_service::set_stack(&ctx, &principal, third, Some(Uuid::new_v4()))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn deleting_the_lead_frees_the_members() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let ctx = context(&pool);
    let principal = principal_of(user(&pool).await);
    let lead = sandbox(&pool, "doomed-lead").await;
    let member = sandbox(&pool, "survivor").await;
    sandbox_service::set_stack(&ctx, &principal, member, Some(lead))
        .await
        .expect("join");

    sandbox_service::delete(&ctx, &principal, lead).await.expect("delete");

    let parent: Option<Uuid> = sqlx::query_scalar("SELECT parent_id FROM sandboxes WHERE id = $1")
        .bind(member)
        .fetch_one(&pool)
        .await
        .expect("member survives");
    assert_eq!(parent, None);
}
