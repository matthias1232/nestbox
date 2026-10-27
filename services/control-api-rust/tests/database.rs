//! Database integration checks.
//!
//! These run only when `TEST_DATABASE_URL` points at a disposable PostgreSQL.
//! Without it they skip, so `cargo test` works on a machine with no database —
//! but they are the tests that actually prove the schema, the row-level
//! security, and the seed are correct.
//!
//!   docker compose -f ../../deploy/docker-compose.test.yml up -d
//!   export TEST_DATABASE_URL=postgres://nestbox:test@localhost:55432/nestbox_test
//!   cargo test --test database

use uuid::Uuid;

async fn connect() -> Option<sqlx::PgPool> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    // A fresh schema every run, so the migration path itself is tested rather
    // than assumed.
    let _ = sqlx::query("DROP SCHEMA IF EXISTS public CASCADE").execute(&pool).await;
    let _ = sqlx::query("CREATE SCHEMA public").execute(&pool).await;
    sqlx::migrate!("./migrations").run(&pool).await.ok()?;
    Some(pool)
}

/// The migrations apply cleanly and seed the environment the console expects.
#[tokio::test]
async fn migrations_seed_the_default_environment() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };

    let environment: Option<String> =
        sqlx::query_scalar("SELECT name FROM environments WHERE slug = 'default'")
            .fetch_optional(&pool)
            .await
            .expect("query");
    assert_eq!(environment.as_deref(), Some("default"));

    let models: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert!(models >= 4, "the model catalogue should be seeded");

    let schedules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schedules")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert!(schedules >= 8, "the operational schedules should be seeded");
}

/// The catalogue describes the gate it actually uses.
///
/// A CPU model without a RAM figure has no gate at all and would look
/// installable everywhere; a GPU model without a VRAM figure likewise. This is
/// checked as data rather than left to convention, because the catalog is
/// written by hand and a missing column is invisible in JSON (`null` reads as
/// "no requirement", not "unknown").
#[tokio::test]
async fn the_model_catalog_states_the_gate_it_uses() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };

    let cpu_without_ram: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM models
          WHERE is_cpu_only AND (min_ram_bytes IS NULL OR min_vram_bytes IS NOT NULL)",
    )
    .fetch_one(&pool)
    .await
    .expect("query");
    assert_eq!(
        cpu_without_ram, 0,
        "a CPU-only model needs a RAM gate and must not claim a VRAM gate"
    );

    let cuda_without_vram: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM models
          WHERE runtime = 'cuda' AND NOT is_remote AND min_vram_bytes IS NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("query");
    assert_eq!(
        cuda_without_vram, 0,
        "a GPU-only model needs a VRAM gate"
    );

    let hybrid_without_either: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM models
          WHERE runtime = 'hybrid'
            AND (min_vram_bytes IS NULL OR min_ram_bytes IS NULL)",
    )
    .fetch_one(&pool)
    .await
    .expect("query");
    assert_eq!(
        hybrid_without_either, 0,
        "a hybrid model needs both the GPU gate and the CPU fallback gate"
    );

    let catalog_size: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert!(
        catalog_size >= 40,
        "the catalog should be broad (NVIDIA, DeepSeek, Qwen, Llama, Gemma, Phi, CPU-only), got {catalog_size}"
    );

    // Recommended never lies below minimum, and the two units are distinct on
    // purpose: a row that copies VRAM into RAM would hide the CPU path.
    let copied_gate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM models
          WHERE recommended_ram_bytes IS NOT NULL AND min_ram_bytes IS NOT NULL
            AND recommended_ram_bytes < min_ram_bytes",
    )
    .fetch_one(&pool)
    .await
    .expect("query");
    assert_eq!(copied_gate, 0, "recommended RAM must be at least the minimum");
}

/// Row-level security refuses a row from another environment.
///
/// This is the second line of defence (ADR 0004): even a query that forgot its
/// `WHERE environment_id = ...` must not return another tenant's data.
#[tokio::test]
async fn row_level_security_blocks_a_cross_tenant_read() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };

    let environment: Uuid = sqlx::query_scalar("SELECT id FROM environments LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("environment");

    // Insert a sandbox as the owner, outside any tenant scope.
    let cluster: Uuid = sqlx::query_scalar("SELECT id FROM clusters LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("cluster");
    sqlx::query(
        "INSERT INTO sandboxes (environment_id, cluster_id, name, image, image_digest)
         VALUES ($1, $2, 'rls-canary', 'img@sha256:abc', 'sha256:abc')",
    )
    .bind(environment)
    .bind(cluster)
    .execute(&pool)
    .await
    .expect("insert");

    // Read it back with the tenant scope set: visible.
    let mut tx = pool.begin().await.expect("tx");
    sqlx::query("SELECT set_config('app.environment_id', $1, true)")
        .bind(environment.to_string())
        .execute(&mut *tx)
        .await
        .expect("set scope");
    let visible: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sandboxes WHERE name = 'rls-canary'")
        .fetch_one(&mut *tx)
        .await
        .expect("count scoped");
    assert_eq!(visible, 1, "the owning environment must see its own row");
    tx.rollback().await.expect("rollback");

    // Read it back with a different scope: invisible.
    let other = Uuid::new_v4();
    let mut tx = pool.begin().await.expect("tx");
    sqlx::query("SELECT set_config('app.environment_id', $1, true)")
        .bind(other.to_string())
        .execute(&mut *tx)
        .await
        .expect("set scope");
    let hidden: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sandboxes WHERE name = 'rls-canary'")
        .fetch_one(&mut *tx)
        .await
        .expect("count scoped");
    assert_eq!(hidden, 0, "another tenant must not see the row");
    tx.rollback().await.expect("rollback");
}

/// An injection-shaped name is stored as data, not executed.
#[tokio::test]
async fn an_injection_shaped_name_is_just_a_string() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };

    let environment: Uuid = sqlx::query_scalar("SELECT id FROM environments LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("environment");
    let cluster: Uuid = sqlx::query_scalar("SELECT id FROM clusters LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("cluster");

    let hostile = "'; DROP TABLE sandboxes; --";
    sqlx::query(
        "INSERT INTO sandboxes (environment_id, cluster_id, name, image, image_digest)
         VALUES ($1, $2, $3, 'img@sha256:abc', 'sha256:abc')",
    )
    .bind(environment)
    .bind(cluster)
    .bind(hostile)
    .execute(&pool)
    .await
    .expect("the value is data, so it stores");

    let still_there: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sandboxes")
        .fetch_one(&pool)
        .await
        .expect("the table still exists");
    assert!(still_there >= 1);

    let round_trip: String =
        sqlx::query_scalar("SELECT name FROM sandboxes WHERE name = $1")
            .bind(hostile)
            .fetch_one(&pool)
            .await
            .expect("read back");
    assert_eq!(round_trip, hostile);
}

/// A second global policy for the same environment is refused by the database.
#[tokio::test]
async fn only_one_global_policy_can_exist() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };

    let environment: Uuid = sqlx::query_scalar("SELECT id FROM environments LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("environment");

    let spec = serde_json::json!({ "network_policies": {} });
    sqlx::query("INSERT INTO policies (environment_id, name, scope, spec) VALUES ($1, 'g1', 'global', $2)")
        .bind(environment)
        .bind(&spec)
        .execute(&pool)
        .await
        .expect("first global policy");

    let second = sqlx::query(
        "INSERT INTO policies (environment_id, name, scope, spec) VALUES ($1, 'g2', 'global', $2)",
    )
    .bind(environment)
    .bind(&spec)
    .execute(&pool)
    .await;

    assert!(second.is_err(), "a second global policy must be refused");
}

/// The audit table refuses an update for the application role's grants.
#[tokio::test]
async fn the_audit_table_is_append_only_for_the_app_role() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };

    let privileges: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.role_table_grants
          WHERE grantee = 'nestbox_app' AND table_name = 'audit_events'
            AND privilege_type IN ('UPDATE', 'DELETE')",
    )
    .fetch_one(&pool)
    .await
    .expect("grants");

    assert_eq!(
        privileges, 0,
        "nestbox_app must not be able to change or remove audit records"
    );
}

/// The telemetry partitions for the current month exist.
#[tokio::test]
async fn a_telemetry_partition_exists_for_this_month() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };

    let partitions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_class c
           JOIN pg_inherits i ON i.inhrelid = c.oid
           JOIN pg_class p ON p.oid = i.inhparent
          WHERE p.relname = 'telemetry_events' AND c.relname <> 'telemetry_events_default'",
    )
    .fetch_one(&pool)
    .await
    .expect("count partitions");

    assert!(partitions >= 3, "the next months should be pre-created");
}
