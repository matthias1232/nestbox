//! Image templates against a real database.
//!
//! What a pure function cannot prove: that an unshared template stays invisible
//! to another operator of the same environment, that only one default exists per
//! environment, and that applying a template moves the revision and clears the
//! built image.
//!
//! Skips cleanly without `TEST_DATABASE_URL`. The schema is applied with
//! `sqlx::migrate!` when `image_templates` is missing, so the suite works against
//! a fresh database as well as against one prepared with `psql -f`.

use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::OnceCell;
use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::domain::builder::RepositorySpec;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::ServiceContext;
use nestbox_control_api::services::builder_service::{self, RecipeInput};
use nestbox_control_api::services::template_service::{self, TemplateInput};

static READY: OnceCell<bool> = OnceCell::const_new();

async fn ready() -> bool {
    *READY
        .get_or_init(|| async {
            let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
                return false;
            };
            let Ok(pool) = sqlx::PgPool::connect(&url).await else {
                return false;
            };
            let present: Option<String> =
                sqlx::query_scalar("SELECT to_regclass('public.image_templates')::text")
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(None);
            if present.is_none() {
                let setup = async {
                    sqlx::query(
                        "DO $$BEGIN
                            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'nestbox_app') THEN
                                CREATE ROLE nestbox_app LOGIN;
                            END IF;
                        END$$",
                    )
                    .execute(&pool)
                    .await?;
                    sqlx::query("GRANT USAGE, CREATE ON SCHEMA public TO nestbox_app")
                        .execute(&pool)
                        .await?;
                    sqlx::migrate!("./migrations").run(&pool).await?;
                    Ok::<(), sqlx::Error>(())
                }
                .await;
                if setup.is_err() {
                    return false;
                }
            }
            true
        })
        .await
}

async fn pool() -> Option<PgPool> {
    if !ready().await {
        return None;
    }
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    PgPool::connect(&url).await.ok()
}

fn context(pool: &PgPool) -> ServiceContext {
    ServiceContext::new(
        pool.clone(),
        Arc::new(FakeRuntime::new()),
        Arc::new(nestbox_control_api::events::bus::LocalBus::new()),
        std::path::PathBuf::from("/tmp/opencode"),
    )
}

async fn environment_id(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM environments WHERE slug = 'default'")
        .fetch_one(pool)
        .await
        .expect("the default environment is seeded")
}

/// A user of the environment, so `created_by` has a real row behind it.
async fn user(pool: &PgPool, environment_id: Uuid, role: &str) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let email = format!("tpl-{}@example.test", &suffix[..8]);
    sqlx::query_scalar(
        "INSERT INTO users (organization_id, email, display_name, role, status, password_hash)
         SELECT organization_id, $1, $2, $3, 'active', 'x'
           FROM environments WHERE id = $4
         RETURNING id",
    )
    .bind(&email)
    .bind(&email)
    .bind(role)
    .bind(environment_id)
    .fetch_one(pool)
    .await
    .expect("a user")
}

fn principal(environment_id: Uuid, user_id: Uuid, role: Role) -> Principal {
    Principal {
        user_id,
        environment_id,
        role,
        display_name: "Template test".into(),
        via: CredentialSource::Token,
    }
}

fn input(name: &str, shared: bool, is_default: bool) -> TemplateInput {
    TemplateInput {
        name: name.to_string(),
        description: String::new(),
        baseline_key: "ubuntu-24.04".into(),
        tools: vec!["git".into()],
        extra_packages: vec!["jq".into()],
        repositories: vec![RepositorySpec {
            name: "artgate".into(),
            url: "https://packages.example.test/apt".into(),
            suite: Some("stable".into()),
            components: vec!["main".into()],
            signing_key: None,
            credential_ref: None,
        }],
        mcp_server_keys: vec![],
        is_default,
        is_shared: shared,
    }
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", &Uuid::new_v4().simple().to_string()[..8])
}

#[tokio::test]
async fn an_unshared_template_is_invisible_to_another_operator() {
    let Some(pool) = pool().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let environment = environment_id(&pool).await;
    let author = principal(environment, user(&pool, environment, "operator").await, Role::Operator);
    let colleague = principal(environment, user(&pool, environment, "operator").await, Role::Operator);
    let ctx = context(&pool);

    let shared = template_service::create(&ctx, &author, input(&unique("shared"), true, false))
        .await
        .expect("a shared template");
    let private = template_service::create(&ctx, &author, input(&unique("private"), false, false))
        .await
        .expect("a private template");

    let mine = template_service::list(&ctx, &author).await.expect("list");
    let theirs = template_service::list(&ctx, &colleague).await.expect("list");
    assert!(mine.iter().any(|entry| entry.id == private.id));
    assert!(mine.iter().any(|entry| entry.id == shared.id));
    assert!(theirs.iter().any(|entry| entry.id == shared.id));
    assert!(
        !theirs.iter().any(|entry| entry.id == private.id),
        "an unshared template must not be listed to another operator"
    );
}

#[tokio::test]
async fn only_one_template_is_the_default() {
    let Some(pool) = pool().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let environment = environment_id(&pool).await;
    let admin = principal(environment, user(&pool, environment, "admin").await, Role::Admin);
    let ctx = context(&pool);

    let first = template_service::create(&ctx, &admin, input(&unique("first"), true, true))
        .await
        .expect("first default");
    let second = template_service::create(&ctx, &admin, input(&unique("second"), true, true))
        .await
        .expect("second default");

    let current = template_service::default_for(&ctx, &admin)
        .await
        .expect("default")
        .expect("a default exists");
    assert_eq!(current.id, second.id);
    let first_now = template_service::get(&ctx, &admin, first.id).await.expect("get");
    assert!(!first_now.is_default, "the previous default is cleared");
}

#[tokio::test]
async fn applying_a_template_clears_the_built_image_and_moves_the_revision() {
    let Some(pool) = pool().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let environment = environment_id(&pool).await;
    let operator = principal(environment, user(&pool, environment, "operator").await, Role::Operator);
    let ctx = context(&pool);

    let template = template_service::create(&ctx, &operator, input(&unique("apply"), true, false))
        .await
        .expect("template");
    let recipe = builder_service::create(
        &ctx,
        &operator,
        RecipeInput {
            name: unique("tpl-recipe"),
            baseline_key: "ubuntu-24.04".into(),
            tools: vec![],
            extra_packages: vec![],
            repositories: vec![],
            mcp_server_keys: vec![],
            agents: vec![],
            resources: Default::default(),
            project_mounts: vec![],
            node: None,
        },
    )
    .await
    .expect("recipe");

    let applied = template_service::apply_to_recipe(&ctx, &operator, template.id, recipe.id)
        .await
        .expect("apply");
    assert_eq!(applied.revision, recipe.revision + 1);
    assert_eq!(applied.extra_packages, vec!["jq".to_string()]);
    assert_eq!(applied.repositories.len(), 1);
    assert!(applied.image_ref.is_none(), "the old image does not contain the selection");
    assert_eq!(applied.state, nestbox_control_api::domain::builder::RecipeState::Draft);
}
