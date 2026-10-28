//! The preset library and the default policy, against a real PostgreSQL.
//!
//! These tests prove what the console promises: every preset the migration seeds
//! is a document the validator accepts, the "best practice" policy a new sandbox
//! receives is the same document as the preset of that name, a preset becomes a
//! policy that can be assigned, and an operator's own assignment survives the
//! create path's default.
//!
//! They skip when `TEST_DATABASE_URL` is unset, so `cargo test` works without a
//! database:
//!
//!   export TEST_DATABASE_URL="postgres://core@localhost:55434/nestbox_presets?host=/tmp/opencode"
//!   cargo test --test policy_presets -- --test-threads=1

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::domain::policy::{validate_document, PolicyScope};
use nestbox_control_api::events::bus::LocalBus;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::ServiceContext;
use nestbox_control_api::services::policy_service::{self, SAFE_DEFAULTS_POLICY_NAME};
use nestbox_control_api::store::TenantScope;
use nestbox_control_api::store::postgres::policies as policy_store;

const ENVIRONMENT: &str = "00000000-0000-0000-0000-000000000002";
const CLUSTER: &str = "00000000-0000-0000-0000-000000000004";
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

/// A sandbox the create path would have written, on the seeded node.
async fn sandbox(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO sandboxes (environment_id, cluster_id, node_id, name, image, status)
         VALUES ($1, $2, $3, $4, 'ghcr.io/nestbox/base@sha256:0000', 'running')
         RETURNING id",
    )
    .bind(environment_id())
    .bind(Uuid::from_str(CLUSTER).unwrap_or_else(|_| Uuid::nil()))
    .bind(Uuid::from_str(LOCAL_NODE).unwrap_or_else(|_| Uuid::nil()))
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("insert sandbox")
}

async fn template_id(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM policy_templates WHERE environment_id = $1 AND name = $2",
    )
    .bind(environment_id())
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("template is seeded")
}

/// Every preset the seed writes is a document this control plane accepts.
///
/// A preset list that offers a document the save path refuses is worse than a
/// short list: the operator picks one and learns at the last step that it never
/// worked. This is the test that keeps the two ends in one vocabulary.
#[tokio::test]
async fn every_seeded_preset_is_a_document_the_validator_accepts() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let mut scope = TenantScope::begin(&pool, environment_id())
        .await
        .expect("scope");

    let templates = policy_store::list_templates(&mut scope)
        .await
        .expect("templates");
    scope.commit().await.expect("commit");

    assert!(
        templates.len() >= 8,
        "the preset library should carry the eight seeded presets, found {}",
        templates.len()
    );
    for template in &templates {
        let sections = validate_document(&template.spec).unwrap_or_else(|error| {
            panic!("the preset {:?} is not a valid document: {error}", template.name)
        });
        assert!(
            !sections.is_empty(),
            "the preset {:?} names no section",
            template.name
        );
        assert!(
            !template.description.trim().is_empty(),
            "the preset {:?} has no description for the picker",
            template.name
        );
    }

    // The one preset that deliberately permits nothing still says so with a
    // section rather than by being empty.
    let air_gapped = templates
        .iter()
        .find(|t| t.name == "Air-gapped")
        .expect("air-gapped preset");
    assert!(policy_service::project_rules(&air_gapped.spec).is_empty());
    assert!(air_gapped.spec.get("network_policies").is_some());
}

/// The policy a new sandbox receives is the preset of that name, character for
/// character, and it has a first revision to roll back to.
#[tokio::test]
async fn the_default_policy_is_the_preset_of_the_same_name() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let mut scope = TenantScope::begin(&pool, environment_id())
        .await
        .expect("scope");
    let template = policy_store::get_template(
        &mut scope,
        template_id(&pool, SAFE_DEFAULTS_POLICY_NAME).await,
    )
    .await
    .expect("template");
    let policy = policy_store::get_policy_by_name(&mut scope, SAFE_DEFAULTS_POLICY_NAME)
        .await
        .expect("policy lookup")
        .expect("the default policy is seeded");
    scope.commit().await.expect("commit");

    assert_eq!(policy.scope, PolicyScope::Sandbox);
    assert_eq!(
        policy.spec, template.spec,
        "the default and the preset it is named after must be one document"
    );

    let versions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM policy_versions WHERE policy_id = $1")
            .bind(policy.id)
            .fetch_one(&pool)
            .await
            .expect("count versions");
    assert_eq!(versions, 1, "a seeded policy needs a first revision");
}

/// A preset becomes a policy, and assigning it at the same time makes it the
/// sandbox's policy. The assignment carries the policy's name, which is what the
/// console shows next to the sandbox.
#[tokio::test]
async fn a_preset_becomes_a_policy_and_can_be_assigned_in_one_step() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir();
    let ctx = context(&pool, &dir);
    let who = principal(user(&pool).await);
    let sandbox_id = sandbox(&pool, "preset-instantiate").await;
    let id = template_id(&pool, "Model access only").await;

    let result = policy_service::instantiate_template(&ctx, &who, id, Some(sandbox_id))
        .await
        .expect("instantiate");
    assert_eq!(result["policy"]["name"], "Model access only");
    assert_eq!(result["assignment"]["sandbox_id"], sandbox_id.to_string());

    let assignment = policy_service::policy_of_sandbox(&ctx, &who, sandbox_id)
        .await
        .expect("assignment lookup")
        .expect("the sandbox is assigned");
    assert_eq!(assignment.policy_name, "Model access only");
    assert_eq!(assignment.desired_version, 1);
    assert_eq!(assignment.applied_version, None, "nothing has loaded it yet");

    // And the rules the document names exist as rows, because the console's
    // traffic view reads the projection, not the document.
    let mut scope = TenantScope::begin(&pool, environment_id())
        .await
        .expect("scope");
    let templates = policy_store::list_templates(&mut scope)
        .await
        .expect("templates");
    scope.commit().await.expect("commit");
    let template = templates
        .iter()
        .find(|t| t.name == "Model access only")
        .expect("template");
    assert_eq!(policy_service::project_rules(&template.spec).len(), 3);

    let _ = std::fs::remove_dir_all(&dir);
}

/// One sandbox, one policy: a second assignment replaces the first instead of
/// stacking two documents the runtime would have to arbitrate.
#[tokio::test]
async fn a_second_assignment_replaces_the_first() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir();
    let ctx = context(&pool, &dir);
    let who = principal(user(&pool).await);
    let sandbox_id = sandbox(&pool, "preset-replace").await;

    let safe = policy_service::instantiate_template(
        &ctx,
        &who,
        template_id(&pool, SAFE_DEFAULTS_POLICY_NAME).await,
        Some(sandbox_id),
    )
    .await
    .expect("first instantiate");
    let second = policy_service::instantiate_template(
        &ctx,
        &who,
        template_id(&pool, "Air-gapped").await,
        None,
    )
    .await
    .expect("second instantiate");
    let second_id = Uuid::from_str(
        second["policy"]["id"]
            .as_str()
            .expect("policy id is a string"),
    )
    .expect("policy id");

    policy_service::assign_policy_to_sandbox(&ctx, &who, sandbox_id, second_id)
        .await
        .expect("assign");

    let assignment = policy_service::policy_of_sandbox(&ctx, &who, sandbox_id)
        .await
        .expect("lookup")
        .expect("assigned");
    assert_eq!(assignment.policy_id, second_id);
    assert_eq!(assignment.policy_name, "Air-gapped");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM sandbox_policies WHERE sandbox_id = $1")
        .bind(sandbox_id)
        .fetch_one(&pool)
        .await
        .expect("count assignments");
    assert_eq!(rows, 1, "a sandbox carries exactly one policy");

    // The first policy still exists: replacing an assignment is not deleting a
    // policy someone may have written on purpose.
    let first_id = Uuid::from_str(
        safe["policy"]["id"]
            .as_str()
            .expect("policy id is a string"),
    )
    .expect("policy id");
    let mut scope = TenantScope::begin(&pool, environment_id())
        .await
        .expect("scope");
    policy_store::get(&mut scope, first_id)
        .await
        .expect("the replaced policy still exists");
    scope.commit().await.expect("commit");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Unassigning is a real state: the sandbox then has no policy, which the console
/// has to be able to show rather than guessing "the default".
#[tokio::test]
async fn unassigning_leaves_the_sandbox_without_a_policy() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir();
    let ctx = context(&pool, &dir);
    let who = principal(user(&pool).await);
    let sandbox_id = sandbox(&pool, "preset-unassign").await;

    policy_service::instantiate_template(
        &ctx,
        &who,
        template_id(&pool, SAFE_DEFAULTS_POLICY_NAME).await,
        Some(sandbox_id),
    )
    .await
    .expect("instantiate");

    policy_service::unassign_policy_from_sandbox(&ctx, &who, sandbox_id)
        .await
        .expect("unassign");

    assert!(
        policy_service::policy_of_sandbox(&ctx, &who, sandbox_id)
            .await
            .expect("lookup")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The create path's default applies to a sandbox that has nothing, and does not
/// override a choice the caller already made.
///
/// This is the rule the plan promised: best practice by default, overridable.
#[tokio::test]
async fn the_create_path_applies_the_default_but_never_overrides_a_choice() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let dir = secret_dir();
    let ctx = context(&pool, &dir);
    let who = principal(user(&pool).await);

    let fresh = sandbox(&pool, "preset-default").await;
    policy_service::apply_safe_defaults(&ctx, environment_id(), fresh).await;
    let assignment = policy_service::policy_of_sandbox(&ctx, &who, fresh)
        .await
        .expect("lookup")
        .expect("the default was applied");
    assert_eq!(assignment.policy_name, SAFE_DEFAULTS_POLICY_NAME);

    let chosen = sandbox(&pool, "preset-chosen").await;
    let air_gapped = policy_service::instantiate_template(
        &ctx,
        &who,
        template_id(&pool, "Air-gapped").await,
        Some(chosen),
    )
    .await
    .expect("instantiate");
    assert_eq!(air_gapped["assignment"]["policy_name"], "Air-gapped");

    policy_service::apply_safe_defaults(&ctx, environment_id(), chosen).await;
    let assignment = policy_service::policy_of_sandbox(&ctx, &who, chosen)
        .await
        .expect("lookup")
        .expect("still assigned");
    assert_eq!(
        assignment.policy_name, "Air-gapped",
        "the caller's choice wins over the default"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
