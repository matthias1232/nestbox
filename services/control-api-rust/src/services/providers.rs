//! Provider-Profile und ihre Secrets.
//!
//! Öffentliche API liefert nur Metadaten. Der Key wird beim Anlegen einmal
//! entgegengenommen, im [`crate::state::SecretStore`] abgelegt und danach
//! ausschließlich als Umgebungswert einer Sandbox verwendet.

use chrono::Utc;
use uuid::Uuid;

use crate::domain::provider::{ProviderInput, ProviderProfile};
use crate::error::ApiError;
use crate::state::AppState;

/// Zugangsdaten für die Injektion in eine Sandbox.
pub struct ProviderCredentials {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

/// Öffentliche Liste aller Profile.
pub async fn list(state: &AppState) -> Vec<ProviderProfile> {
    state.data.read().await.providers.values().cloned().collect()
}

/// Legt ein Profil an; der Key landet getrennt von den Metadaten.
pub async fn create(state: &AppState, input: ProviderInput) -> Result<ProviderProfile, ApiError> {
    let name = input.name.trim().to_string();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(ApiError::Invalid(
            "Provider-Name muss ^[a-z0-9-]{1,63}$ erfüllen.".to_string(),
        ));
    }
    if input.base_url.trim().is_empty() || input.model.trim().is_empty() {
        return Err(ApiError::Invalid(
            "base_url und model dürfen nicht leer sein.".to_string(),
        ));
    }

    {
        let data = state.data.read().await;
        if data.providers.contains_key(&name) {
            return Err(ApiError::Conflict {
                resource: "provider",
                name,
            });
        }
    }

    let profile = ProviderProfile {
        id: Uuid::new_v4().to_string(),
        name: name.clone(),
        kind: input.kind,
        base_url: input.base_url.trim().to_string(),
        model: input.model.trim().to_string(),
        api_key_configured: input.api_key.as_deref().is_some_and(|key| !key.is_empty()),
        created_at: Utc::now().to_rfc3339(),
    };

    if let Some(key) = input.api_key.filter(|key| !key.is_empty()) {
        state
            .secrets
            .write()
            .await
            .provider_api_keys
            .insert(name.clone(), key);
    }

    state
        .data
        .write()
        .await
        .providers
        .insert(name.clone(), profile.clone());

    state
        .audit(
            "provider.created",
            &profile.id,
            serde_json::json!({ "name": name, "base_url": profile.base_url, "model": profile.model }),
        )
        .await;

    Ok(profile)
}

/// Liest die Zugangsdaten für eine Sandbox.
pub async fn credentials(state: &AppState, name: &str) -> Result<ProviderCredentials, ApiError> {
    let profile = {
        let data = state.data.read().await;
        data.providers
            .get(name)
            .cloned()
            .ok_or_else(|| ApiError::Invalid(format!("Unbekannter Provider '{name}'.")))?
    };
    let api_key = state.secrets.read().await.provider_api_keys.get(name).cloned();

    Ok(ProviderCredentials {
        base_url: profile.base_url,
        model: profile.model,
        api_key,
    })
}
