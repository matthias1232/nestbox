//! Die Grenze zwischen Controller und Ausführung.
//!
//! Der Controller kennt nur dieses Trait. Welche Technik darunter liegt –
//! Docker über den Sandbox-Runner, später NVIDIA OpenShell – entscheidet allein
//! die Implementierung. Ein Wechsel betrifft genau eine Datei.

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use thiserror::Error;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::domain::metrics::{InventoryProbe, ProcessTable};
use crate::domain::pack_access::ForwardState;
use crate::domain::runtime::{
    BuildRequest, BuildResult, ComponentState, ExecResult, ForwardRequest, HardwareReport,
    ModelDownload, ModelDownloadResult, ModelServe, ModelServeResult, RuntimeFacts, SandboxSpec,
};

/// Fehler aus der Ausführungsschicht.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Sandbox '{0}' existiert im Runtime nicht")]
    NotFound(String),

    #[error("Runtime lehnt die Anfrage ab: {0}")]
    Rejected(String),

    #[error("Runtime ist nicht erreichbar: {0}")]
    Unavailable(String),
}

/// Was der Controller vom Runtime braucht.
#[async_trait]
pub trait RuntimeAdapter: Send + Sync {
    /// Technischer Name, erscheint in `/v1/runtime`.
    fn name(&self) -> &'static str;

    /// Netzwerk, in dem die Sandboxes liegen.
    fn network(&self) -> &str;

    /// Prüft, ob der Runtime erreichbar ist.
    async fn health(&self) -> Result<(), RuntimeError>;

    /// Erzeugt eine Sandbox und liefert ihren Ist-Zustand.
    async fn create(&self, spec: SandboxSpec) -> Result<RuntimeFacts, RuntimeError>;

    /// Liest den Ist-Zustand einer Sandbox.
    async fn inspect(&self, name: &str) -> Result<RuntimeFacts, RuntimeError>;

    /// Listet alle verwalteten Sandboxes.
    async fn list(&self) -> Result<Vec<RuntimeFacts>, RuntimeError>;

    async fn start(&self, name: &str) -> Result<RuntimeFacts, RuntimeError>;
    async fn stop(&self, name: &str) -> Result<RuntimeFacts, RuntimeError>;
    async fn restart(&self, name: &str) -> Result<RuntimeFacts, RuntimeError>;
    async fn remove(&self, name: &str) -> Result<(), RuntimeError>;

    /// Liest die letzten `tail` Logzeilen.
    async fn logs(&self, name: &str, tail: u32) -> Result<String, RuntimeError>;

    /// Run one command inside a running sandbox and return its output.
    ///
    /// The command is an argument vector: the adapter never builds a shell
    /// string, so a name or an argument from a request cannot become syntax.
    async fn exec(
        &self,
        name: &str,
        command: &[String],
        timeout_seconds: u32,
    ) -> Result<ExecResult, RuntimeError>;

    /// What the runner's host has: CPU, memory, GPUs, container GPU support.
    ///
    /// Asked of the runner rather than measured here, because the control
    /// plane may run in a container that sees none of the node's devices.
    async fn hardware(&self) -> Result<HardwareReport, RuntimeError>;

    /// Build an image from a generated Dockerfile.
    async fn build(&self, request: BuildRequest) -> Result<BuildResult, RuntimeError>;

    /// Open an interactive terminal in a sandbox.
    ///
    /// Returns a live channel instead of a request/response pair, because a
    /// shell is a conversation. No request field reaches a command line: the
    /// runner starts a fixed shell with a PTY, and this side only moves bytes.
    async fn processes(&self, name: &str) -> Result<ProcessTable, RuntimeError>;

    /// A fresh inventory snapshot, taken by the container's fixed probe script.
    ///
    /// The script ships with the runner's image and takes no argument, so
    /// nothing in this call can change what it reads. The answer keeps the
    /// probe's sections as a document: the service that stores it decides which
    /// keys survive, because the table has fixed columns and the probe may gain
    /// a section.
    async fn inventory(&self, name: &str) -> Result<InventoryProbe, RuntimeError>;

    /// Build an image from a generated Dockerfile.

    /// Fetch one model artefact onto the node.
    ///
    /// The download runs on the node, not here: the control plane has neither
    /// the disk nor the bandwidth, and a half-written file must not look like a
    /// finished install on a machine it cannot see.
    async fn download_model(&self, request: ModelDownload) -> Result<ModelDownloadResult, RuntimeError>;

    /// Start the model server container for an install.
    async fn serve_model(&self, request: ModelServe) -> Result<ModelServeResult, RuntimeError>;

    /// Stop and remove a model server, if it exists.
    ///
    /// Idempotent: removing a server that is already gone is not an error, so an
    /// uninstall that follows a crashed install still finishes.
    async fn stop_model(&self, model_key: &str) -> Result<(), RuntimeError>;

    async fn open_terminal(&self, name: &str) -> Result<BoxTerminal, RuntimeError>;

    /// Hold exactly these ports open for a sandbox, and accept exactly these
    /// public keys on its SSH port.
    ///
    /// The complete set, not a delta: a retry is the same call, and a withdrawn
    /// port is simply missing from the next one. Returns what the runner opened.
    ///
    /// The default says the runtime cannot publish ports, so an adapter that
    /// never could (a test double, a future OpenShell driver) fails visibly
    /// rather than pretending.
    async fn set_forwards(
        &self,
        _name: &str,
        _forwards: &[ForwardRequest],
        _authorized_keys: &[String],
    ) -> Result<Vec<ForwardState>, RuntimeError> {
        Err(RuntimeError::Unavailable("this runtime cannot publish ports".into()))
    }

    /// What the runner holds open for a sandbox right now, and whether anything
    /// answers behind each port.
    async fn forward_states(&self, _name: &str) -> Result<Vec<ForwardState>, RuntimeError> {
        Err(RuntimeError::Unavailable("this runtime cannot publish ports".into()))
    }

    /// Close every port of a sandbox. Idempotent.
    async fn clear_forwards(&self, _name: &str) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// The appliance's own containers: state and health.
    async fn components(&self) -> Result<Vec<ComponentState>, RuntimeError> {
        Err(RuntimeError::Unavailable("this runtime does not report components".into()))
    }

    /// The last lines of one appliance component. `service` must be one of the
    /// names `components` returns; the runner refuses any other.
    async fn component_logs(&self, _service: &str, _tail: u32) -> Result<String, RuntimeError> {
        Err(RuntimeError::Unavailable("this runtime does not report components".into()))
    }
}

/// One frame in the direction runner → control API.
///
/// Text rather than bytes because both ends of this hop speak WebSocket, whose
/// text frames are UTF-8 by definition. The PTY's stream is decoded with
/// replacement at the runner, so an invalid sequence degrades to a glyph
/// instead of ending the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalMessage {
    /// What the shell printed.
    Output(String),
    /// The session ended, with the reason the runner named.
    Closed(String),
}

/// One end of a live terminal, owned by whoever opened it.
///
/// A trait rather than a concrete type so the session rules — the limits, the
/// idle clock, the audit row — can be tested without a container, and so the
/// day the runner is an OpenShell gateway only one adapter changes.
#[async_trait]
pub trait TerminalSocket: Send {
    /// The next frame, or `None` when the stream ended without a reason.
    async fn recv(&mut self) -> Result<Option<TerminalMessage>, RuntimeError>;

    /// Send keyboard input to the PTY.
    async fn send_input(&mut self, text: &str) -> Result<(), RuntimeError>;

    /// Tell the PTY how wide and how tall the browser's window is.
    async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), RuntimeError>;

    /// End the session, saying why. Failure is ignored: the caller is closing.
    async fn close(&mut self, reason: &str);
}

/// A terminal channel, whatever holds the other end.
pub type BoxTerminal = Box<dyn TerminalSocket>;

/// The close reason in a text frame, if that frame is a control frame.
///
/// The runner sends one control frame before it closes — `{"type":"close",
/// "reason":"idle"}` — and everything else it sends is output. The rule is
/// therefore narrow on purpose: a frame must be a JSON *object* whose `type` is
/// `close`. A shell that prints `{"type":"close"}` verbatim would be
/// misread, which is a documented and accepted edge: the alternative is an
/// escape sequence the terminal cannot carry without corrupting binary output.
pub fn close_control_frame(text: &str) -> Option<String> {
    if !text.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if value.get("type")?.as_str()? != "close" {
        return None;
    }
    Some(
        value
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("error")
            .to_string(),
    )
}

/// A terminal attached to a sandbox through the runner's WebSocket.
pub struct RunnerTerminal {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
}

impl RunnerTerminal {
    /// Wrap an upgraded WebSocket.
    pub fn new(socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>) -> Self {
        Self { socket }
    }
}

#[async_trait]
impl TerminalSocket for RunnerTerminal {
    async fn recv(&mut self) -> Result<Option<TerminalMessage>, RuntimeError> {
        loop {
            let message = match self.socket.next().await {
                Some(Ok(message)) => message,
                Some(Err(error)) => return Err(RuntimeError::Unavailable(error.to_string())),
                None => return Ok(None),
            };
            match message {
                Message::Text(text) => {
                    let raw = text.as_str();
                    return Ok(Some(match close_control_frame(raw) {
                        Some(reason) => TerminalMessage::Closed(reason),
                        None => TerminalMessage::Output(raw.to_string()),
                    }));
                }
                Message::Binary(bytes) => {
                    return Ok(Some(TerminalMessage::Output(
                        String::from_utf8_lossy(&bytes).to_string(),
                    )));
                }
                Message::Close(frame) => {
                    let reason = frame
                        .map(|frame| frame.reason.to_string())
                        .filter(|reason| !reason.is_empty())
                        .unwrap_or_else(|| "error".to_string());
                    return Ok(Some(TerminalMessage::Closed(reason)));
                }
                // Protocol bookkeeping, not a frame the session sees.
                // tungstenite queues the pong for a ping and writes it while
                // the stream is polled, which this loop always is.
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
            }
        }
    }

    async fn send_input(&mut self, text: &str) -> Result<(), RuntimeError> {
        self.socket
            .send(Message::Text(text.to_string().into()))
            .await
            .map_err(|error| RuntimeError::Unavailable(error.to_string()))
    }

    async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), RuntimeError> {
        let frame = serde_json::json!({ "type": "resize", "cols": cols, "rows": rows });
        self.socket
            .send(Message::Text(frame.to_string().into()))
            .await
            .map_err(|error| RuntimeError::Unavailable(error.to_string()))
    }

    async fn close(&mut self, reason: &str) {
        // A close frame is the polite form; dropping the socket is the form that
        // works. Both are attempted, and neither is allowed to fail the session
        // that is already over.
        let frame = tokio_tungstenite::tungstenite::protocol::CloseFrame {
            code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
            reason: reason.to_string().into(),
        };
        let _ = self.socket.send(Message::Close(Some(frame))).await;
        let _ = self.socket.close(None).await;
    }
}

pub mod fake;
pub mod http_runner;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_close_frame_names_its_reason() {
        assert_eq!(
            close_control_frame(r#"{"type":"close","reason":"idle"}"#),
            Some("idle".to_string())
        );
        assert_eq!(
            close_control_frame(r#"{"type":"close","reason":"sandbox_stopped"}"#),
            Some("sandbox_stopped".to_string())
        );
    }

    #[test]
    fn a_close_frame_without_a_reason_is_an_error() {
        assert_eq!(
            close_control_frame(r#"{"type":"close"}"#),
            Some("error".to_string())
        );
    }

    #[tokio::test]
    async fn an_idle_terminal_answers_a_ping() {
        // The runner pings every twenty seconds. This side is read-only while a
        // shell sits at its prompt, so the pong has to be flushed explicitly —
        // without that the peer gives up after a minute and an idle terminal
        // dies for no reason a user could see.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|error| panic!("a local port: {error}"));
        let address = listener.local_addr().unwrap_or_else(|error| panic!("{error}"));
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap_or_else(|error| panic!("{error}"));
            let mut socket = tokio_tungstenite::accept_async(stream)
                .await
                .unwrap_or_else(|error| panic!("handshake: {error}"));
            socket
                .send(Message::Ping(vec![1, 2, 3].into()))
                .await
                .unwrap_or_else(|error| panic!("ping: {error}"));
            let answer = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
                .await
                .unwrap_or_else(|_| panic!("no answer within five seconds"));
            matches!(answer, Some(Ok(Message::Pong(_))))
        });

        let (stream, _) =
            tokio_tungstenite::connect_async(format!("ws://{address}"))
                .await
                .unwrap_or_else(|error| panic!("connect: {error}"));
        let mut terminal = RunnerTerminal::new(stream);
        let reader = tokio::spawn(async move {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(3), terminal.recv()).await;
        });

        assert!(server.await.unwrap_or(false), "the pong must reach the peer");
        let _ = reader.await;
    }

    #[test]
    fn terminal_output_is_not_mistaken_for_a_control_frame() {
        // A shell prints all of this, and none of it may end the session.
        for text in [
            "ls -la\r\n",
            "{",
            "{}",
            "{not json",
            r#"{"cols":80,"rows":24}"#,
            r#"{"type":"output"}"#,
            r#"["close"]"#,
            r#"{"type":1,"reason":"idle"}"#,
        ] {
            assert_eq!(close_control_frame(text), None, "{text}");
        }
    }
}
