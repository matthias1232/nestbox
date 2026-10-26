//! Regeln rund um Sandboxes: anlegen, auflisten, steuern.

use chrono::Utc;
use uuid::Uuid;

use crate::domain::pack;
use crate::domain::runtime::SandboxSpec;
use crate::domain::sandbox::{LifecycleAction, Sandbox, SandboxInput, SandboxLogs};
use crate::error::ApiError;
use crate::runtime::RuntimeError;
use crate::services::providers;
use crate::state::AppState;

/// Prüft den Sandbox-Namen: klein, Ziffern, Bindestriche.
pub fn validate_name(name: &str) -> Result<(), ApiError> {
    let allowed = !name.is_empty()
        && name.len() <= 63
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });
    let clean_edges = !name.starts_with('-') && !name.ends_with('-');

    if !allowed || !clean_edges {
        return Err(ApiError::Invalid(
            "Name muss ^[a-z0-9-]{1,63}$ erfüllen und darf nicht mit '-' beginnen oder enden."
                .to_string(),
        ));
    }
    Ok(())
}

/// Erzeugt eine Sandbox aus einem Pack und startet sie im Runtime.
pub async fn create(state: &AppState, input: SandboxInput) -> Result<Sandbox, ApiError> {
    validate_name(&input.name)?;

    let selected = pack::find(&input.pack_key)
        .ok_or_else(|| ApiError::Invalid(format!("Unbekanntes Pack '{}'.", input.pack_key)))?;

    {
        let data = state.data.read().await;
        if data.sandboxes.values().any(|item| item.name == input.name) {
            return Err(ApiError::Conflict {
                resource: "sandbox",
                name: input.name,
            });
        }
    }

    let tool_profile = input
        .tool_profile
        .clone()
        .unwrap_or_else(|| selected.tool_profile.clone());

    {
        let data = state.data.read().await;
        if !data.tool_profiles.contains_key(&tool_profile) {
            return Err(ApiError::Invalid(format!(
                "Unbekanntes Tool-Profil '{tool_profile}'."
            )));
        }
    }

    // Provider-Zugangsdaten werden nur als Umgebungswerte weitergereicht.
    let mut environment: Vec<(String, String)> = input.environment.into_iter().collect();
    if let Some(provider_id) = &input.provider_id {
        let credentials = providers::credentials(state, provider_id).await?;
        environment.push(("OPENAI_BASE_URL".to_string(), credentials.base_url));
        environment.push(("OPENAI_MODEL".to_string(), credentials.model));
        if let Some(key) = credentials.api_key {
            environment.push(("OPENAI_API_KEY".to_string(), key));
        }
    }

    // Dienste, die ein Token verlangen, bekommen ein serverseitig erzeugtes.
    // Das Token liegt im SecretStore und wird nie über die API ausgegeben.
    if input.pack_key == "openclaw-gateway"
        && !environment
            .iter()
            .any(|(key, _)| key == "OPENCLAW_GATEWAY_TOKEN")
    {
        let token = {
            let mut secrets = state.secrets.write().await;
            secrets
                .sandbox_tokens
                .entry(input.name.clone())
                .or_insert_with(|| Uuid::new_v4().to_string())
                .clone()
        };
        environment.push(("OPENCLAW_GATEWAY_TOKEN".to_string(), token));
    }

    // Reihenfolge: explizite Eingabe > Empfehlung des Packs > sicherer Default.
    let memory_limit = input
        .memory_limit
        .or(selected.default_memory)
        .unwrap_or_else(|| "1g".to_string());
    let cpu_limit = input.cpu_limit.or(selected.default_cpu).unwrap_or(1.0);
    let pids_limit = input.pids_limit.unwrap_or(256);

    let spec = SandboxSpec {
        name: input.name.clone(),
        image: selected.image,
        command: selected.command,
        environment,
        pack_key: input.pack_key.clone(),
        tool_profile: tool_profile.clone(),
        memory_limit,
        cpu_limit,
        pids_limit,
    };

    let facts = state
        .runtime
        .create(spec)
        .await
        .map_err(|error| runtime_error(error, &input.name))?;

    let sandbox = Sandbox {
        id: Uuid::new_v4().to_string(),
        name: input.name.clone(),
        pack_key: input.pack_key,
        tool_profile,
        provider_id: input.provider_id,
        status: facts.status.clone(),
        created_at: Utc::now().to_rfc3339(),
        runtime: Some(facts),
    };

    state
        .data
        .write()
        .await
        .sandboxes
        .insert(sandbox.id.clone(), sandbox.clone());

    state
        .audit(
            "sandbox.created",
            &sandbox.id,
            serde_json::json!({ "name": sandbox.name, "pack_key": sandbox.pack_key }),
        )
        .await;

    Ok(sandbox)
}

/// Alle Sandboxes mit aktuellem Ist-Zustand.
pub async fn list(state: &AppState) -> Vec<Sandbox> {
    let mut items: Vec<Sandbox> = state
        .data
        .read()
        .await
        .sandboxes
        .values()
        .cloned()
        .collect();

    for item in &mut items {
        match state.runtime.inspect(&item.name).await {
            Ok(facts) => {
                item.status = facts.status.clone();
                item.runtime = Some(facts);
            }
            Err(RuntimeError::NotFound(_)) => item.status = "missing".to_string(),
            Err(_) => item.status = "unknown".to_string(),
        }
    }
    items
}

/// Eine einzelne Sandbox mit Ist-Zustand.
pub async fn get(state: &AppState, id: &str) -> Result<Sandbox, ApiError> {
    let mut sandbox = {
        let data = state.data.read().await;
        data.sandboxes
            .get(id)
            .cloned()
            .or_else(|| {
                data.sandboxes
                    .values()
                    .find(|item| item.name == id)
                    .cloned()
            })
            .ok_or_else(|| ApiError::NotFound {
                resource: "sandbox",
                id: id.to_string(),
            })?
    };

    match state.runtime.inspect(&sandbox.name).await {
        Ok(facts) => {
            sandbox.status = facts.status.clone();
            sandbox.runtime = Some(facts);
        }
        Err(RuntimeError::NotFound(_)) => sandbox.status = "missing".to_string(),
        Err(_) => sandbox.status = "unknown".to_string(),
    }
    Ok(sandbox)
}

/// Führt eine Lifecycle-Aktion aus.
pub async fn lifecycle(
    state: &AppState,
    id: &str,
    action: LifecycleAction,
) -> Result<Sandbox, ApiError> {
    let sandbox = get(state, id).await?;

    if matches!(action, LifecycleAction::Remove) {
        state
            .runtime
            .remove(&sandbox.name)
            .await
            .map_err(|error| runtime_error(error, &sandbox.name))?;
        state.data.write().await.sandboxes.remove(&sandbox.id);
        state
            .audit(
                "sandbox.removed",
                &sandbox.id,
                serde_json::json!({ "name": sandbox.name }),
            )
            .await;
        return Ok(Sandbox {
            status: "deleted".to_string(),
            runtime: None,
            ..sandbox
        });
    }

    let facts = match action {
        LifecycleAction::Start => state.runtime.start(&sandbox.name).await,
        LifecycleAction::Stop => state.runtime.stop(&sandbox.name).await,
        LifecycleAction::Restart => state.runtime.restart(&sandbox.name).await,
        LifecycleAction::Remove => unreachable!("bereits oben behandelt"),
    }
    .map_err(|error| runtime_error(error, &sandbox.name))?;

    let updated = {
        let mut data = state.data.write().await;
        let entry = data
            .sandboxes
            .entry(sandbox.id.clone())
            .or_insert_with(|| sandbox.clone());
        entry.status = facts.status.clone();
        entry.runtime = Some(facts);
        entry.clone()
    };

    state
        .audit(
            &format!("sandbox.{}", action.as_str()),
            &updated.id,
            serde_json::json!({ "name": updated.name, "status": updated.status }),
        )
        .await;

    Ok(updated)
}

/// Liest die letzten Logzeilen einer Sandbox.
pub async fn logs(state: &AppState, id: &str, tail: u32) -> Result<SandboxLogs, ApiError> {
    let sandbox = get(state, id).await?;
    let tail = tail.clamp(1, 1000);
    let output = state
        .runtime
        .logs(&sandbox.name, tail)
        .await
        .map_err(|error| runtime_error(error, &sandbox.name))?;

    Ok(SandboxLogs {
        name: sandbox.name,
        tail,
        logs: output,
    })
}

fn runtime_error(error: RuntimeError, name: &str) -> ApiError {
    match error {
        RuntimeError::NotFound(_) => ApiError::NotFound {
            resource: "sandbox",
            id: name.to_string(),
        },
        RuntimeError::Rejected(message) => ApiError::Invalid(message),
        RuntimeError::Unavailable(message) => ApiError::Runtime(message),
    }
}
