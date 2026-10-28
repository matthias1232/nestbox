//! The Admin Center's traffic reads against a real database.
//!
//! These prove what a pure function cannot: that the row list pages by cursor
//! without scanning, that the window counts and the map edges are aggregated by
//! the database, that a body is only readable inside its own sandbox and
//! environment, that MCP arguments are redacted on the way out, that the
//! deployed badge is a hash comparison, and that one tenant's traffic is
//! invisible to another.
//!
//! The suite owns `nestbox_traffic`, so it can run next to `tests/database.rs`
//! and `tests/recipes.rs` without any of them dropping another's schema:
//!
//!   createdb -h /tmp/opencode/pgtest -p 55432 nestbox_traffic
//!   export TEST_DATABASE_URL='postgres://core@localhost:55432/nestbox_traffic?host=/tmp/opencode/pgtest'
//!   cargo test --test traffic -- --test-threads=1

use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::postgres::PgConnectOptions;
use sqlx::PgPool;
use tokio::sync::OnceCell;
use uuid::Uuid;

use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::traffic_service::{self, McpCallsRequest, TrafficRequest};
use nestbox_control_api::services::ServiceContext;

/// The database this suite owns.
const DATABASE: &str = "nestbox_traffic";

/// The seeded administrator, which every audit trail foreign key accepts.
const ADMIN: &str = "00000000-0000-0000-0000-000000000005";

static READY: OnceCell<bool> = OnceCell::const_new();

/// Create the suite's database once per process, then migrate it.
async fn ready() -> bool {
    *READY
        .get_or_init(|| async {
            let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
                return false;
            };
            let Ok(options) = PgConnectOptions::from_str(&url) else {
                return false;
            };
            let Ok(maintenance) =
                sqlx::PgPool::connect_with(options.clone().database("postgres")).await
            else {
                return false;
            };
            let setup = async {
                sqlx::query(
                    "DO $$BEGIN
                        IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'nestbox_app') THEN
                            CREATE ROLE nestbox_app LOGIN;
                        END IF;
                    END$$",
                )
                .execute(&maintenance)
                .await?;
                sqlx::query(&format!("DROP DATABASE IF EXISTS {DATABASE} WITH (FORCE)"))
                    .execute(&maintenance)
                    .await?;
                sqlx::query(&format!("CREATE DATABASE {DATABASE}"))
                    .execute(&maintenance)
                    .await?;

                let pool = sqlx::PgPool::connect_with(options.database(DATABASE)).await?;
                sqlx::query("GRANT USAGE, CREATE ON SCHEMA public TO nestbox_app")
                    .execute(&pool)
                    .await?;
                sqlx::migrate!("./migrations").run(&pool).await?;
                Ok::<(), sqlx::Error>(())
            }
            .await;
            setup.is_ok()
        })
        .await
}

async fn pool() -> Option<PgPool> {
    if !ready().await {
        return None;
    }
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let options = PgConnectOptions::from_str(&url).ok()?;
    sqlx::PgPool::connect_with(options.database(DATABASE)).await.ok()
}

fn principal(environment_id: Uuid) -> Principal {
    Principal {
        user_id: Uuid::parse_str(ADMIN).expect("a uuid"),
        environment_id,
        role: Role::Admin,
        display_name: "Test administrator".into(),
        via: CredentialSource::Token,
    }
}

fn context(pool: &PgPool, runtime: &Arc<FakeRuntime>) -> ServiceContext {
    ServiceContext::new(
        pool.clone(),
        runtime.clone(),
        Arc::new(nestbox_control_api::events::bus::LocalBus::new()),
        std::path::PathBuf::from("/tmp/opencode"),
    )
}

async fn default_environment(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM environments WHERE slug = 'default'")
        .fetch_one(pool)
        .await
        .expect("the default environment is seeded")
}

/// A second tenant, so isolation is proven against real rows.
async fn other_environment(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO environments (organization_id, name, slug)
         VALUES ('00000000-0000-0000-0000-000000000001', $1, $1)
         RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("insert environment")
}

async fn cluster(pool: &PgPool, environment_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO clusters (environment_id, name, slug) VALUES ($1, $2, $2) RETURNING id",
    )
    .bind(environment_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("insert cluster")
}

async fn node(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO nodes (name, display_name) VALUES ($1, $1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("insert node")
}

async fn sandbox(
    pool: &PgPool,
    environment_id: Uuid,
    cluster_id: Uuid,
    node_id: Uuid,
    name: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO sandboxes
            (environment_id, cluster_id, node_id, name, pack_key, image, spec, status)
         VALUES ($1, $2, $3, $4, 'ubuntu-dev', 'ghcr.io/nestbox/ubuntu-dev@sha256:abc', '{}', 'running')
         RETURNING id",
    )
    .bind(environment_id)
    .bind(cluster_id)
    .bind(node_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("insert sandbox")
}

/// Write one proxy access-log line the way the ingest path stores it.
async fn event(
    pool: &PgPool,
    environment_id: Uuid,
    sandbox_id: Uuid,
    ts: DateTime<Utc>,
    detail: serde_json::Value,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO telemetry_events
            (environment_id, sandbox_id, kind, source, ts, detail)
         VALUES ($1, $2, 'network', 'egress-proxy', $3, $4)
         RETURNING id",
    )
    .bind(environment_id)
    .bind(sandbox_id)
    .bind(ts)
    .bind(detail)
    .fetch_one(pool)
    .await
    .expect("insert event")
}

/// A minimal access-log line; the tests override the fields they care about.
fn access_log(host: &str, port: i32, protocol: &str, decision: &str) -> serde_json::Value {
    json!({
        "host": host,
        "port": port,
        "protocol": protocol,
        "method": "GET",
        "path": "/v1/models",
        "status": 200,
        "duration_ms": 12,
        "bytes_in": 100,
        "bytes_out": 200,
        "decision": decision,
        "reason": "rule",
        "user_agent": "hermes-agent/0.22.5",
        "inspected": true,
    })
}

/// A fresh sandbox of the default tenant, for one test.
async fn test_sandbox(pool: &PgPool, name: &str) -> (Uuid, Uuid) {
    let environment_id = default_environment(pool).await;
    let cluster_id = cluster(pool, environment_id, &format!("c-{name}")).await;
    let node_id = node(pool, &format!("n-{name}")).await;
    let sandbox_id = sandbox(pool, environment_id, cluster_id, node_id, name).await;
    (environment_id, sandbox_id)
}

#[tokio::test]
async fn traffic_pages_by_cursor_and_is_bounded() {
    let Some(pool) = pool().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let (environment_id, sandbox_id) = test_sandbox(&pool, "traffic-paging").await;
    let who = principal(environment_id);

    let now = Utc::now();
    for index in 0..4 {
        event(
            &pool,
            environment_id,
            sandbox_id,
            now - Duration::minutes(4 - index),
            access_log(&format!("host{index}.example.com"), 443, "https", "allow"),
        )
        .await;
    }

    let first = traffic_service::traffic(
        &ctx,
        &who,
        sandbox_id,
        TrafficRequest {
            window: Some("5m".into()),
            filter: None,
            host: None,
            cursor: None,
            limit: Some(2),
        },
    )
    .await
    .expect("first page");

    assert_eq!(first.window, "5m");
    assert_eq!(first.items.len(), 2);
    assert!(first.summary.requests >= 4, "the counts cover the window");
    assert_eq!(first.items[0].host, "host3.example.com", "newest first");
    assert_eq!(first.items[0].agent_hint.as_deref(), Some("hermes"));
    assert!(first.items[0].inspected);
    let cursor = first.next_cursor.clone().expect("a full page has a next cursor");

    let second = traffic_service::traffic(
        &ctx,
        &who,
        sandbox_id,
        TrafficRequest {
            window: Some("5m".into()),
            filter: None,
            host: None,
            cursor: Some(cursor.clone()),
            limit: Some(2),
        },
    )
    .await
    .expect("second page");
    assert_eq!(second.items.len(), 2);
    assert_eq!(second.items[0].host, "host1.example.com");
    assert_eq!(second.items[1].host, "host0.example.com");

    // The third page is empty: a page that was full may still be the last one,
    // which is why the client follows the cursor until it gets nothing.
    let third = traffic_service::traffic(
        &ctx,
        &who,
        sandbox_id,
        TrafficRequest {
            window: Some("5m".into()),
            filter: None,
            host: None,
            cursor: second.next_cursor,
            limit: Some(2),
        },
    )
    .await
    .expect("third page");
    assert!(third.items.is_empty());

    // A limit beyond the store's maximum is clamped, not honoured.
    let wide = traffic_service::traffic(
        &ctx,
        &who,
        sandbox_id,
        TrafficRequest {
            window: Some("48h".into()),
            filter: None,
            host: None,
            cursor: None,
            limit: Some(10_000),
        },
    )
    .await
    .expect("clamped page");
    assert_eq!(wide.limit, 200);
    assert_eq!(wide.window, "24h", "the window is capped, not rejected");
    assert_eq!(wide.items.len(), 4);
}

#[tokio::test]
async fn a_window_that_is_not_a_duration_is_refused() {
    let Some(pool) = pool().await else {
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let (environment_id, sandbox_id) = test_sandbox(&pool, "traffic-bad-window").await;

    let error = traffic_service::traffic(
        &ctx,
        &principal(environment_id),
        sandbox_id,
        TrafficRequest {
            window: Some("5min".into()),
            filter: None,
            host: None,
            cursor: None,
            limit: None,
        },
    )
    .await
    .expect_err("5min is not a window");
    assert_eq!(error.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_filters_narrow_the_rows_without_changing_the_window_counts() {
    let Some(pool) = pool().await else {
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let (environment_id, sandbox_id) = test_sandbox(&pool, "traffic-filters").await;
    let who = principal(environment_id);
    let now = Utc::now();

    let mut allow = access_log("api.omniroute.local", 20128, "https", "allow");
    allow["bytes_in"] = json!(1000);
    allow["bytes_out"] = json!(2000);
    event(&pool, environment_id, sandbox_id, now - Duration::seconds(50), allow).await;

    event(
        &pool,
        environment_id,
        sandbox_id,
        now - Duration::seconds(40),
        access_log("blocked.example.com", 443, "https", "deny"),
    )
    .await;

    let mut mcp = access_log("mcp.example.com", 443, "mcp", "allow");
    mcp["mcp_tool"] = json!("read_file");
    mcp["mcp_method"] = json!("tools/call");
    mcp["mcp_session_id"] = json!("sess-1");
    event(&pool, environment_id, sandbox_id, now - Duration::seconds(30), mcp).await;

    let mut audit = access_log("audited.example.com", 443, "https", "audit");
    audit["reason"] = json!("rule");
    event(&pool, environment_id, sandbox_id, now - Duration::seconds(20), audit).await;

    let request = |filter: &str, host: Option<&str>| {
        let ctx = &ctx;
        let who = &who;
        let filter = filter.to_string();
        let host = host.map(str::to_string);
        async move {
            traffic_service::traffic(
                ctx,
                who,
                sandbox_id,
                TrafficRequest {
                    window: Some("5m".into()),
                    filter: Some(filter),
                    host,
                    cursor: None,
                    limit: None,
                },
            )
            .await
            .expect("page")
        }
    };

    let blocked = request("blocked", None).await;
    assert_eq!(blocked.items.len(), 1);
    assert_eq!(blocked.items[0].host, "blocked.example.com");
    assert_eq!(blocked.summary.requests, 4, "the counts ignore the filter");
    assert_eq!(blocked.summary.denied, 1);
    assert_eq!(blocked.summary.matching, 1);

    let allowed = request("allowed", None).await;
    assert_eq!(allowed.items.len(), 3, "allow and audit both passed");
    assert_eq!(allowed.summary.allowed, 2);
    assert_eq!(allowed.summary.audit, 1);

    let mcp_only = request("mcp", None).await;
    assert_eq!(mcp_only.items.len(), 1);
    assert_eq!(mcp_only.items[0].mcp_tool.as_deref(), Some("read_file"));
    assert_eq!(mcp_only.summary.mcp_requests, 1);

    let by_host = request("all", Some("blocked")).await;
    assert_eq!(by_host.items.len(), 1);
    assert_eq!(by_host.summary.matching, 1);

    let everything = request("all", None).await;
    assert_eq!(everything.summary.bytes_in, 1300);
    assert_eq!(everything.summary.bytes_out, 2600);
    assert_eq!(everything.summary.destinations, 4);
    assert!(everything.summary.protocols.iter().any(|(name, _)| name == "mcp"));
}

#[tokio::test]
async fn the_map_aggregates_per_destination() {
    let Some(pool) = pool().await else {
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let (environment_id, sandbox_id) = test_sandbox(&pool, "traffic-map").await;
    let who = principal(environment_id);
    let now = Utc::now();

    let mut allowed = access_log("api.example.com", 443, "https", "allow");
    allowed["bytes_in"] = json!(500);
    allowed["bytes_out"] = json!(1500);
    event(&pool, environment_id, sandbox_id, now - Duration::seconds(120), allowed).await;

    event(
        &pool,
        environment_id,
        sandbox_id,
        now - Duration::seconds(90),
        access_log("api.example.com", 443, "https", "allow"),
    )
    .await;

    event(
        &pool,
        environment_id,
        sandbox_id,
        now - Duration::seconds(60),
        access_log("api.example.com", 443, "https", "deny"),
    )
    .await;

    let mut mcp = access_log("api.example.com", 443, "mcp", "allow");
    mcp["mcp_tool"] = json!("search");
    event(&pool, environment_id, sandbox_id, now - Duration::seconds(30), mcp).await;

    event(
        &pool,
        environment_id,
        sandbox_id,
        now - Duration::seconds(20),
        access_log("pypi.org", 443, "https", "allow"),
    )
    .await;

    let map = traffic_service::map(&ctx, &who, sandbox_id, Some("5m"), None)
        .await
        .expect("map");

    assert_eq!(map.window, "5m");
    // An edge is a destination *and* a protocol: the MCP edge to the same host
    // is its own edge, which is what lets the map draw the tool chain apart from
    // the plain HTTPS traffic.
    assert_eq!(map.edges.len(), 3);
    assert_eq!(map.edges[0].host, "api.example.com");
    assert_eq!(map.edges[0].protocol, "https");
    assert_eq!(map.edges[0].requests, 3);
    assert_eq!(map.edges[0].allowed, 2);
    assert_eq!(map.edges[0].denied, 1);
    assert!(map.edges[0].bytes >= 2000);
    let mcp_edge = map
        .edges
        .iter()
        .find(|edge| edge.protocol == "mcp")
        .expect("the MCP edge");
    assert_eq!(mcp_edge.host, "api.example.com");
    assert_eq!(mcp_edge.requests, 1);
    assert!(mcp_edge.tools.contains(&"search".to_string()));
    assert_eq!(map.totals.requests, 5);
    assert_eq!(map.totals.denied, 1);
    assert_eq!(map.totals.destinations, 3);
    assert_eq!(map.totals.blocked_destinations, 1, "one red edge");
}

#[tokio::test]
async fn a_body_is_reachable_inside_its_sandbox_only_and_arrives_redacted() {
    let Some(pool) = pool().await else {
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let (environment_id, sandbox_id) = test_sandbox(&pool, "traffic-body").await;
    let who = principal(environment_id);
    let now = Utc::now();

    let event_id = event(
        &pool,
        environment_id,
        sandbox_id,
        now - Duration::seconds(10),
        access_log("api.example.com", 443, "https", "allow"),
    )
    .await;

    // Stored unredacted on purpose: the read path must be the second boundary,
    // not a hope that the write path was the first.
    sqlx::query(
        "INSERT INTO traffic_bodies
            (event_id, environment_id, sandbox_id, ts, request_body, response_body,
             request_bytes, response_bytes, truncated)
         VALUES ($1, $2, $3, $4, $5, $6, 64, 65536, true)",
    )
    .bind(event_id)
    .bind(environment_id)
    .bind(sandbox_id)
    .bind(now - Duration::seconds(10))
    .bind(r#"{"password":"hunter2","model":"gpt"}"#)
    .bind(r#"{"id":"chatcmpl-1"}"#)
    .execute(&pool)
    .await
    .expect("insert body");

    let body = traffic_service::body(&ctx, &who, sandbox_id, event_id)
        .await
        .expect("body");
    assert!(body.truncated, "the truncation is visible, not implied");
    assert!(body.redacted);
    let request = body.request_body.expect("request body");
    assert!(request.contains("[redacted]"), "{request}");
    assert!(!request.contains("hunter2"), "{request}");
    assert_eq!(body.request_bytes, 64);
    assert_eq!(body.response_bytes, 65536);
    assert_eq!(
        body.retained_until,
        body.captured_at + Duration::hours(24),
        "the console shows the day it lives"
    );

    // A row without a body is a 404, not an empty object.
    let missing = traffic_service::body(&ctx, &who, sandbox_id, Uuid::new_v4())
        .await
        .expect_err("no body");
    assert_eq!(missing.status, axum::http::StatusCode::NOT_FOUND);

    // The list says which rows have one, in one bounded query per page.
    let page = traffic_service::traffic(
        &ctx,
        &who,
        sandbox_id,
        TrafficRequest {
            window: Some("5m".into()),
            filter: None,
            host: None,
            cursor: None,
            limit: None,
        },
    )
    .await
    .expect("page");
    let row = page
        .items
        .iter()
        .find(|row| row.id == event_id)
        .expect("the row");
    assert!(row.has_body);
}

#[tokio::test]
async fn mcp_calls_carry_tool_session_duration_and_redacted_arguments() {
    let Some(pool) = pool().await else {
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let (environment_id, sandbox_id) = test_sandbox(&pool, "traffic-mcp").await;
    let who = principal(environment_id);
    let now = Utc::now();

    let calls = [(0, "read_file", 12), (1, "read_file", 30), (2, "write_file", 7)];
    for (index, tool, duration) in calls {
        sqlx::query(
            "INSERT INTO mcp_tool_calls
                (environment_id, sandbox_id, ts, server_host, server_key, session_id,
                 method, tool_name, arguments, result, is_error, duration_ms)
             VALUES ($1, $2, $3, 'mcp-hub', 'nestbox-hub', 'sess-1', 'tools/call', $4,
                     $5, '{}'::jsonb, false, $6)",
        )
        .bind(environment_id)
        .bind(sandbox_id)
        .bind(now - Duration::seconds(60 - index * 10))
        .bind(tool)
        .bind(json!({ "api_key": "sk-live-should-not-appear" }))
        .bind(duration)
        .execute(&pool)
        .await
        .expect("insert mcp call");
    }

    let all = traffic_service::mcp_calls(
        &ctx,
        &who,
        sandbox_id,
        McpCallsRequest {
            window: Some("5m".into()),
            host: None,
            tool: None,
            cursor: None,
            limit: None,
        },
    )
    .await
    .expect("mcp calls");
    assert_eq!(all.items.len(), 3);
    assert_eq!(all.items[0].tool_name.as_deref(), Some("write_file"));
    assert_eq!(all.items[0].session_id.as_deref(), Some("sess-1"));
    assert_eq!(all.items[0].duration_ms, Some(7));
    let arguments = all.items[0].arguments.as_ref().expect("arguments");
    assert!(
        !arguments.to_string().contains("sk-live-should-not-appear"),
        "the arguments leave redacted: {arguments}"
    );

    let only_reads = traffic_service::mcp_calls(
        &ctx,
        &who,
        sandbox_id,
        McpCallsRequest {
            window: Some("5m".into()),
            host: None,
            tool: Some("read_file".into()),
            cursor: None,
            limit: Some(1),
        },
    )
    .await
    .expect("filtered calls");
    assert_eq!(only_reads.items.len(), 1);
    assert_eq!(only_reads.items[0].tool_name.as_deref(), Some("read_file"));
    assert!(only_reads.next_cursor.is_some(), "a full page has a cursor");
}

#[tokio::test]
async fn effective_policies_name_their_source_rank_and_deployment() {
    let Some(pool) = pool().await else {
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let environment_id = default_environment(&pool).await;
    let cluster_id = cluster(&pool, environment_id, "c-policies").await;
    let node_id = node(&pool, "n-policies").await;
    let sandbox_id = sandbox(&pool, environment_id, cluster_id, node_id, "traffic-policies").await;
    let who = principal(environment_id);

    let policy_id: Uuid = sqlx::query_scalar(
        "INSERT INTO policies (environment_id, name, scope, spec)
         VALUES ($1, 'global-egress', 'global', '{\"network_policies\":{}}'::jsonb)
         RETURNING id",
    )
    .bind(environment_id)
    .fetch_one(&pool)
    .await
    .expect("insert policy");

    // A sandbox-scoped rule, an environment-wide rule from the global policy,
    // and a disabled rule that must not be listed as in force.
    sqlx::query(
        "INSERT INTO network_rules
            (environment_id, policy_id, sandbox_id, name, action, host, port, protocol,
             enforcement, priority, enabled)
         VALUES
            ($1, NULL, $2, 'allow omniroute', 'allow', '10.0.92.30', 20128, 'rest', 'enforced', 10, true),
            ($1, $3, NULL, 'deny everything else', 'deny', '*', NULL, 'tcp', 'audit', 20, true),
            ($1, NULL, $2, 'old rule', 'deny', 'legacy.example.com', 443, 'rest', 'enforced', 5, false)",
    )
    .bind(environment_id)
    .bind(sandbox_id)
    .bind(policy_id)
    .execute(&pool)
    .await
    .expect("insert rules");

    sqlx::query(
        "INSERT INTO traffic_windows (environment_id, sandbox_id, reason, expires_at)
         VALUES ($1, $2, 'debugging', now() + interval '15 minutes')",
    )
    .bind(environment_id)
    .bind(sandbox_id)
    .execute(&pool)
    .await
    .expect("insert window");

    sqlx::query(
        "INSERT INTO policy_compilations (environment_id, hash, rule_count, window_count)
         VALUES ($1, 'aaaaaaaaaaaa1111', 2, 1)
         ON CONFLICT (environment_id) DO UPDATE SET hash = EXCLUDED.hash",
    )
    .bind(environment_id)
    .execute(&pool)
    .await
    .expect("insert compilation");

    sqlx::query("UPDATE nodes SET policy_hash_reported = 'aaaaaaaaaaaa1111' WHERE id = $1")
        .bind(node_id)
        .execute(&pool)
        .await
        .expect("report the hash");

    let deployed = traffic_service::effective_policies(&ctx, &who, sandbox_id)
        .await
        .expect("effective policies");

    assert_eq!(deployed.rules.len(), 2, "the disabled rule is not in force");
    assert_eq!(deployed.rules[0].priority, 10, "lowest priority number first");
    assert_eq!(deployed.rules[0].name, "allow omniroute");
    assert_eq!(deployed.rules[0].scope, "sandbox");
    assert_eq!(deployed.rules[0].policy, None);
    assert_eq!(deployed.rules[1].scope, "global");
    assert_eq!(deployed.rules[1].policy.as_deref(), Some("global-egress"));
    assert_eq!(deployed.rules[1].enforcement, "audit");
    assert_eq!(deployed.windows.len(), 1);
    assert_eq!(deployed.windows[0].reason, "debugging");
    assert!(deployed.deployment.deployed);
    assert_eq!(deployed.deployment.rule_count, 2);

    // A node still on an older rule set is yellow, and the sentence says why.
    sqlx::query("UPDATE nodes SET policy_hash_reported = 'bbbbbbbbbbbb2222' WHERE id = $1")
        .bind(node_id)
        .execute(&pool)
        .await
        .expect("report another hash");
    let stale = traffic_service::effective_policies(&ctx, &who, sandbox_id)
        .await
        .expect("effective policies");
    assert!(!stale.deployment.deployed);
    assert!(stale.deployment.explanation.contains("older rule set"));

    // A node that has reported nothing is not deployed either.
    sqlx::query("UPDATE nodes SET policy_hash_reported = NULL WHERE id = $1")
        .bind(node_id)
        .execute(&pool)
        .await
        .expect("clear the hash");
    let silent = traffic_service::effective_policies(&ctx, &who, sandbox_id)
        .await
        .expect("effective policies");
    assert!(!silent.deployment.deployed);
    assert!(silent.deployment.explanation.contains("has not reported"));
}

#[tokio::test]
async fn one_tenants_traffic_is_invisible_to_another() {
    let Some(pool) = pool().await else {
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);

    let (mine_env, my_sandbox) = test_sandbox(&pool, "traffic-mine").await;

    let their_env = other_environment(&pool, "traffic-other").await;
    let their_cluster = cluster(&pool, their_env, "c-other").await;
    let their_node = node(&pool, "n-other").await;
    let their_sandbox = sandbox(&pool, their_env, their_cluster, their_node, "their-box").await;

    let now = Utc::now();
    event(
        &pool,
        mine_env,
        my_sandbox,
        now - Duration::seconds(30),
        access_log("mine.example.com", 443, "https", "allow"),
    )
    .await;
    let their_event = event(
        &pool,
        their_env,
        their_sandbox,
        now - Duration::seconds(20),
        access_log("theirs.example.com", 443, "https", "deny"),
    )
    .await;

    let who = principal(mine_env);

    // My own traffic is what I see.
    let mine = traffic_service::traffic(
        &ctx,
        &who,
        my_sandbox,
        TrafficRequest {
            window: Some("5m".into()),
            filter: None,
            host: None,
            cursor: None,
            limit: None,
        },
    )
    .await
    .expect("my page");
    assert_eq!(mine.items.len(), 1);
    assert_eq!(mine.items[0].host, "mine.example.com");

    // Their sandbox is not "empty"; it does not exist as far as I am concerned.
    let foreign = traffic_service::traffic(
        &ctx,
        &who,
        their_sandbox,
        TrafficRequest {
            window: Some("5m".into()),
            filter: None,
            host: None,
            cursor: None,
            limit: None,
        },
    )
    .await
    .expect_err("another tenant's sandbox");
    assert_eq!(foreign.status, axum::http::StatusCode::NOT_FOUND);
    assert_eq!(foreign.code, nestbox_control_api::error::ErrorCode::SANDBOX_NOT_FOUND);

    // Their event's body is not reachable through my sandbox either.
    let body = traffic_service::body(&ctx, &who, my_sandbox, their_event)
        .await
        .expect_err("another tenant's body");
    assert_eq!(body.status, axum::http::StatusCode::NOT_FOUND);

    // Their rules are not listed under my sandbox.
    let policies = traffic_service::effective_policies(&ctx, &who, their_sandbox)
        .await
        .expect_err("another tenant's policies");
    assert_eq!(policies.status, axum::http::StatusCode::NOT_FOUND);

    // The environment-wide graph carries my edges and not theirs.
    let live = traffic_service::live_edges(&ctx, &who, Some("5m"))
        .await
        .expect("live edges");
    let hosts: Vec<&str> = live.edges.iter().map(|edge| edge.host.as_str()).collect();
    assert!(hosts.contains(&"mine.example.com"));
    assert!(!hosts.contains(&"theirs.example.com"));
    assert!(live.sandboxes.iter().all(|entry| entry.id != their_sandbox));
}

#[tokio::test]
async fn lives_edges_cover_the_environment_and_flag_truncation_honestly() {
    let Some(pool) = pool().await else {
        return;
    };
    let runtime = Arc::new(FakeRuntime::new());
    let ctx = context(&pool, &runtime);
    let environment_id = default_environment(&pool).await;
    let cluster_id = cluster(&pool, environment_id, "c-live").await;
    let node_id = node(&pool, "n-live").await;
    let first = sandbox(&pool, environment_id, cluster_id, node_id, "live-a").await;
    let second = sandbox(&pool, environment_id, cluster_id, node_id, "live-b").await;
    let who = principal(environment_id);
    let now = Utc::now();

    event(
        &pool,
        environment_id,
        first,
        now - Duration::seconds(30),
        access_log("shared.example.com", 443, "https", "allow"),
    )
    .await;
    event(
        &pool,
        environment_id,
        second,
        now - Duration::seconds(20),
        access_log("shared.example.com", 443, "https", "deny"),
    )
    .await;

    let live = traffic_service::live_edges(&ctx, &who, Some("5m"))
        .await
        .expect("live edges");
    assert_eq!(live.window, "5m");
    assert!(!live.truncated);
    assert!(live.edges.len() >= 2);
    // Other tests in this database leave recent events behind, so the graph is
    // checked for the two sandboxes it must contain, not for an exact size.
    assert!(live.sandboxes.iter().any(|entry| entry.id == first));
    assert!(live.sandboxes.iter().any(|entry| entry.id == second));
    assert_eq!(
        live.sandboxes
            .iter()
            .filter(|entry| entry.id == first || entry.id == second)
            .count(),
        2,
        "one entry per sandbox, not per edge"
    );
    let blocked = live
        .edges
        .iter()
        .find(|edge| edge.sandbox_id == second)
        .expect("the denied edge");
    assert_eq!(blocked.denied, 1);
    assert_eq!(blocked.host, "shared.example.com");
}
