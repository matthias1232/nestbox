//! Docker-Adapter über den Sandbox-Runner.
//!
//! Der Runner ist ein kleiner Dienst mit Docker-Socket und führt die
//! Container-Operationen aus. Diese Datei übersetzt zwischen dem
//! [`RuntimeAdapter`]-Vertrag und dessen HTTP-Endpunkten.

use async_trait::async_trait;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::domain::runtime::{
    ExecResult, NetworkInterface, RuntimeFacts, RuntimeResources, SandboxSpec,
};
use crate::runtime::{RuntimeAdapter, RuntimeError};

/// Antwort des Runners für eine Sandbox.
#[derive(Debug, Deserialize)]
struct RunnerSandbox {
    id: String,
    name: String,
    #[serde(default)]
    container_name: String,
    status: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    pack_key: String,
    #[serde(default)]
    tool_profile: String,
    #[serde(default)]
    uptime_seconds: u64,
    #[serde(default)]
    health: String,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    exit_code: Option<i64>,
    #[serde(default)]
    network: Vec<NetworkInterface>,
    #[serde(default)]
    resources: RuntimeResources,
}

impl From<RunnerSandbox> for RuntimeFacts {
    fn from(value: RunnerSandbox) -> Self {
        let first = value.network.first().cloned().unwrap_or_default();
        Self {
            id: value.id,
            name: value.name,
            container_name: value.container_name,
            status: value.status,
            image: value.image,
            pack_key: value.pack_key,
            tool_profile: value.tool_profile,
            ip: first.ip.clone(),
            gateway: first.gateway.clone(),
            network: value.network,
            uptime_seconds: value.uptime_seconds,
            health: value.health,
            started_at: value.started_at,
            exit_code: value.exit_code,
            resources: value.resources,
        }
    }
}

/// Adapter, der einen HTTP-fähigen Runtime-Dienst anspricht.
pub struct HttpRunner {
    base_url: String,
    network: String,
    client: reqwest::Client,
}

impl HttpRunner {
    pub fn new(base_url: impl Into<String>, network: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            network: network.into(),
            // A hung runner must not hold an API worker forever. Docker's own
            // stop grace period is 10 s and an exec may run for minutes, so the
            // bound is generous but finite.
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(660))
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    async fn send<T: for<'de> Deserialize<'de>>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T, RuntimeError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.request(method, url);
        if let Some(payload) = body {
            request = request.json(payload);
        }
        let response = request
            .send()
            .await
            .map_err(|error| RuntimeError::Unavailable(error.to_string()))?;

        let status = response.status();
        let raw = response
            .text()
            .await
            .map_err(|error| RuntimeError::Unavailable(error.to_string()))?;

        if status == StatusCode::NOT_FOUND {
            return Err(RuntimeError::NotFound(raw));
        }
        if !status.is_success() {
            return Err(RuntimeError::Rejected(raw));
        }
        serde_json::from_str(&raw).map_err(|error| RuntimeError::Unavailable(error.to_string()))
    }
}

#[async_trait]
impl RuntimeAdapter for HttpRunner {
    fn name(&self) -> &'static str {
        "docker-development"
    }

    fn network(&self) -> &str {
        &self.network
    }

    async fn health(&self) -> Result<(), RuntimeError> {
        let url = format!("{}/healthz", self.base_url);
        self.client
            .get(url)
            .send()
            .await
            .map_err(|error| RuntimeError::Unavailable(error.to_string()))?
            .error_for_status()
            .map_err(|error| RuntimeError::Unavailable(error.to_string()))?;
        Ok(())
    }

    async fn create(&self, spec: SandboxSpec) -> Result<RuntimeFacts, RuntimeError> {
        let environment: std::collections::BTreeMap<_, _> = spec.environment.into_iter().collect();
        let payload = serde_json::json!({
            "name": spec.name,
            "image": spec.image,
            "command": spec.command,
            "runtime": "docker-development",
            "pack_key": spec.pack_key,
            "tool_profile": spec.tool_profile,
            "environment": environment,
            "memory_limit": spec.memory_limit,
            "cpu_limit": spec.cpu_limit,
            "pids_limit": spec.pids_limit,
        });
        let facts: RunnerSandbox = self
            .send(reqwest::Method::POST, "/v1/sandboxes", Some(&payload))
            .await?;
        Ok(facts.into())
    }

    async fn inspect(&self, name: &str) -> Result<RuntimeFacts, RuntimeError> {
        let facts: RunnerSandbox = self
            .send(reqwest::Method::GET, &format!("/v1/sandboxes/{name}"), None)
            .await?;
        Ok(facts.into())
    }

    async fn list(&self) -> Result<Vec<RuntimeFacts>, RuntimeError> {
        let items: Vec<RunnerSandbox> = self
            .send(reqwest::Method::GET, "/v1/sandboxes", None)
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    async fn start(&self, name: &str) -> Result<RuntimeFacts, RuntimeError> {
        self.action(name, "start").await
    }

    async fn stop(&self, name: &str) -> Result<RuntimeFacts, RuntimeError> {
        self.action(name, "stop").await
    }

    async fn restart(&self, name: &str) -> Result<RuntimeFacts, RuntimeError> {
        self.action(name, "restart").await
    }

    async fn remove(&self, name: &str) -> Result<(), RuntimeError> {
        let url = format!("{}/v1/sandboxes/{name}/remove", self.base_url);
        let response = self
            .client
            .post(url)
            .send()
            .await
            .map_err(|error| RuntimeError::Unavailable(error.to_string()))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(RuntimeError::NotFound(name.to_string()));
        }
        response
            .error_for_status()
            .map_err(|error| RuntimeError::Rejected(error.to_string()))?;
        Ok(())
    }

    async fn logs(&self, name: &str, tail: u32) -> Result<String, RuntimeError> {
        #[derive(Deserialize)]
        struct LogResponse {
            #[serde(default)]
            logs: String,
        }
        let response: LogResponse = self
            .send(
                reqwest::Method::GET,
                &format!("/v1/sandboxes/{name}/logs?tail={tail}"),
                None,
            )
            .await?;
        Ok(response.logs)
    }

    async fn exec(
        &self,
        name: &str,
        command: &[String],
        timeout_seconds: u32,
    ) -> Result<ExecResult, RuntimeError> {
        let payload = serde_json::json!({
            "command": command,
            "timeout_seconds": timeout_seconds.clamp(1, 600),
        });
        let result: ExecResult = self
            .send(
                reqwest::Method::POST,
                &format!("/v1/sandboxes/{name}/exec"),
                Some(&payload),
            )
            .await?;
        Ok(result)
    }

}

impl HttpRunner {
    async fn action(&self, name: &str, action: &str) -> Result<RuntimeFacts, RuntimeError> {
        let facts: RunnerSandbox = self
            .send(
                reqwest::Method::POST,
                &format!("/v1/sandboxes/{name}/{action}"),
                None,
            )
            .await?;
        Ok(facts.into())
    }

}
