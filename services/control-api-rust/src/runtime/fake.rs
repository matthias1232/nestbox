//! An in-memory runtime adapter for tests: no Docker, no network.
//!
//! It behaves like the real adapter where the services depend on it: `create`
//! records an entry, lifecycle actions change the status, and `logs` returns
//! representative lines. It deliberately does not emulate isolation, because a
//! fake that pretends to be more than it is hides the bugs the real adapter
//! would expose.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::domain::runtime::{ExecResult, RuntimeFacts, SandboxSpec};
use crate::runtime::{RuntimeAdapter, RuntimeError};

/// Take a lock, recovering from poisoning.
///
/// A poisoned mutex means another test panicked while holding it. In a fake
/// adapter that is not a reason to abort the current test as well, so the guard
/// is recovered and the caller continues.
fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}


/// In-Memory-Runtime für Unit- und Integrationstests.
#[derive(Default)]
pub struct FakeRuntime {
    sandboxes: Mutex<BTreeMap<String, RuntimeFacts>>,
    specs: Mutex<Vec<SandboxSpec>>,
}

impl FakeRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Die Specs aller erzeugten Sandboxes – für Tests der Umgebungsvariablen.
    pub fn created_specs(&self) -> Vec<SandboxSpec> {
        lock(&self.specs).clone()
    }

    fn with_entry<T>(
        &self,
        name: &str,
        change: impl FnOnce(&mut RuntimeFacts) -> T,
    ) -> Result<T, RuntimeError> {
        let mut guard = lock(&self.sandboxes);
        let entry = guard
            .get_mut(name)
            .ok_or_else(|| RuntimeError::NotFound(name.to_string()))?;
        Ok(change(entry))
    }
}

#[async_trait]
impl RuntimeAdapter for FakeRuntime {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn network(&self) -> &str {
        "nestbox-test"
    }

    async fn health(&self) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn create(&self, spec: SandboxSpec) -> Result<RuntimeFacts, RuntimeError> {
        // Record the spec before its fields move into the facts, so a test can
        // assert what was requested separately from what was reported.
        lock(&self.specs).push(spec.clone());

        let facts = RuntimeFacts {
            id: format!("fake-{}", spec.name),
            name: spec.name.clone(),
            container_name: format!("nestbox-sandbox-{}", spec.name),
            status: "running".to_string(),
            image: spec.image,
            pack_key: spec.pack_key,
            tool_profile: spec.tool_profile,
            ip: "172.30.0.2".to_string(),
            gateway: "172.30.0.1".to_string(),
            uptime_seconds: 1,
            health: "starting".to_string(),
            ..RuntimeFacts::default()
        };
        let mut guard = lock(&self.sandboxes);
        guard.insert(spec.name, facts.clone());
        Ok(facts)
    }

    async fn inspect(&self, name: &str) -> Result<RuntimeFacts, RuntimeError> {
        let guard = lock(&self.sandboxes);
        guard
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::NotFound(name.to_string()))
    }

    async fn list(&self) -> Result<Vec<RuntimeFacts>, RuntimeError> {
        let guard = lock(&self.sandboxes);
        Ok(guard.values().cloned().collect())
    }

    async fn start(&self, name: &str) -> Result<RuntimeFacts, RuntimeError> {
        self.with_entry(name, |entry| {
            entry.status = "running".to_string();
            entry.clone()
        })
    }

    async fn stop(&self, name: &str) -> Result<RuntimeFacts, RuntimeError> {
        self.with_entry(name, |entry| {
            entry.status = "exited".to_string();
            entry.uptime_seconds = 0;
            entry.clone()
        })
    }

    async fn restart(&self, name: &str) -> Result<RuntimeFacts, RuntimeError> {
        self.with_entry(name, |entry| {
            entry.status = "running".to_string();
            entry.clone()
        })
    }

    async fn remove(&self, name: &str) -> Result<(), RuntimeError> {
        let mut guard = lock(&self.sandboxes);
        guard
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| RuntimeError::NotFound(name.to_string()))
    }

    async fn logs(&self, name: &str, tail: u32) -> Result<String, RuntimeError> {
        let guard = lock(&self.sandboxes);
        if !guard.contains_key(name) {
            return Err(RuntimeError::NotFound(name.to_string()));
        }
        Ok(format!("{name}: last {tail} lines (fake output)"))
    }

    async fn exec(
        &self,
        name: &str,
        command: &[String],
        _timeout_seconds: u32,
    ) -> Result<ExecResult, RuntimeError> {
        let guard = lock(&self.sandboxes);
        if !guard.contains_key(name) {
            return Err(RuntimeError::NotFound(name.to_string()));
        }
        // Echo the vector back joined by spaces so a test can assert that the
        // argument boundaries survived the trip.
        Ok(ExecResult {
            exit_code: Some(0),
            output: command.join(" "),
            truncated: false,
        })
    }
}
