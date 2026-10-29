//! Who is signed in where: the login of each running sandbox, and who is connected.
//!
//! One read for the whole overview. Each running sandbox is asked, in parallel and with
//! a short limit, what its agent is signed in with (files and environment inside it); a
//! sandbox that does not answer is reported as unknown rather than slowing the page.
//! Open SSH and terminal sessions come from the session audit.

use std::time::Duration;

use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::login::{self, LoginState, Probe};
use crate::error::{ApiError, StoreError};
use crate::store::TenantScope;

const PROBE_SECONDS: u32 = 6;

/// What runs inside the sandbox. python3 is in the agent images; its absence is an
/// "unknown", not a failure.
const PROBE: &str = r#"python3 - <<'PY'
import json, os
home = os.path.expanduser("~")
out = {"gateway_key": bool(os.environ.get("ANTHROPIC_AUTH_TOKEN") or os.environ.get("OPENAI_API_KEY")), "account": None}
cfg = os.environ.get("CLAUDE_CONFIG_DIR") or home + "/.claude"
try:
    oauth = json.load(open(cfg + "/.credentials.json")).get("claudeAiOauth") or {}
    state = {}
    try:
        state = json.load(open(cfg + "/.claude.json"))
    except Exception:
        pass
    out["account"] = {"provider": "claude", "email": (state.get("oauthAccount") or {}).get("emailAddress"), "expires_at_ms": oauth.get("expiresAt")}
except Exception:
    pass
if out["account"] is None:
    try:
        keys = list(json.load(open(home + "/.local/share/opencode/auth.json")).keys())
        if keys:
            out["account"] = {"provider": "opencode:" + ",".join(keys), "email": None, "expires_at_ms": None}
    except Exception:
        pass
print(json.dumps(out))
PY"#;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SandboxLogin {
    pub sandbox_id: Uuid,
    pub sandbox_name: String,
    pub pack_key: String,
    pub login: LoginState,
    /// The supervisor is connected to the gateway, so the sandbox is mediated by OpenShell.
    pub openshell_connected: bool,
    pub ssh_sessions: i64,
    pub terminal_sessions: i64,
}

pub async fn overview(ctx: &ServiceContext, principal: &Principal) -> Result<Vec<SandboxLogin>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal).await.map_err(ApiError::from)?;
    let db = |error: sqlx::Error| ApiError::from(StoreError::Database(error));
    let rows: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(
        "SELECT s.id, s.name, s.pack_key, v.status
           FROM sandboxes s LEFT JOIN supervisors v ON v.id = s.supervisor_id
          WHERE s.status = 'running' ORDER BY s.name",
    )
    .fetch_all(scope.connection())
    .await
    .map_err(db)?;
    let sessions: Vec<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT sandbox_id, kind, count(*) FROM sandbox_sessions WHERE closed_at IS NULL GROUP BY sandbox_id, kind",
    )
    .fetch_all(scope.connection())
    .await
    .map_err(db)?;
    let gateway_reachable: bool = sqlx::query_scalar(
        "SELECT COALESCE(bool_or(status = 'reachable'), false) FROM provider_profiles WHERE kind = 'omniroute'",
    )
    .fetch_one(scope.connection())
    .await
    .map_err(db)?;
    scope.commit().await.map_err(ApiError::from)?;

    let now_ms = chrono::Utc::now().timestamp_millis();
    let probes = rows.iter().map(|(_, name, ..)| {
        let command = vec!["sh".to_string(), "-c".to_string(), PROBE.to_string()];
        async move {
            let answer = tokio::time::timeout(
                Duration::from_secs(u64::from(PROBE_SECONDS) + 2),
                ctx.runtime.exec(name, &command, PROBE_SECONDS),
            )
            .await;
            match answer {
                Ok(Ok(result)) if result.exit_code == Some(0) => {
                    result.output.lines().rev().find_map(|line| serde_json::from_str::<Probe>(line.trim()).ok())
                }
                _ => None,
            }
        }
    });
    let probed = futures::future::join_all(probes).await;

    let count = |id: Uuid, kind: &str| {
        sessions.iter().find(|(sandbox, k, _)| *sandbox == id && k == kind).map_or(0, |(_, _, n)| *n)
    };
    Ok(rows
        .into_iter()
        .zip(probed)
        .map(|((id, name, pack_key, supervisor_status), probe)| SandboxLogin {
            sandbox_id: id,
            sandbox_name: name,
            pack_key,
            login: login::assess(probe.as_ref(), now_ms, gateway_reachable),
            openshell_connected: supervisor_status.as_deref() == Some("running"),
            ssh_sessions: count(id, "ssh"),
            terminal_sessions: count(id, "terminal"),
        })
        .collect())
}
