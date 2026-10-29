//! Docker-Adapter über den Sandbox-Runner.
//!
//! Der Runner ist ein kleiner Dienst mit Docker-Socket und führt die
//! Container-Operationen aus. Diese Datei übersetzt zwischen dem
//! [`RuntimeAdapter`]-Vertrag und dessen HTTP-Endpunkten.

use async_trait::async_trait;
use reqwest::StatusCode;
use serde::Deserialize;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use chrono::{DateTime, Utc};
use crate::domain::metrics::{InventoryProbe, ProcessTable};
use crate::domain::pack_access::ForwardState;
use crate::domain::runtime::{
    BuildRequest, BuildResult, ComponentState, ExecResult, ForwardRequest, HardwareReport,
    ModelDownload, ModelDownloadResult, ModelServe, ModelServeResult, NetworkInterface,
    RuntimeFacts, RuntimeResources, SandboxSpec, SshCredential,
};
use crate::runtime::{BoxTerminal, RunnerTerminal, RuntimeAdapter, RuntimeError};

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
    #[serde(default)]
    supervisor_status: String,
    #[serde(default)]
    supervisor_image: String,
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
            supervisor_status: value.supervisor_status,
            supervisor_image: value.supervisor_image,
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

    /// The same host and port, with the WebSocket scheme.
    fn ws_base_url(&self) -> String {
        if let Some(rest) = self.base_url.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.base_url.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            format!("ws://{}", self.base_url)
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
            // `openshell` puts the sandbox on the OpenShell gateway; anything else is
            // the development Docker adapter. One value per deployment.
            "runtime": std::env::var("NESTBOX_SANDBOX_RUNTIME")
                .ok()
                .filter(|value| value == "openshell")
                .unwrap_or_else(|| "docker-development".to_string()),
            "pack_key": spec.pack_key,
            "tool_profile": spec.tool_profile,
            "environment": environment,
            "memory_limit": spec.memory_limit,
            "cpu_limit": spec.cpu_limit,
            "pids_limit": spec.pids_limit,
            "mounts": spec.mounts,
            "gpu_count": spec.gpu_count,
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

    async fn hardware(&self) -> Result<HardwareReport, RuntimeError> {
        self.send(reqwest::Method::GET, "/v1/hardware", None).await
    }

    async fn build(&self, request: BuildRequest) -> Result<BuildResult, RuntimeError> {
        let payload = serde_json::to_value(&request)
            .map_err(|error| RuntimeError::Rejected(error.to_string()))?;
        self.send(reqwest::Method::POST, "/v1/builds", Some(&payload))
            .await
    }

    async fn processes(&self, name: &str) -> Result<ProcessTable, RuntimeError> {
        self.send(
            reqwest::Method::GET,
            &format!("/v1/sandboxes/{name}/processes"),
            None,
        )
        .await
    }

    async fn inventory(&self, name: &str) -> Result<InventoryProbe, RuntimeError> {
        /// The runner's envelope: its own name for the sandbox, the moment, the
        /// sections, and why a snapshot may be partial.
        #[derive(Deserialize)]
        struct RunnerInventory {
            taken_at: DateTime<Utc>,
            inventory: serde_json::Value,
            #[serde(default)]
            note: Option<String>,
        }

        let answer: RunnerInventory = self
            .send(
                reqwest::Method::GET,
                &format!("/v1/sandboxes/{name}/inventory"),
                None,
            )
            .await?;

        Ok(InventoryProbe {
            taken_at: answer.taken_at,
            facts: answer.inventory,
            note: answer.note,
        })
    }

    async fn download_model(
        &self,
        request: ModelDownload,
    ) -> Result<ModelDownloadResult, RuntimeError> {
        let payload = serde_json::to_value(&request)
            .map_err(|error| RuntimeError::Rejected(error.to_string()))?;
        self.send(reqwest::Method::POST, "/v1/models/download", Some(&payload))
            .await
    }

    async fn serve_model(&self, request: ModelServe) -> Result<ModelServeResult, RuntimeError> {
        let payload = serde_json::to_value(&request)
            .map_err(|error| RuntimeError::Rejected(error.to_string()))?;
        self.send(reqwest::Method::POST, "/v1/models/serve", Some(&payload))
            .await
    }

    async fn stop_model(&self, model_key: &str) -> Result<(), RuntimeError> {
        // The runner answers 404 when the container is already gone. That is
        // the state the caller wants, so it is not an error here.
        match self
            .send::<serde_json::Value>(
                reqwest::Method::DELETE,
                &format!("/v1/models/serve/{model_key}"),
                None,
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(RuntimeError::NotFound(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn set_forwards(
        &self,
        name: &str,
        forwards: &[ForwardRequest],
        authorized_keys: &[String],
        credentials: &[SshCredential],
    ) -> Result<Vec<ForwardState>, RuntimeError> {
        #[derive(Deserialize)]
        struct Answer {
            #[serde(default)]
            forwards: Vec<ForwardState>,
        }
        let payload = serde_json::json!({
            "forwards": forwards,
            "authorized_keys": authorized_keys,
            "credentials": credentials,
        });
        let answer: Answer = self
            .send(reqwest::Method::PUT, &format!("/v1/sandboxes/{name}/forwards"), Some(&payload))
            .await?;
        Ok(answer.forwards)
    }

    async fn forward_states(&self, name: &str) -> Result<Vec<ForwardState>, RuntimeError> {
        #[derive(Deserialize)]
        struct Answer {
            #[serde(default)]
            forwards: Vec<ForwardState>,
        }
        let answer: Answer = self
            .send(reqwest::Method::GET, &format!("/v1/sandboxes/{name}/forwards"), None)
            .await?;
        Ok(answer.forwards)
    }

    async fn clear_forwards(&self, name: &str) -> Result<(), RuntimeError> {
        let _: serde_json::Value = self
            .send(reqwest::Method::DELETE, &format!("/v1/sandboxes/{name}/forwards"), None)
            .await?;
        Ok(())
    }

    async fn host_load(&self) -> Result<serde_json::Value, RuntimeError> {
        self.send(reqwest::Method::GET, "/v1/host/load", None).await
    }

    async fn update_resources(
        &self,
        name: &str,
        memory_bytes: Option<i64>,
        cpus: Option<f64>,
        pids_limit: Option<i64>,
    ) -> Result<(), RuntimeError> {
        let payload = serde_json::json!({
            "memory_bytes": memory_bytes,
            "cpus": cpus,
            "pids_limit": pids_limit,
        });
        let _: serde_json::Value = self
            .send(reqwest::Method::PATCH, &format!("/v1/sandboxes/{name}/resources"), Some(&payload))
            .await?;
        Ok(())
    }

    async fn backup_create(&self, root_path: &str, backup_id: &str) -> Result<(i64, String), RuntimeError> {
        #[derive(Deserialize)]
        struct Answer {
            bytes: i64,
            sha256: String,
        }
        let payload = serde_json::json!({ "root_path": root_path, "backup_id": backup_id });
        let answer: Answer = self.send(reqwest::Method::POST, "/v1/backups", Some(&payload)).await?;
        Ok((answer.bytes, answer.sha256))
    }

    async fn backup_restore(
        &self,
        backup_id: &str,
        root_path: &str,
        sha256: &str,
        replace: bool,
    ) -> Result<serde_json::Value, RuntimeError> {
        let payload = serde_json::json!({ "root_path": root_path, "sha256": sha256, "replace": replace });
        self.send(reqwest::Method::POST, &format!("/v1/backups/{backup_id}/restore"), Some(&payload)).await
    }

    async fn backup_delete(&self, backup_id: &str) -> Result<(), RuntimeError> {
        let _: serde_json::Value = self
            .send(reqwest::Method::DELETE, &format!("/v1/backups/{backup_id}"), None)
            .await?;
        Ok(())
    }

    async fn openshell_view(&self, name: &str) -> Result<serde_json::Value, RuntimeError> {
        self.send(reqwest::Method::GET, &format!("/v1/sandboxes/{name}/openshell"), None).await
    }

    async fn openshell_events(&self, since_seconds: u32) -> Result<Vec<serde_json::Value>, RuntimeError> {
        #[derive(Deserialize)]
        struct Answer {
            #[serde(default)]
            events: Vec<serde_json::Value>,
        }
        let answer: Answer = self
            .send(reqwest::Method::GET, &format!("/v1/openshell/events?since={since_seconds}"), None)
            .await?;
        Ok(answer.events)
    }

    async fn set_openshell_policy(
        &self,
        name: &str,
        document: &serde_json::Value,
    ) -> Result<(), RuntimeError> {
        let payload = serde_json::json!({ "document": document });
        let _: serde_json::Value = self
            .send(reqwest::Method::PUT, &format!("/v1/sandboxes/{name}/openshell-policy"), Some(&payload))
            .await?;
        Ok(())
    }

    async fn components(&self) -> Result<Vec<ComponentState>, RuntimeError> {
        #[derive(Deserialize)]
        struct Answer {
            #[serde(default)]
            components: Vec<ComponentState>,
        }
        let answer: Answer = self.send(reqwest::Method::GET, "/v1/system/components", None).await?;
        Ok(answer.components)
    }

    async fn component_logs(&self, service: &str, tail: u32) -> Result<String, RuntimeError> {
        #[derive(Deserialize)]
        struct Answer {
            #[serde(default)]
            logs: String,
        }
        let answer: Answer = self
            .send(
                reqwest::Method::GET,
                &format!("/v1/system/components/{service}/logs?tail={tail}"),
                None,
            )
            .await?;
        Ok(answer.logs)
    }

    async fn open_terminal(&self, name: &str) -> Result<BoxTerminal, RuntimeError> {
        let url = format!("{}/v1/sandboxes/{}/terminal", self.ws_base_url(), name);
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|error| RuntimeError::Rejected(format!("invalid terminal URL: {error}")))?;
        // The token travels in a header, not in a query string: a URL ends up in
        // access logs and in every error message that quotes the request. The
        // subprotocol the runner answers with stays first, so the negotiation
        // still selects it.
        // Resolved per terminal, not cached at start (ADR 0016): rotating the
        // file takes effect for the next session instead of requiring a restart
        // of the API, and a token never sits in the process longer than the
        // handshake that needs it.
        let token = read_node_token();
        let protocol = match &token {
            Some(token) => format!("{TERMINAL_SUBPROTOCOL}, {token}"),
            None => TERMINAL_SUBPROTOCOL.to_string(),
        };
        request.headers_mut().insert(
            "sec-websocket-protocol",
            reqwest::header::HeaderValue::from_str(&protocol).map_err(|_| {
                RuntimeError::Rejected("the node token is not a valid header value".to_string())
            })?,
        );
        let upgrade = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio_tungstenite::connect_async(request),
        )
        .await
        .map_err(|_| RuntimeError::Unavailable("the runner did not answer the upgrade".into()))?
        .map_err(|error| match error {
            tokio_tungstenite::tungstenite::Error::Http(response) => {
                let status = response.status().as_u16();
                match status {
                    404 => RuntimeError::NotFound(name.to_string()),
                    // The runner demands the node token, and the message names
                    // the variable: a terminal that silently refuses to open is
                    // the kind of failure that costs an afternoon.
                    401 | 403 => RuntimeError::Rejected(
                        "the runner refused the node token; NESTBOX_NODE_TOKEN must be the \
                         same on the control API and on the sandbox runner"
                            .to_string(),
                    ),
                    // The runner's handshake parser refuses a malformed
                    // `Sec-WebSocket-Protocol` before the endpoint sees it, so a
                    // token with `+`, `/` or `=` arrives here as a bare 400.
                    400 => RuntimeError::Rejected(
                        "the runner rejected the terminal handshake (400); a node token must \
                         be a single token of letters and digits — hex, not base64"
                            .to_string(),
                    ),
                    _ => RuntimeError::Rejected(format!(
                        "the runner refused the terminal ({status})"
                    )),
                }
            }
            other => RuntimeError::Unavailable(other.to_string()),
        })?;
        Ok(Box::new(RunnerTerminal::new(upgrade.0)))
    }
}

/// The subprotocol the terminal speaks. Named so both ends agree, and so a
/// proxy that inspects the upgrade sees a protocol rather than a bare socket.
const TERMINAL_SUBPROTOCOL: &str = "nestbox-terminal";

/// The node token, from a file when one is configured and from the environment
/// otherwise.
///
/// The file wins because an environment variable is visible in `docker inspect`
/// and in a crash dump (ADR 0005), and this token is the only thing between a
/// compromised sandbox and a shell in its neighbour.
fn read_node_token() -> Option<String> {
    if let Ok(path) = std::env::var("NESTBOX_NODE_TOKEN_FILE") {
        return match std::fs::read_to_string(&path) {
            Ok(value) => Some(value.trim().to_string()).filter(|token| !token.is_empty()),
            Err(error) => {
                tracing::warn!(path = %path, error = %error, "cannot read the node token file");
                None
            }
        };
    }
    std::env::var("NESTBOX_NODE_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|token| !token.is_empty())
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
