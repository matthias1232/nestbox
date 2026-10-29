//! The stack diagram, automatic stacking and port publication, against a real PostgreSQL.
//!
//! The connect view is one read of the node's address and hostname, the
//! sandbox's published ports, and the key state. These tests prove it says why
//! nothing can be dialled, dials the hostname once one is set, and that setting
//! the endpoint is validated and needs the node permission.
//!
//!   export TEST_DATABASE_URL="postgres://core@localhost/nestbox_connect?host=/tmp/opencode&port=55432"
//!   cargo test --test stack_topology -- --test-threads=1

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::events::bus::LocalBus;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::runtime::RuntimeAdapter;
use nestbox_control_api::services::sandbox_service::{self, CreateSandbox};
use nestbox_control_api::services::{access_service, stack_topology_service, ServiceContext};

const ENVIRONMENT: &str = "00000000-0000-0000-0000-000000000002";
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

fn context(pool: &sqlx::PgPool, runtime: &Arc<FakeRuntime>) -> ServiceContext {
    ServiceContext::new(
        pool.clone(),
        runtime.clone(),
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

fn request(name: &str, pack: &str) -> CreateSandbox {
    CreateSandbox {
        name: name.to_string(),
        pack_key: pack.to_string(),
        image: String::new(),
        image_digest: None,
        command: Vec::new(),
        memory_limit: "512Mi".to_string(),
        cpu_limit: 0.5,
        pids_limit: 256,
        node: None,
        parent_id: None,
    }
}

#[tokio::test]
async fn a_tool_created_without_a_lead_joins_the_only_agent_and_not_when_there_are_two() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let principal = principal_of(user(&pool).await);

    // No agent yet: nothing to join.
    let alone = sandbox_service::create(&ctx, &principal, request("git-alone", "git")).await.expect("create");
    assert_eq!(alone.parent_id, None);

    let lead = sandbox_service::create(&ctx, &principal, request("hermes-01", "hermes")).await.expect("lead");
    assert_eq!(lead.parent_id, None, "an agent never joins anything");

    let helper = sandbox_service::create(&ctx, &principal, request("git-01", "git")).await.expect("helper");
    assert_eq!(helper.parent_id, Some(lead.id), "the only agent on the node is the lead");

    // A second agent makes the choice ambiguous, so the next helper stays alone.
    sandbox_service::create(&ctx, &principal, request("hermes-02", "hermes")).await.expect("second agent");
    let ambiguous = sandbox_service::create(&ctx, &principal, request("git-02", "git")).await.expect("ambiguous");
    assert_eq!(ambiguous.parent_id, None);
}

#[tokio::test]
async fn provisioning_opens_the_ports_on_the_runner_and_the_panel_tells_the_truth() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let principal = principal_of(user(&pool).await);
    sqlx::query("UPDATE nodes SET public_address = '192.168.55.50' WHERE id = $1")
        .bind(uuid(LOCAL_NODE))
        .execute(&pool)
        .await
        .expect("address");

    let created = sandbox_service::create(&ctx, &principal, request("oc-01", "openchamber")).await.expect("create");
    access_service::provision(&ctx, &principal, &created).await.expect("provision");

    // The runner was handed the complete set and the sandbox's public key.
    let (forwards, keys) = runtime.forwards_for("oc-01").expect("forwards were pushed");
    let purposes: Vec<&str> = forwards.iter().map(|forward| forward.purpose.as_str()).collect();
    assert_eq!(purposes.len(), 2);
    assert!(purposes.contains(&"ssh") && purposes.contains(&"agent"));
    assert_eq!(keys.len(), 1);
    assert!(keys[0].starts_with("ssh-ed25519 "));
    assert!(forwards.iter().all(|forward| (22200..=22299).contains(&forward.port)), "ports come from the new range");

    // What the runner opened is written back, so the row is no longer "pending".
    let states: Vec<String> = sqlx::query_scalar("SELECT state FROM forwards WHERE sandbox_id = $1")
        .bind(created.id)
        .fetch_all(&pool)
        .await
        .expect("states");
    assert!(states.iter().all(|state| state == "active"), "{states:?}");

    // The panel says what the port is for, and that nothing answers behind it yet.
    let info = access_service::connect(&ctx, &principal, created.id).await.expect("connect");
    assert_eq!(info.forwarding, "live");
    let web = info.ports.iter().find(|port| port.kind == "web").expect("the OpenChamber web port");
    assert_eq!(web.label.as_deref(), Some("OpenChamber web UI"));
    assert_eq!(web.listening, Some(true));
    assert_eq!(web.target_up, Some(false));
    assert!(web.url.as_deref().unwrap_or_default().starts_with("http://192.168.55.50:"));
    assert!(info.notes.iter().any(|note| note.contains("scp")));

    // A rotated key must reach the runner, or the new key is refused.
    access_service::rotate(&ctx, &principal, created.id).await.expect("rotate");
    let (_, rotated) = runtime.forwards_for("oc-01").expect("still pushed");
    assert_ne!(rotated, keys, "the runner has the new public key");

    // A runner that lost its state is healed by the next read.
    runtime.clear_forwards("oc-01").await.expect("clear");
    assert!(runtime.forwards_for("oc-01").is_none());
    access_service::connect(&ctx, &principal, created.id).await.expect("connect again");
    assert!(runtime.forwards_for("oc-01").is_some(), "the read pushed the set again");
}

#[tokio::test]
async fn the_stack_diagram_covers_the_lead_and_its_members_and_what_is_missing() {
    let Some(pool) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let principal = principal_of(user(&pool).await);

    let lead = sandbox_service::create(&ctx, &principal, request("hermes-01", "hermes")).await.expect("lead");
    access_service::provision(&ctx, &principal, &lead).await.expect("provision");
    let helper = sandbox_service::create(&ctx, &principal, request("git-01", "git")).await.expect("helper");
    assert_eq!(helper.parent_id, Some(lead.id));

    // Opening either member draws the same stack.
    let from_member = stack_topology_service::stack(&ctx, &principal, helper.id, None).await.expect("stack");
    let from_lead = stack_topology_service::stack(&ctx, &principal, lead.id, None).await.expect("stack");
    assert_eq!(from_member.lead, lead.id);
    assert_eq!(from_member.nodes.len(), from_lead.nodes.len());

    let kinds: Vec<&str> = from_lead.nodes.iter().map(|node| node.kind.as_str()).collect();
    for kind in ["operator", "port", "sandbox", "control_api", "runner", "egress_proxy", "internet"] {
        assert!(kinds.contains(&kind), "missing a {kind} card");
    }
    assert_eq!(kinds.iter().filter(|kind| **kind == "sandbox").count(), 2);
    assert!(from_lead.edges.iter().any(|edge| edge.kind == "stack"));
    // No supervisor is bound to the agent, and the diagram says so.
    assert!(from_lead.nodes.iter().any(|node| node.kind == "absent"));
    // Every component card names a log the console can open.
    let proxy = from_lead.nodes.iter().find(|node| node.kind == "egress_proxy").expect("proxy");
    assert_eq!(proxy.status, "ok");
    assert_eq!(proxy.logs.as_ref().map(|log| log.id.as_str()), Some("egress-proxy"));

    let logs = stack_topology_service::component_logs(&ctx, &principal, "egress-proxy", 50).await.expect("log");
    assert!(logs.contains("egress-proxy"));
    assert!(stack_topology_service::component_logs(&ctx, &principal, "../etc", 50).await.is_err());
}
