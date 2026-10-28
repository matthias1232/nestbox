//! HTTP: parsing, validation, delegation, and responses.
//!
//! Nothing here contains a rule or a query. A handler reads the request, asks a
//! service, and formats the answer. If a handler grows an `if` that decides
//! something about the domain, that `if` belongs in `services/`.

pub mod access;
pub mod auth;
pub mod cluster;
pub mod events;
pub mod inventory;
pub mod jobs;
pub mod mcp;
pub mod metrics;
pub mod models;
pub mod nodes;
pub mod packs;
pub mod persistent_roots;
pub mod policies;
pub mod principal;
pub mod providers;
pub mod recipes;
pub mod sandbox_agents;
pub mod sandboxes;
pub mod sandboxes_settings;
pub mod sandboxes_terminal;
pub mod system;
pub mod telemetry;
pub mod topology;
pub mod traffic;

use axum::Router;
use axum::routing::{any, delete, get, patch, post, put};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::security::headers;
use crate::state::AppState;

/// Build the versioned router.
///
/// The `/api/v1/` prefix is frozen (ADR 0015): a breaking change gets a new
/// prefix rather than silently altering this one.
pub fn router(state: AppState) -> Router {
    let mut router = Router::new()
        // System and documentation
        .route("/api/v1/healthz", get(system::healthz))
        .route("/api/v1/readyz", get(system::readyz))
        .route("/api/v1/runtime", get(system::runtime_info))
        .route("/api/v1/openapi.json", get(system::openapi))
        .route("/api/v1/docs", get(system::docs))
        .route("/api/schema/v1", get(system::openapi))
        // Authentication
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/auth/me", get(auth::me))
        // Environments
        .route("/api/v1/environments", get(system::environments))
        .route("/api/v1/environments/current", get(system::current_environment))
        // Sandboxes
        .route(
            "/api/v1/sandboxes",
            get(sandboxes::list).post(sandboxes::create),
        )
        .route(
            "/api/v1/sandboxes/{id}",
            get(sandboxes::get).delete(sandboxes::delete),
        )
        .route("/api/v1/sandboxes/{id}/actions", post(sandboxes::action))
        .route("/api/v1/sandboxes/{id}/logs", get(sandboxes::logs))
        .route("/api/v1/sandboxes/{id}/exec", post(sandboxes::exec))
        .route("/api/v1/sandboxes/{id}/inference", get(telemetry::sandbox_inference))
        .route("/api/v1/sandboxes/{id}/rules", get(policies::sandbox_rules))
        // Which sandboxes belong together.
        .route("/api/v1/sandboxes/{id}/stack", put(sandboxes::put_stack))
        // Admin Center: resource history, processes, inventory
        .route(
            "/api/v1/internal/metrics/ingest",
            post(metrics::ingest),
        )
        .route("/api/v1/sandboxes/{id}/metrics", get(metrics::metrics))
        .route("/api/v1/sandboxes/{id}/processes", get(metrics::processes))
        .route("/api/v1/sandboxes/{id}/inventory", get(inventory::get))
        .route(
            "/api/v1/sandboxes/{id}/inventory/refresh",
            post(inventory::refresh),
        )
        // Admin Center: terminal, SSH, ports, sessions
        .route(
            "/api/v1/sandboxes/{id}/terminal",
            get(sandboxes_terminal::terminal),
        )
        .route(
            "/api/v1/sandboxes/{id}/settings",
            get(sandboxes_settings::get).put(sandboxes_settings::update),
        )
        .route(
            "/api/v1/sandboxes/{id}/supervisor/actions",
            post(sandboxes_settings::action),
        )
        .route("/api/v1/sandboxes/{id}/connect", get(access::connect))
        .route("/api/v1/nodes/{id}/endpoint", put(access::put_node_endpoint))
        .route("/api/v1/sandboxes/{id}/ssh-key", get(access::ssh_key))
        .route(
            "/api/v1/sandboxes/{id}/ssh-key/download",
            get(access::download_ssh_key),
        )
        .route(
            "/api/v1/sandboxes/{id}/ssh-key/rotate",
            post(access::rotate_ssh_key),
        )
        .route(
            "/api/v1/sandboxes/{id}/ports",
            get(access::ports).post(access::add_port),
        )
        .route(
            "/api/v1/sandboxes/{id}/ports/{forward_id}",
            delete(access::remove_port),
        )
        .route("/api/v1/sandboxes/{id}/sessions", get(access::sessions))
        // Admin Center: traffic, map, effective policies, MCP calls
        .route("/api/v1/sandboxes/{id}/traffic", get(traffic::traffic))
        .route(
            "/api/v1/sandboxes/{id}/traffic/{event_id}/body",
            get(traffic::body),
        )
        .route("/api/v1/sandboxes/{id}/map", get(traffic::map))
        .route(
            "/api/v1/sandboxes/{id}/policies/effective",
            get(traffic::effective_policies),
        )
        .route(
            "/api/v1/sandboxes/{id}/mcp/calls",
            get(traffic::mcp_calls),
        )
        // MCP registry and sandbox attachments
        .route(
            "/api/v1/mcp/servers",
            get(mcp::list_servers).post(mcp::create_server),
        )
        .route(
            "/api/v1/mcp/servers/{key}",
            get(mcp::get_server)
                .put(mcp::update_server)
                .delete(mcp::delete_server),
        )
        .route(
            "/api/v1/sandboxes/{id}/mcp/servers",
            get(mcp::sandbox_servers).post(mcp::attach_server),
        )
        .route(
            "/api/v1/sandboxes/{id}/mcp/servers/{key}",
            delete(mcp::detach_server),
        )
        .route(
            "/api/v1/sandboxes/{id}/mcp/servers/{key}/discover",
            post(mcp::discover),
        )
        // Internal: node-token endpoints for the proxy and the runner.
        //
        // The policy check is registered twice and accepts any method, for one
        // reason: Envoy's ext_authz HTTP filter forwards the original method and
        // appends the original path to its `path_prefix`. So the proxy calls
        // `/internal/policy/check/<the path it is asking about>` with `GET` when
        // the agent used `GET`. The handler reads headers, not a body, and
        // ignores the suffix; without both routes every request would be a 404
        // and therefore (fail closed) a deny.
        .route(
            "/api/v1/internal/policy/check",
            any(policies::internal_check),
        )
        .route(
            "/api/v1/internal/policy/check/{*suffix}",
            any(policies::internal_check),
        )
        // Envoy appends the original path to the prefix; for a request to `/`
        // that is a trailing slash, which the wildcard route does not match.
        // Without this route the proxy's very first plain-HTTP request would be
        // a 404 — a deny that never asked a question.
        .route(
            "/api/v1/internal/policy/check/",
            any(policies::internal_check),
        )
        .route(
            "/api/v1/internal/telemetry/batch",
            post(telemetry::internal_batch),
        )
        // Policies
        .route("/api/v1/policies", get(policies::list).post(policies::create))
        .route("/api/v1/policies/global", get(policies::global))
        .route(
            "/api/v1/policies/{id}",
            get(policies::get).put(policies::update).delete(policies::delete),
        )
        .route("/api/v1/policies/{id}/diff", get(policies::diff))
        // The preset library, the editor's schema, and a sandbox's policy
        .route(
            "/api/v1/policy-templates",
            get(policies::list_templates).post(policies::create_template),
        )
        .route(
            "/api/v1/policy-templates/{id}/instantiate",
            post(policies::instantiate_template),
        )
        .route("/api/v1/policy-schema", get(policies::schema))
        .route(
            "/api/v1/sandboxes/{id}/policy",
            get(policies::get_sandbox_policy)
                .put(policies::put_sandbox_policy)
                .delete(policies::delete_sandbox_policy),
        )
        // Network rules
        .route(
            "/api/v1/network-rules",
            get(policies::list_rules).post(policies::create_rule),
        )
        .route("/api/v1/network-rules/{id}", delete(policies::delete_rule))
        // DNS rules: which names the proxy may resolve.
        .route(
            "/api/v1/dns-rules",
            get(policies::list_dns_rules).post(policies::create_dns_rule),
        )
        .route("/api/v1/dns-rules/{id}", delete(policies::delete_dns_rule))
        // How deeply one sandbox is observed on the way out.
        .route(
            "/api/v1/sandboxes/{id}/egress-mode",
            get(policies::get_egress_mode).put(policies::put_egress_mode),
        )
        // Rule drafts and temporary windows
        .route(
            "/api/v1/rule-drafts",
            get(policies::list_drafts).post(policies::draft_from_event),
        )
        .route("/api/v1/rule-drafts/{id}/approve", post(policies::approve_draft))
        .route("/api/v1/rule-drafts/{id}/reject", post(policies::reject_draft))
        .route("/api/v1/traffic-windows", post(policies::open_window))
        .route(
            "/api/v1/traffic-windows/{id}",
            delete(policies::close_window),
        )
        // Telemetry
        .route("/api/v1/telemetry/events", get(telemetry::history))
        .route("/api/v1/telemetry/facets", get(telemetry::facets))
        .route("/api/v1/telemetry/stream", get(events::telemetry_stream))
        .route("/api/v1/telemetry/ingest", post(telemetry::ingest))
        .route("/api/v1/telemetry/seed-demo", post(telemetry::seed_demo))
        .route("/api/v1/events/stream", get(events::state_stream))
        // Cluster
        .route("/api/v1/gateways", get(cluster::list_gateways))
        .route("/api/v1/gateways/{id}", get(cluster::get_gateway))
        .route(
            "/api/v1/gateways/{id}/settings",
            put(cluster::update_settings),
        )
        .route("/api/v1/gateways/{id}/sync", post(cluster::sync))
        .route(
            "/api/v1/supervisors",
            get(cluster::supervisors).post(cluster::create_supervisor),
        )
        .route(
            "/api/v1/supervisors/{id}",
            patch(cluster::bind_supervisor),
        )
        .route(
            "/api/v1/internal/supervisors/{id}/report",
            post(cluster::report_supervisor),
        )
        .route("/api/v1/topology", get(topology::topology))
        // Providers
        .route(
            "/api/v1/providers",
            get(providers::list).post(providers::create),
        )
        .route(
            "/api/v1/providers/{id}",
            get(providers::get)
                .put(providers::update)
                .delete(providers::delete),
        )
        .route("/api/v1/providers/{id}/probe", post(providers::probe))
        // Models
        .route("/api/v1/models", get(models::catalog))
        .route("/api/v1/models/installed", get(models::installed))
        .route("/api/v1/models/hardware", get(models::hardware))
        .route("/api/v1/models/facets", get(models::facets))
        .route("/api/v1/models/requirements", get(models::requirements))
        .route("/api/v1/models/installs", get(models::installs))
        .route("/api/v1/models/sync", post(models::sync))
        .route("/api/v1/models/{key}/install", post(models::install))
        .route("/api/v1/models/{key}/uninstall", post(models::uninstall))
        .route("/api/v1/models/{key}/install", patch(models::update_install))
        // Nodes
        .route("/api/v1/nodes", get(nodes::list))
        .route("/api/v1/nodes/{id}", get(nodes::get))
        .route("/api/v1/nodes/{id}/sandboxes", get(nodes::sandboxes))
        .route("/api/v1/nodes/{id}/display", put(nodes::update_display))
        .route("/api/v1/nodes/{id}/probe", post(nodes::probe))
        // Builder catalogs
        .route("/api/v1/baselines", get(packs::baselines))
        .route("/api/v1/agent-packs", get(packs::agent_packs))
        .route("/api/v1/tool-catalog", get(packs::tool_catalog))
        // Recipes
        .route("/api/v1/recipes", get(recipes::list).post(recipes::create))
        .route(
            "/api/v1/recipes/{id}",
            get(recipes::get).patch(recipes::update).delete(recipes::delete),
        )
        .route("/api/v1/recipes/{id}/build", post(recipes::build))
        .route("/api/v1/recipes/{id}/instantiate", post(recipes::instantiate))
        // Persistent roots
        .route(
            "/api/v1/persistent-roots",
            get(persistent_roots::list).post(persistent_roots::create),
        )
        .route(
            "/api/v1/persistent-roots/{id}",
            get(persistent_roots::get).delete(persistent_roots::delete),
        )
        .route("/api/v1/persistent-roots/{id}/mount", post(persistent_roots::mount))
        .route("/api/v1/persistent-roots/{id}/dismount", post(persistent_roots::dismount))
        .route("/api/v1/persistent-roots/{id}/usage", get(persistent_roots::usage))
        // Sandbox agents
        .route(
            "/api/v1/sandboxes/{id}/agents",
            get(sandbox_agents::list).post(sandbox_agents::add),
        )
        .route(
            "/api/v1/sandboxes/{id}/agents/{instance}",
            delete(sandbox_agents::remove),
        )
        // Jobs and schedules
        .route("/api/v1/jobs", get(jobs::list))
        .route("/api/v1/jobs/{id}", get(jobs::get))
        .route("/api/v1/jobs/{id}/logs", get(jobs::logs))
        .route("/api/v1/schedules", get(jobs::schedules))
        .route("/api/v1/schedules/{id}/run", post(jobs::run_schedule))
        // Audit
        .route("/api/v1/audit-events", get(system::audit_events))
        .with_state(state.clone());

    // Security headers on every response, including errors and streams. A CSP
    // that appears only on success is not a CSP.
    for layer in headers::security_headers() {
        router = router.layer(layer);
    }
    if state.config.session_cookie_secure {
        router = router.layer(headers::hsts_header());
    }

    // A body limit is the cheapest defence against a request that tries to
    // exhaust memory before any handler sees it.
    router = router.layer(RequestBodyLimitLayer::new(state.config.body_limit_bytes));

    router.layer(TraceLayer::new_for_http())
}
