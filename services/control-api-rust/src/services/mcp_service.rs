//! MCP servers: the registry, a sandbox's attachment to them, and discovery.
//!
//! Transport decides visibility, and this module has to say so honestly rather
//! than imply coverage it does not have: an `http` or `sse` server crosses the
//! egress proxy, so its calls appear in traffic with a tool name; a `stdio`
//! server is a child process inside the container and is not egress. The
//! console shows the difference, so "no calls" means "no calls", not "we cannot
//! see this kind of server" (ADR 0026).
//!
//! # Discovery is egress too
//!
//! `tools/list` is an outbound request like any other. It is therefore **not**
//! made by the control plane: the control plane is on the host network and
//! would bypass the sandbox's proxy, the sandbox's rules, and the sandbox's
//! identity. Instead [`discover`] runs a small JSON-RPC client **inside the
//! container** through the runtime's `exec`, so the request leaves through the
//! same interface as every tool call the agent makes. A sandbox without an
//! allow rule for the server's host cannot discover anything, and the failure
//! is recorded on the attachment rather than hidden.
//!
//! # What discovery returns
//!
//! A failed discovery is recorded as a state of the attachment (`status=error`,
//! `last_error=<reason>`) and returned as a normal response. The row is what
//! the caller asked to update; a 5xx would hide the reason and leave the row
//! claiming the previous state. Request errors stay errors: an unknown sandbox
//! is a 404, a stopped sandbox is a 409, and a `stdio` server is a 422 because
//! there is no channel to it from here.
//!
//! # Credentials
//!
//! The registry holds environment variable **names**, never values. The probe
//! reads the token inside the container from the named variable, so no secret
//! enters this process, the database, a log line, or the audit trail.

use serde::Deserialize;
use uuid::Uuid;

use crate::domain::auth::{Permission, Principal};
use crate::domain::mcp::{
    McpAuth, McpServer, McpToolCall, McpTransport, SandboxMcpServer,
};
use crate::domain::sandbox::SandboxStatus;
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::services::{ServiceContext, require};
use crate::store::TenantScope;
use crate::store::postgres::mcp as store;
use crate::store::postgres::sandboxes;

/// The JSON-RPC client that runs inside the sandbox.
///
/// Deliberately the standard library only: the sandbox's Python may be a bare
/// `python3` from the base image, and a discovery step that needs `pip install`
/// before it can answer "what tools do you have" would fail for the wrong
/// reason. It performs the Streamable-HTTP handshake (`initialize`,
/// `notifications/initialized`, then `tools/list`), keeps the `Mcp-Session-Id`
/// the server assigns, and accepts both a JSON body and an SSE frame, because
/// both are valid answers from a Streamable-HTTP server.
///
/// It uses `urllib`'s default opener, which honours `http_proxy`/`https_proxy`
/// from the sandbox environment. That is the point: the request must take the
/// same path as the agent's own calls.
///
/// Arguments: `endpoint`, then the name of the environment variable that holds
/// the bearer token (empty when the server needs none).
const DISCOVERY_SCRIPT: &str = r#"
import json, os, sys, urllib.request, urllib.error

endpoint = sys.argv[1]
token_env = sys.argv[2] if len(sys.argv) > 2 else ""
token = os.environ.get(token_env, "") if token_env else ""
session = None

def post(payload):
    global session
    headers = {
        "Content-Type": "application/json",
        "Accept": "application/json, text/event-stream",
    }
    if token:
        headers["Authorization"] = "Bearer " + token
    if session:
        headers["Mcp-Session-Id"] = session
    request = urllib.request.Request(
        endpoint, data=json.dumps(payload).encode(), headers=headers, method="POST"
    )
    with urllib.request.urlopen(request, timeout=20) as response:
        assigned = response.headers.get("Mcp-Session-Id")
        if assigned:
            session = assigned
        return response.read(1024 * 1024).decode("utf-8", "replace")

def decode(body):
    text = body.strip()
    if not text:
        return None
    if text.startswith("data:") or text.startswith("event:"):
        for line in text.splitlines():
            if line.startswith("data:"):
                chunk = line[5:].strip()
                if chunk and chunk != "[DONE]":
                    return json.loads(chunk)
        return None
    return json.loads(text)

def failure(message):
    print(json.dumps({"ok": False, "error": message}))
    sys.exit(0)

try:
    answer = decode(post({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "nestbox-control-plane", "version": "1"},
        },
    })) or {}
    if "error" in answer:
        failure("initialize refused: " + str(answer["error"].get("message", answer["error"])))
    post({"jsonrpc": "2.0", "method": "notifications/initialized"})
    answer = decode(post({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})) or {}
    if "error" in answer:
        failure("tools/list refused: " + str(answer["error"].get("message", answer["error"])))
    tools = (answer.get("result") or {}).get("tools") or []
    if not isinstance(tools, list):
        failure("tools/list returned no tool array")
    print(json.dumps({"ok": True, "session_id": session, "tools": tools}))
except urllib.error.HTTPError as exc:
    detail = exc.read(4000).decode("utf-8", "replace").strip()
    failure("HTTP " + str(exc.code) + " from the endpoint" + (": " + detail[:400] if detail else ""))
except Exception as exc:
    failure(type(exc).__name__ + ": " + str(exc))
"#;

/// Seconds a discovery may take before it is cut.
///
/// Discovery is two round trips plus a notification. Two minutes is the
/// runtime's own ceiling for an exec, and a slow vendor server should still fit.
const DISCOVERY_TIMEOUT_SECONDS: u32 = 120;

/// What a create or replace request carries.
///
/// Strings rather than parsed enums on purpose: the API receives JSON, and
/// validation is a rule of this layer, so a bad transport is a 422 with the
/// allowed values instead of a deserialisation error that names a Rust type.
#[derive(Debug, Clone, Default)]
pub struct ServerInput {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
    pub transport: String,
    pub endpoint: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub auth_type: String,
    pub env_slots: Vec<String>,
    pub packs: Vec<String>,
    pub enabled: bool,
}

/// A definition that passed validation, with the transport already parsed.
struct Validated {
    key: String,
    name: String,
    description: Option<String>,
    publisher: Option<String>,
    homepage: Option<String>,
    transport: McpTransport,
    endpoint: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    auth_type: McpAuth,
    env_slots: Vec<String>,
    packs: Vec<String>,
    enabled: bool,
}

impl Validated {
    fn definition(&self) -> store::ServerDefinition<'_> {
        store::ServerDefinition {
            key: &self.key,
            name: &self.name,
            description: self.description.as_deref(),
            publisher: self.publisher.as_deref(),
            homepage: self.homepage.as_deref(),
            transport: self.transport,
            endpoint: self.endpoint.as_deref(),
            command: self.command.as_deref(),
            args: &self.args,
            auth_type: self.auth_type,
            env_slots: &self.env_slots,
            packs: &self.packs,
            enabled: self.enabled,
        }
    }
}

/// Every server in the registry.
pub async fn list(ctx: &ServiceContext, principal: &Principal) -> Result<Vec<McpServer>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let servers = store::list_servers(&mut scope).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(servers)
}

/// One server by key.
pub async fn get(
    ctx: &ServiceContext,
    principal: &Principal,
    key: &str,
) -> Result<McpServer, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let server = store::get_server(&mut scope, key)
        .await
        .map_err(server_not_found("The MCP server does not exist."))?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(server)
}

/// Register a definition.
pub async fn create(
    ctx: &ServiceContext,
    principal: &Principal,
    input: ServerInput,
) -> Result<McpServer, ApiError> {
    // The registry is shared and an attached HTTP server is allowed through the sandbox's
    // policy, so defining one is an admin's decision, not a user's.
    require(principal, Permission::ProviderWrite)?;
    let validated = validate(&input, None)?;

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let server = store::insert_server(&mut scope, &validated.definition())
        .await
        .map_err(|error| match error {
            StoreError::UniqueViolation(_) => ApiError::conflict(
                ErrorCode::CONFLICT,
                format!("A server with the key '{}' already exists.", validated.key),
            ),
            other => ApiError::from(other),
        })?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(server)
}

/// Replace a definition. The key comes from the path and cannot change.
pub async fn replace(
    ctx: &ServiceContext,
    principal: &Principal,
    key: &str,
    input: ServerInput,
) -> Result<McpServer, ApiError> {
    // The registry is shared and an attached HTTP server is allowed through the sandbox's
    // policy, so defining one is an admin's decision, not a user's.
    require(principal, Permission::ProviderWrite)?;
    let validated = validate(&input, Some(key))?;

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let server = store::replace_server(&mut scope, &validated.definition())
        .await
        .map_err(server_not_found("The MCP server does not exist."))?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(server)
}

/// Remove a definition.
///
/// A server a sandbox still uses is refused, and the refusal names the count:
/// deleting it would either break the attachment or, with the foreign key
/// relaxed, leave a row pointing at nothing.
pub async fn delete(
    ctx: &ServiceContext,
    principal: &Principal,
    key: &str,
) -> Result<(), ApiError> {
    // The registry is shared and an attached HTTP server is allowed through the sandbox's
    // policy, so defining one is an admin's decision, not a user's.
    require(principal, Permission::ProviderWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let attached = store::count_attachments(&mut scope, key)
        .await
        .map_err(ApiError::from)?;
    if attached > 0 {
        scope.commit().await.map_err(ApiError::from)?;
        return Err(ApiError::conflict(
            ErrorCode::CONFLICT,
            format!(
                "'{key}' is attached to {attached} sandbox(es). Detach it there first."
            ),
        )
        .with_details(serde_json::json!({ "key": key, "attached": attached })));
    }
    store::delete_server(&mut scope, key)
        .await
        .map_err(server_not_found("The MCP server does not exist."))?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// The servers attached to one sandbox.
pub async fn sandbox_servers(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Vec<SandboxMcpServer>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    // The sandbox is checked first, so an unknown id is a 404 rather than an
    // empty list that looks like "attached to nothing".
    load_sandbox(&mut scope, sandbox_id).await?;
    let servers = store::list_attachments(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(servers)
}

/// Attach a registry server to a sandbox.
pub async fn attach(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    key: &str,
    enabled: bool,
) -> Result<SandboxMcpServer, ApiError> {
    require(principal, Permission::SandboxWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    load_sandbox(&mut scope, sandbox_id).await?;
    let server = store::get_server(&mut scope, key)
        .await
        .map_err(server_not_found("The MCP server does not exist."))?;
    if !server.enabled {
        scope.commit().await.map_err(ApiError::from)?;
        return Err(ApiError::validation(format!(
            "'{key}' is disabled in the registry and cannot be attached."
        )));
    }
    let attachment = store::insert_attachment(&mut scope, sandbox_id, key, enabled)
        .await
        .map_err(|error| match error {
            StoreError::UniqueViolation(_) => ApiError::conflict(
                ErrorCode::CONFLICT,
                format!("'{key}' is already attached to this sandbox."),
            ),
            other => ApiError::from(other),
        })?;
    scope.commit().await.map_err(ApiError::from)?;
    // Attaching is the grant, so the agent's config is rewritten now rather
    // than on the next recreate: a tool the console shows as attached must be
    // a tool the running agent can actually call. A stopped sandbox has no
    // file to write; the config then lands on its next start.
    if let Err(error) =
        crate::services::sandbox_service::reconfigure_agent(ctx, principal, sandbox_id).await
    {
        tracing::debug!(code = error.code.0, "MCP attachment not applied to a running agent");
    }
    Ok(attachment)
}

/// Detach a server from a sandbox.
pub async fn detach(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    key: &str,
) -> Result<(), ApiError> {
    require(principal, Permission::SandboxWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    store::delete_attachment(&mut scope, sandbox_id, key)
        .await
        .map_err(|error| match error {
            StoreError::NotFound => ApiError::not_found(
                ErrorCode::NOT_FOUND,
                "This sandbox has no such MCP server attached.",
            ),
            other => ApiError::from(other),
        })?;
    scope.commit().await.map_err(ApiError::from)?;
    // A removed tool must also disappear from the agent's config, or the
    // console and the agent would disagree about what is available.
    if let Err(error) =
        crate::services::sandbox_service::reconfigure_agent(ctx, principal, sandbox_id).await
    {
        tracing::debug!(code = error.code.0, "MCP detachment not applied to a running agent");
    }
    Ok(())
}

/// Ask a server for its tool list, from inside the sandbox.
///
/// Returns the attachment with what discovery found. A server that did not
/// answer comes back with `status=error` and the reason; see the module note on
/// why that is a response and not a 5xx.
pub async fn discover(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    key: &str,
) -> Result<SandboxMcpServer, ApiError> {
    require(principal, Permission::SandboxWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let sandbox = load_sandbox(&mut scope, sandbox_id).await?;
    let attachment = store::get_attachment(&mut scope, sandbox_id, key)
        .await
        .map_err(|error| match error {
            StoreError::NotFound => ApiError::not_found(
                ErrorCode::NOT_FOUND,
                "This sandbox has no such MCP server attached.",
            ),
            other => ApiError::from(other),
        })?;
    let server = store::get_server(&mut scope, key)
        .await
        .map_err(server_not_found("The MCP server does not exist."))?;
    scope.commit().await.map_err(ApiError::from)?;

    if !attachment.enabled {
        return Err(ApiError::conflict(
            ErrorCode::CONFLICT,
            format!("'{key}' is attached but disabled for this sandbox."),
        ));
    }
    if !server.transport.visible_to_proxy() {
        return Err(ApiError::unprocessable(
            ErrorCode::CONFLICT,
            format!(
                "'{key}' is a {} server: it runs inside the container and has no HTTP endpoint \
                 the control plane could ask. Start it in the sandbox instead.",
                server.transport.as_str()
            ),
        ));
    }
    let endpoint = server.endpoint.as_deref().ok_or_else(|| {
        ApiError::unprocessable(
            ErrorCode::VALIDATION_FAILED,
            format!("'{key}' has no endpoint, so it cannot be discovered."),
        )
    })?;
    if sandbox.status != SandboxStatus::Running {
        return Err(ApiError::conflict(
            ErrorCode::SANDBOX_INVALID_STATE,
            format!(
                "Cannot discover through a sandbox that is '{}'.",
                sandbox.status.as_str()
            ),
        ));
    }

    let token_env = token_env_slot(&server);
    let command = discovery_command(endpoint, token_env.as_deref());
    let outcome = ctx
        .runtime
        .exec(&sandbox.name, &command, DISCOVERY_TIMEOUT_SECONDS)
        .await;

    let (reason, tools, session) = match outcome {
        Err(error) => (Some(describe_runtime_error(&error)), None, None),
        Ok(result) => match read_discovery(&result) {
            Ok(found) => (None, Some(found.tools), found.session_id),
            Err(reason) => (Some(reason), None, None),
        },
    };

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let updated = match (reason, tools) {
        (None, Some(tools)) => {
            store::mark_discovered(&mut scope, attachment.id, &tools, session.as_deref()).await
        }
        (Some(reason), _) => {
            store::mark_discovery_failed(&mut scope, attachment.id, &reason).await
        }
        (None, None) => {
            store::mark_discovery_failed(
                &mut scope,
                attachment.id,
                "discovery returned no tool list",
            )
            .await
        }
    }
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(updated)
}

/// The most recent calls for one sandbox, from the proxy's point of view.
///
/// This is the read half of `mcp_tool_calls`; the write half is the telemetry
/// ingest, which calls [`store::insert_call`].
pub async fn calls(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    limit: i64,
) -> Result<Vec<McpToolCall>, ApiError> {
    require(principal, Permission::TelemetryRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    load_sandbox(&mut scope, sandbox_id).await?;
    let calls = store::list_calls(&mut scope, sandbox_id, limit.clamp(1, 200))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(calls)
}

/// The environment variable that holds a server's token, if any.
///
/// The attachment may carry its own slot list in a later revision; today the
/// registry is the only source, and the first slot is the credential by
/// convention (`NESTBOX_MCP_TOKEN`, `GITHUB_PERSONAL_ACCESS_TOKEN`).
fn token_env_slot(server: &McpServer) -> Option<String> {
    match server.auth_type {
        McpAuth::None => None,
        McpAuth::Token | McpAuth::Oauth => server.env_slots.first().cloned(),
    }
}

/// The argv a discovery runs. A vector, never a shell string.
fn discovery_command(endpoint: &str, token_env: Option<&str>) -> Vec<String> {
    vec![
        "python3".to_string(),
        "-c".to_string(),
        DISCOVERY_SCRIPT.trim_matches('\n').to_string(),
        endpoint.to_string(),
        token_env.unwrap_or_default().to_string(),
    ]
}

/// What the probe printed.
struct Discovery {
    tools: serde_json::Value,
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeAnswer {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    tools: Option<serde_json::Value>,
}

/// Read the probe's answer out of an exec result.
///
/// The last parseable JSON line wins: `docker exec` may prepend a shell's own
/// noise in a future runtime, and the probe prints exactly one object.
fn read_discovery(result: &crate::domain::runtime::ExecResult) -> Result<Discovery, String> {
    let tail: String = result
        .output
        .chars()
        .rev()
        .take(600)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if result.truncated {
        return Err(format!(
            "the discovery answer was truncated by the runtime; tail: {tail}"
        ));
    }
    let answer = result
        .output
        .lines()
        .rev()
        .filter(|line| line.trim_start().starts_with('{'))
        .find_map(|line| serde_json::from_str::<ProbeAnswer>(line.trim()).ok())
        .ok_or_else(|| {
            format!(
                "the probe returned no JSON (exit {:?}): {}",
                result.exit_code,
                tail.trim()
            )
        })?;
    if !answer.ok {
        return Err(answer
            .error
            .unwrap_or_else(|| "the probe reported a failure without a reason".to_string()));
    }
    let tools = answer
        .tools
        .filter(|tools| tools.is_array())
        .ok_or_else(|| "the probe returned no tool array".to_string())?;
    Ok(Discovery {
        tools,
        session_id: answer.session_id,
    })
}

/// A runtime failure, phrased for the attachment's `last_error`.
fn describe_runtime_error(error: &crate::runtime::RuntimeError) -> String {
    match error {
        crate::runtime::RuntimeError::NotFound(_) => {
            "the runtime has no container for this sandbox".to_string()
        }
        crate::runtime::RuntimeError::Rejected(message) => {
            format!("the runtime refused: {message}")
        }
        crate::runtime::RuntimeError::Unavailable(message) => {
            format!("the runtime is unreachable: {message}")
        }
    }
}

/// Load a sandbox, or fail with the sandbox code rather than a generic 404.
async fn load_sandbox(
    scope: &mut TenantScope<'_>,
    sandbox_id: Uuid,
) -> Result<crate::domain::sandbox::Sandbox, ApiError> {
    sandboxes::get(scope, sandbox_id).await.map_err(|error| match error {
        StoreError::NotFound => ApiError::not_found(
            ErrorCode::SANDBOX_NOT_FOUND,
            "The sandbox does not exist.",
        ),
        other => ApiError::from(other),
    })
}

fn server_not_found(message: &'static str) -> impl Fn(StoreError) -> ApiError {
    move |error| match error {
        StoreError::NotFound => ApiError::not_found(ErrorCode::NOT_FOUND, message),
        other => ApiError::from(other),
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a definition.
///
/// `path_key` is the key from the URL on a replace. A body that names a
/// different key is refused rather than silently ignored: a rename would break
/// the attachments that point at the old one.
fn validate(input: &ServerInput, path_key: Option<&str>) -> Result<Validated, ApiError> {
    let body_key = optional_trimmed(&input.key);
    let key = match (path_key, body_key) {
        (Some(path), Some(body)) if body != path => {
            return Err(ApiError::validation(format!(
                "The body names the key '{body}', but the path is '{path}'. A key cannot be changed."
            )));
        }
        (Some(path), _) => path.to_string(),
        (None, Some(body)) => body,
        (None, None) => return Err(ApiError::validation("A server needs a key.")),
    };
    validate_key(&key)?;

    let name = optional_trimmed(&input.name)
        .ok_or_else(|| ApiError::validation("A server needs a name."))?;
    if name.chars().count() > 120 {
        return Err(ApiError::validation(
            "A server name must be at most 120 characters.",
        ));
    }
    let description = bounded(normalize(&input.description), 2000, "description")?;
    let publisher = bounded(normalize(&input.publisher), 120, "publisher")?;
    let homepage = match normalize(&input.homepage) {
        Some(raw) => Some(validate_url(&raw, "homepage")?.to_string()),
        None => None,
    };

    let transport_raw = optional_trimmed(&input.transport).ok_or_else(|| {
        ApiError::validation("A server needs a transport: stdio, http, or sse.")
    })?;
    let transport = McpTransport::parse(&transport_raw).ok_or_else(|| {
        ApiError::validation(format!(
            "Unknown transport '{transport_raw}'. Use 'stdio', 'http', or 'sse'."
        ))
        .with_details(serde_json::json!({ "allowed": ["stdio", "http", "sse"] }))
    })?;

    let auth_raw = optional_trimmed(&input.auth_type).unwrap_or_else(|| "none".to_string());
    let auth_type = McpAuth::parse(&auth_raw).ok_or_else(|| {
        ApiError::validation(format!(
            "Unknown auth type '{auth_raw}'. Use 'none', 'token', or 'oauth'."
        ))
        .with_details(serde_json::json!({ "allowed": ["none", "token", "oauth"] }))
    })?;

    let endpoint = normalize(&input.endpoint);
    let command = normalize(&input.command);

    match transport {
        McpTransport::Stdio => {
            if endpoint.is_some() {
                return Err(ApiError::validation(
                    "A stdio server has no endpoint: it starts a process from 'command'.",
                ));
            }
            if command.is_none() {
                return Err(ApiError::validation(
                    "A stdio server needs a 'command' to run.",
                ));
            }
            if auth_type != McpAuth::None {
                return Err(ApiError::validation(
                    "A stdio server authenticates through 'env'; auth_type must be 'none'.",
                ));
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            if command.is_some() {
                return Err(ApiError::validation(
                    "An http or sse server has no 'command': give it an 'endpoint'.",
                ));
            }
            match &endpoint {
                Some(raw) => {
                    validate_url(raw, "endpoint")?;
                }
                None => {
                    return Err(ApiError::validation(
                        "An http or sse server needs an 'endpoint' such as https://host/mcp.",
                    ));
                }
            }
        }
    }

    if let Some(raw) = &command {
        validate_command(raw)?;
    }
    validate_args(&input.args)?;
    validate_env_slots(&input.env_slots)?;
    validate_packs(&input.packs)?;

    Ok(Validated {
        key,
        name,
        description,
        publisher,
        homepage,
        transport,
        endpoint,
        command,
        args: trimmed_all(&input.args),
        auth_type,
        env_slots: trimmed_all(&input.env_slots),
        packs: trimmed_all(&input.packs),
        enabled: input.enabled,
    })
}

/// A key is a stable identifier a config file and a URL can carry.
fn validate_key(key: &str) -> Result<(), ApiError> {
    let valid = !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && key.as_bytes()[0].is_ascii_alphanumeric()
        && key.as_bytes()[key.len() - 1].is_ascii_alphanumeric();
    if valid {
        Ok(())
    } else {
        Err(ApiError::validation(format!(
            "'{key}' is not a valid key: 1–64 characters, lowercase letters, digits and hyphens, \
             starting and ending with a letter or digit."
        )))
    }
}

/// An `http`/`https` URL with a host and no embedded credential.
///
/// A credential in a URL ends up in a log line, a proxy access log, and a
/// browser history. The registry has `env_slots` for exactly that reason, so a
/// URL carrying one is refused instead of stored.
fn validate_url<'a>(raw: &'a str, field: &str) -> Result<&'a str, ApiError> {
    if raw.len() > 2048 {
        return Err(ApiError::validation(format!(
            "The {field} must be at most 2048 characters."
        )));
    }
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(ApiError::validation(format!(
            "The {field} must not contain whitespace or control characters."
        )));
    }
    let rest = raw
        .strip_prefix("https://")
        .or_else(|| raw.strip_prefix("http://"))
        .ok_or_else(|| {
            ApiError::validation(format!(
                "The {field} must start with http:// or https:// (got '{raw}')."
            ))
        })?;
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        return Err(ApiError::validation(format!(
            "The {field} needs a host, not just a scheme."
        )));
    }
    if authority.contains('@') {
        return Err(ApiError::validation(
            "A URL must not carry credentials. Register the variable name in 'env_slots' instead.",
        ));
    }
    let host = authority.split(':').next().unwrap_or_default();
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'[' || b == b']')
    {
        return Err(ApiError::validation(format!(
            "The {field} has an invalid host '{host}'."
        )));
    }
    Ok(raw)
}

/// A stdio command: one executable name or path, no shell, no control bytes.
fn validate_command(raw: &str) -> Result<(), ApiError> {
    if raw.len() > 1024 {
        return Err(ApiError::validation(
            "A command must be at most 1024 characters.",
        ));
    }
    if raw.chars().any(|c| c.is_control()) {
        return Err(ApiError::validation(
            "A command must not contain control characters or a newline.",
        ));
    }
    Ok(())
}

fn validate_args(args: &[String]) -> Result<(), ApiError> {
    if args.len() > 64 {
        return Err(ApiError::validation("At most 64 arguments are allowed."));
    }
    if let Some(bad) = args.iter().find(|arg| arg.len() > 1024 || arg.contains('\0')) {
        return Err(ApiError::validation(format!(
            "An argument is too long or contains NUL: '{}'.",
            bad.chars().take(40).collect::<String>()
        )));
    }
    Ok(())
}

/// Environment slots are variable names, never values.
fn validate_env_slots(slots: &[String]) -> Result<(), ApiError> {
    if slots.len() > 32 {
        return Err(ApiError::validation(
            "At most 32 environment slots are allowed.",
        ));
    }
    for slot in slots {
        let slot = slot.trim();
        let valid = !slot.is_empty()
            && slot.len() <= 64
            && slot.as_bytes()[0].is_ascii_uppercase()
            && slot
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
        if !valid {
            return Err(ApiError::validation(format!(
                "'{slot}' is not an environment variable name (uppercase letters, digits, and \
                 underscores, starting with a letter). A slot is a name, never a value."
            )));
        }
    }
    let distinct = slots
        .iter()
        .map(|slot| slot.trim())
        .collect::<std::collections::BTreeSet<_>>();
    if distinct.len() != slots.len() {
        return Err(ApiError::validation("An environment slot is listed twice."));
    }
    Ok(())
}

fn validate_packs(packs: &[String]) -> Result<(), ApiError> {
    if packs.len() > 32 {
        return Err(ApiError::validation("At most 32 packs are allowed."));
    }
    for pack in packs {
        validate_key(pack.trim()).map_err(|_| {
            ApiError::validation(format!(
                "'{}' is not a pack key (lowercase letters, digits and hyphens).",
                pack.trim()
            ))
        })?;
    }
    Ok(())
}

fn normalize(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The same normalisation for a field that is a plain string in the body.
fn optional_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn bounded(
    value: Option<String>,
    limit: usize,
    field: &str,
) -> Result<Option<String>, ApiError> {
    if let Some(value) = &value {
        if value.chars().count() > limit {
            return Err(ApiError::validation(format!(
                "The {field} must be at most {limit} characters."
            )));
        }
    }
    Ok(value)
}

fn trimmed_all(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio_input() -> ServerInput {
        ServerInput {
            key: "filesystem".into(),
            name: "Filesystem".into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into()],
            enabled: true,
            ..Default::default()
        }
    }

    fn http_input() -> ServerInput {
        ServerInput {
            key: "nestbox-hub".into(),
            name: "Nestbox hub".into(),
            transport: "http".into(),
            endpoint: Some("http://mcp-hub:8080/mcp".into()),
            auth_type: "token".into(),
            env_slots: vec!["NESTBOX_MCP_TOKEN".into()],
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_stdio_server_and_an_http_server_both_validate() {
        assert!(validate(&stdio_input(), None).is_ok());
        assert!(validate(&http_input(), None).is_ok());
    }

    #[test]
    fn an_unknown_transport_is_refused_with_the_allowed_values() {
        let mut input = http_input();
        input.transport = "websocket".into();
        let error = validate(&input, None).err().expect("refused");
        assert_eq!(error.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert!(error.details.to_string().contains("stdio"));
    }

    #[test]
    fn an_http_server_without_an_endpoint_is_refused() {
        let mut input = http_input();
        input.endpoint = None;
        assert!(validate(&input, None).is_err());
    }

    #[test]
    fn an_endpoint_must_be_http_or_https() {
        for endpoint in ["ftp://example.com/x", "example.com/mcp", "https://"] {
            let mut input = http_input();
            input.endpoint = Some(endpoint.into());
            assert!(
                validate(&input, None).is_err(),
                "{endpoint} should be refused"
            );
        }
    }

    #[test]
    fn a_url_may_not_carry_a_credential() {
        let mut input = http_input();
        input.endpoint = Some("https://user:secret@mcp.example.com/mcp".into());
        let error = validate(&input, None).err().expect("refused");
        assert!(error.message.contains("env_slots"));
    }

    #[test]
    fn a_stdio_server_cannot_claim_an_http_auth_type() {
        let mut input = stdio_input();
        input.auth_type = "oauth".into();
        assert!(validate(&input, None).is_err());
    }

    #[test]
    fn a_stdio_server_may_not_carry_both_a_command_and_an_endpoint() {
        let mut input = stdio_input();
        input.endpoint = Some("https://example.com/mcp".into());
        assert!(validate(&input, None).is_err());
    }

    #[test]
    fn a_key_is_lowercase_and_hyphenated() {
        for key in ["Nestbox", "a b", "-x", "x-", "a_b", ""] {
            let mut input = http_input();
            input.key = key.into();
            assert!(validate(&input, None).is_err(), "{key} should be refused");
        }
    }

    #[test]
    fn a_replace_may_not_change_the_key() {
        let mut input = http_input();
        input.key = "other".into();
        let error = validate(&input, Some("nestbox-hub")).err().expect("refused");
        assert!(error.message.contains("cannot be changed"));
    }

    #[test]
    fn an_env_slot_is_a_name_never_a_value() {
        let mut input = http_input();
        input.env_slots = vec!["GITHUB_PERSONAL_ACCESS_TOKEN=ghp_x".into()];
        assert!(validate(&input, None).is_err());

        let mut lower = http_input();
        lower.env_slots = vec!["token".into()];
        assert!(validate(&lower, None).is_err());
    }

    #[test]
    fn a_duplicate_env_slot_is_refused() {
        let mut input = http_input();
        input.env_slots = vec!["TOKEN".into(), "TOKEN".into()];
        assert!(validate(&input, None).is_err());
    }

    #[test]
    fn the_discovery_command_carries_the_variable_name_not_the_token() {
        let command = discovery_command("http://mcp-hub:8080/mcp", Some("NESTBOX_MCP_TOKEN"));
        assert_eq!(command[0], "python3");
        assert_eq!(command[1], "-c");
        assert!(command[2].contains("tools/list"));
        assert_eq!(command[3], "http://mcp-hub:8080/mcp");
        assert_eq!(command[4], "NESTBOX_MCP_TOKEN");
        // Five parts: interpreter, flag, script, endpoint, slot. A credential
        // would be a sixth, and the script only ever reads it from the
        // container's own environment.
        assert_eq!(command.len(), 5);
        assert!(!command.iter().any(|part| part.contains("ghp_")));
    }

    #[test]
    fn the_probe_honours_the_sandbox_proxy_environment() {
        // `urllib.request.urlopen` without an explicit opener is the whole
        // mechanism; a custom opener with an empty ProxyHandler would bypass it.
        assert!(DISCOVERY_SCRIPT.contains("urllib.request.urlopen"));
        assert!(!DISCOVERY_SCRIPT.contains("ProxyHandler({})"));
    }

    #[test]
    fn the_probe_reads_the_token_from_the_container_environment() {
        assert!(DISCOVERY_SCRIPT.contains("os.environ.get(token_env"));
    }

    #[test]
    fn a_visible_transport_is_the_only_one_with_an_endpoint_probe() {
        assert!(McpTransport::Http.visible_to_proxy());
        assert!(!McpTransport::Stdio.visible_to_proxy());
    }

    fn exec(output: &str) -> crate::domain::runtime::ExecResult {
        crate::domain::runtime::ExecResult {
            exit_code: Some(0),
            output: output.to_string(),
            truncated: false,
        }
    }

    #[test]
    fn a_successful_probe_answer_yields_tools_and_a_session() {
        let result = exec(
            r#"{"ok": true, "session_id": "abc123", "tools": [{"name": "read_file"}]}"#,
        );
        let found = read_discovery(&result).ok().expect("a tool list");
        assert_eq!(found.session_id.as_deref(), Some("abc123"));
        assert_eq!(found.tools.as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn a_probe_error_is_the_reason_recorded_on_the_attachment() {
        let result = exec(r#"{"ok": false, "error": "HTTP 403 from the endpoint"}"#);
        let reason = read_discovery(&result).err().expect("a refusal");
        assert!(reason.contains("403"));
    }

    #[test]
    fn garbage_output_is_reported_rather_than_swallowed() {
        let result = exec("python3: command not found");
        let reason = read_discovery(&result).err().expect("a reason");
        assert!(reason.contains("no JSON"));
        assert!(reason.contains("command not found"));
    }

    #[test]
    fn a_truncated_answer_is_a_failure_not_a_partial_tool_list() {
        let mut result = exec(r#"{"ok": true, "tools": []}"#);
        result.truncated = true;
        let reason = read_discovery(&result).err().expect("a reason");
        assert!(reason.contains("truncated"));
    }

    #[test]
    fn a_probe_answer_without_a_tool_array_is_a_failure() {
        let result = exec(r#"{"ok": true, "session_id": "x"}"#);
        assert!(read_discovery(&result).is_err());
    }
}

/// Check every attached HTTP MCP server on a running sandbox that is due, so "connected"
/// and "not running" are facts the console shows instead of something to click for.
///
/// Called from the reconcile loop. A server that is not connected is retried every pass
/// (about 30 s); a connected one is re-checked every 90 s. A stdio server runs inside the
/// container and has nothing to ask, so it is left alone. At most a handful per pass, so a
/// slow server cannot hold the loop. The control plane acts here for nobody, which is why
/// the check runs as an admin of the environment it belongs to.
pub async fn check_attached(ctx: &ServiceContext) -> Result<u64, ApiError> {
    use crate::domain::auth::{CredentialSource, Role};
    const PER_PASS: usize = 8;
    let environments: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM environments")
        .fetch_all(&ctx.pool)
        .await
        .map_err(|error| ApiError::from(StoreError::Database(error)))?;
    let mut checked = 0;
    for environment_id in environments {
        let mut scope = TenantScope::begin(&ctx.pool, environment_id).await.map_err(ApiError::from)?;
        let due: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT a.sandbox_id, a.server_key
               FROM sandbox_mcp_servers a
               JOIN sandboxes s ON s.id = a.sandbox_id
               JOIN mcp_registry r ON r.key = a.server_key
              WHERE s.status = 'running' AND a.enabled AND r.transport IN ('http', 'sse')
                AND a.updated_at < now() - interval '25 seconds'
                AND (a.status <> 'connected' OR a.updated_at < now() - interval '90 seconds')
              ORDER BY a.updated_at LIMIT $1",
        )
        .bind(PER_PASS as i64)
        .fetch_all(scope.connection())
        .await
        .map_err(|error| ApiError::from(StoreError::Database(error)))?;
        scope.commit().await.map_err(ApiError::from)?;
        let system = Principal {
            user_id: Uuid::nil(),
            environment_id,
            role: Role::Admin,
            display_name: "mcp health check".into(),
            via: CredentialSource::Token,
        };
        for (sandbox_id, key) in due {
            // A failure is recorded on the attachment by `discover` itself; what is left
            // here (the sandbox went away meanwhile) is not worth more than a log line.
            if let Err(error) = discover(ctx, &system, sandbox_id, &key).await {
                tracing::debug!(%sandbox_id, key = %key, code = error.code.0, "mcp check skipped");
            }
            checked += 1;
        }
    }
    Ok(checked)
}
