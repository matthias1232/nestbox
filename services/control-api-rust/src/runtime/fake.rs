//! Ein Runtime-Adapter für Tests – ohne Docker, ohne Netzwerk.
//!
//! Er verhält sich wie der echte Adapter: `create` legt einen Eintrag an,
//! Lifecycle-Aktionen ändern den Status, `logs` liefert Beispielzeilen.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::domain::runtime::{RuntimeFacts, SandboxSpec};
use crate::runtime::{RuntimeAdapter, RuntimeError};

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
        self.specs.lock().expect("Mutex vergiftet").clone()
    }

    fn with_entry<T>(&self, name: &str, change: impl FnOnce(&mut RuntimeFacts) -> T) -> Result<T, RuntimeError> {
        let mut guard = self.sandboxes.lock().expect("Mutex vergiftet");
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
        // Für Tests: die Spec sichern, bevor Felder in die Fakten wandern.
        self.specs
            .lock()
            .expect("Mutex vergiftet")
            .push(spec.clone());

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
        let mut guard = self.sandboxes.lock().expect("Mutex vergiftet");
        guard.insert(spec.name, facts.clone());
        Ok(facts)
    }

    async fn inspect(&self, name: &str) -> Result<RuntimeFacts, RuntimeError> {
        let guard = self.sandboxes.lock().expect("Mutex vergiftet");
        guard
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::NotFound(name.to_string()))
    }

    async fn list(&self) -> Result<Vec<RuntimeFacts>, RuntimeError> {
        let guard = self.sandboxes.lock().expect("Mutex vergiftet");
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
        let mut guard = self.sandboxes.lock().expect("Mutex vergiftet");
        guard
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| RuntimeError::NotFound(name.to_string()))
    }

    async fn logs(&self, name: &str, tail: u32) -> Result<String, RuntimeError> {
        let guard = self.sandboxes.lock().expect("Mutex vergiftet");
        if !guard.contains_key(name) {
            return Err(RuntimeError::NotFound(name.to_string()));
        }
        Ok(format!("{name}: letzte {tail} Zeilen (Testausgabe)"))
    }
}
