//! Telemetry ingest, live publish, and history.

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::telemetry::{Decision, EventKind, LiveEvent, TelemetryEvent};
use crate::error::ApiError;
use crate::store::TenantScope;
use crate::store::postgres::telemetry as store;

/// Accept one event from a gateway or the runtime.
///
/// Two independent effects: persist it for history, and publish it live. A slow
/// database must not delay the stream, so the publish happens whether or not the
/// insert succeeded; a lost history row is a gap in a report, a lost live event
/// is a console that shows nothing while an agent works.
pub async fn ingest(ctx: &ServiceContext, event: TelemetryEvent) -> Result<Uuid, ApiError> {
    let environment_id = event.environment_id();
    let sandbox_id = event.sandbox_id();
    let kind = event.kind();
    let ts = match &event {
        TelemetryEvent::Network(e) => e.ts,
        TelemetryEvent::Dns(e) => e.ts,
        TelemetryEvent::Inference(e) => e.ts,
    };

    let detail = serde_json::to_value(&event).unwrap_or(serde_json::Value::Null);

    let stored = store::insert(
        &ctx.pool,
        store::NewEvent {
            environment_id,
            sandbox_id,
            gateway_id: None,
            kind,
            source: Some("gateway"),
            ts,
            detail,
        },
    )
    .await;

    let id = match stored {
        Ok(id) => id,
        Err(error) => {
            tracing::error!(error = %error, "telemetry insert failed; publishing anyway");
            Uuid::new_v4()
        }
    };

    if let TelemetryEvent::Inference(inference) = &event {
        if let Err(error) = store::insert_inference(&ctx.pool, inference).await {
            tracing::warn!(error = %error, "inference row insert failed");
        }
    }

    ctx.bus.publish(LiveEvent::Telemetry(event));
    Ok(id)
}

/// Recent events, for the console's history view.
///
/// The time range defaults to one hour: a query without a bound scans every
/// partition, which defeats the partitioning.
pub async fn history(
    ctx: &ServiceContext,
    principal: &Principal,
    kind: Option<EventKind>,
    minutes: i64,
    limit: i64,
) -> Result<Vec<store::StoredEvent>, ApiError> {
    require(principal, Permission::TelemetryRead)?;
    let since = Utc::now() - Duration::minutes(minutes.clamp(1, 10_080));
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let events = store::list(&mut scope, kind, since, limit.clamp(1, 500))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(events)
}

/// Counts per kind for the filter bar.
pub async fn facets(
    ctx: &ServiceContext,
    principal: &Principal,
    minutes: i64,
) -> Result<std::collections::BTreeMap<String, i64>, ApiError> {
    require(principal, Permission::TelemetryRead)?;
    let since = Utc::now() - Duration::minutes(minutes.clamp(1, 10_080));
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let facets = store::facets(&mut scope, since).await.map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(facets)
}

/// Inference requests for one sandbox.
pub async fn sandbox_inference(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    limit: i64,
) -> Result<Vec<serde_json::Value>, ApiError> {
    require(principal, Permission::TelemetryRead)?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let rows = store::inference_for_sandbox(&mut scope, sandbox_id, limit.clamp(1, 500))
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(rows)
}

/// Create the partitions the retention policy will later drop.
pub async fn maintain_partitions(
    ctx: &ServiceContext,
    months_ahead: i32,
) -> Result<serde_json::Value, ApiError> {
    let created = store::ensure_partitions(&ctx.pool, months_ahead.clamp(1, 12))
        .await
        .map_err(ApiError::from)?;
    Ok(serde_json::json!({ "created": created }))
}

/// A synthetic event, used by the console's demo mode and by the smoke test.
///
/// Kept in the service rather than the store so it goes through the same redact
/// and publish path as a real event.
pub fn sample_event(environment_id: Uuid, sandbox_id: Option<Uuid>, index: u64) -> TelemetryEvent {
    let destinations = [
        ("api.anthropic.com", 443, "rest", Decision::Allow),
        ("telemetry.example.net", 443, "rest", Decision::Deny),
        ("pypi.org", 443, "rest", Decision::Allow),
        ("169.254.169.254", 80, "http", Decision::Deny),
    ];
    let (destination, port, protocol, decision) =
        destinations[(index as usize) % destinations.len()];

    TelemetryEvent::Network(crate::domain::telemetry::NetworkEvent {
        id: Uuid::new_v4(),
        environment_id,
        sandbox_id,
        gateway_id: None,
        ts: Utc::now(),
        direction: crate::domain::telemetry::Direction::Outbound,
        protocol: protocol.to_string(),
        source: "10.244.0.2".to_string(),
        destination: destination.to_string(),
        port: Some(port),
        binary: Some("/usr/local/bin/agent".to_string()),
        decision,
        rule_id: None,
        rule_name: Some("demo".to_string()),
        latency_ms: Some(12 + (index % 40) as i32),
        correlation_id: Some(format!("demo-{index}")),
        enforcement: Some("enforced".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sample_event_alternates_allow_and_deny() {
        let env = Uuid::new_v4();
        let mut decisions = Vec::new();
        for index in 0..4 {
            let TelemetryEvent::Network(event) = sample_event(env, None, index) else {
                panic!("expected network");
            };
            decisions.push(event.decision);
        }
        assert!(decisions.contains(&Decision::Allow));
        assert!(decisions.contains(&Decision::Deny));
    }
}
