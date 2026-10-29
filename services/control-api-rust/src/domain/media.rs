//! Media rendering: which model answers an image, video or music request.
//!
//! The rule that matters is one sentence: **a sandbox may choose a model, but
//! only from the set the environment allowed, and only when the environment has
//! a usable default or the sandbox has an explicit one.** Everything else here
//! is the vocabulary for that sentence.
//!
//! Pure on purpose: the console, the API and the MCP bridge all ask the same
//! question ("what would answer a video request for this sandbox?"), and the
//! answer must not depend on which of them asked.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// The three things a text model may ask to be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
    Music,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Music => "music",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "image" => Some(Self::Image),
            "video" => Some(Self::Video),
            "music" => Some(Self::Music),
            _ => None,
        }
    }

    /// The MCP tool name an agent calls for this kind.
    ///
    /// Kept next to the kind so a tool rename cannot drift from the server that
    /// advertises it.
    pub fn tool_name(self) -> &'static str {
        match self {
            Self::Image => "render_image",
            Self::Video => "render_video",
            Self::Music => "render_music",
        }
    }

    /// The provider path used when the provider profile does not override it.
    ///
    /// OpenAI-shaped endpoints use `/images/generations`; video and music have
    /// no single standard, so the documented defaults are the ones the media
    /// templates in this repository speak. A provider that differs is given an
    /// explicit endpoint in its profile labels (`media_path_video`, …).
    pub fn default_path(self) -> &'static str {
        match self {
            Self::Image => "/images/generations",
            Self::Video => "/videos/generations",
            Self::Music => "/audio/music",
        }
    }
}

/// One model on one provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MediaTarget {
    pub provider_id: Uuid,
    pub model: String,
}

impl MediaTarget {
    /// A target with a bounded model name. An empty or over-long name is a
    /// validation error rather than a request the provider will reject later.
    pub fn new(provider_id: Uuid, model: &str) -> Result<Self, String> {
        let model = model.trim();
        if model.is_empty() {
            return Err("A media model needs a model name.".into());
        }
        if model.len() > 200 {
            return Err("A media model name may be at most 200 characters.".into());
        }
        Ok(Self { provider_id, model: model.to_string() })
    }
}

/// The environment default and the set a sandbox owner may choose from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct MediaSettings {
    pub enabled: bool,
    pub image: Option<MediaTarget>,
    pub video: Option<MediaTarget>,
    pub music: Option<MediaTarget>,
    /// Empty means "every provider in this environment"; a non-empty list is
    /// the restriction an admin made.
    pub allowed_provider_ids: Vec<Uuid>,
}

impl MediaSettings {
    pub fn target(&self, kind: MediaKind) -> Option<&MediaTarget> {
        match kind {
            MediaKind::Image => self.image.as_ref(),
            MediaKind::Video => self.video.as_ref(),
            MediaKind::Music => self.music.as_ref(),
        }
    }

    /// Whether a provider may be chosen by a sandbox owner.
    ///
    /// `provider_exists` is passed in because "allowed" has two halves — the
    /// admin's list and the provider still existing — and only the caller can
    /// answer the second without a query here.
    pub fn permits(&self, provider_id: Uuid, provider_exists: bool) -> bool {
        provider_exists
            && (self.allowed_provider_ids.is_empty() || self.allowed_provider_ids.contains(&provider_id))
    }
}

/// A sandbox's own choice. A missing kind inherits the environment default.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct SandboxMediaSettings {
    pub image: Option<MediaTarget>,
    pub video: Option<MediaTarget>,
    pub music: Option<MediaTarget>,
}

impl SandboxMediaSettings {
    pub fn target(&self, kind: MediaKind) -> Option<&MediaTarget> {
        match kind {
            MediaKind::Image => self.image.as_ref(),
            MediaKind::Video => self.video.as_ref(),
            MediaKind::Music => self.music.as_ref(),
        }
    }
}

/// The target that would answer this kind for this sandbox.
///
/// Sandbox first, environment second. That order is the product promise: the
/// environment sets what is normal, a sandbox may narrow it, and a sandbox
/// cannot invent a model that is not allowed — [`validate_choice`] is what
/// enforces the last part at write time.
pub fn effective(
    global: &MediaSettings,
    sandbox: Option<&SandboxMediaSettings>,
    kind: MediaKind,
) -> Option<MediaTarget> {
    sandbox
        .and_then(|settings| settings.target(kind))
        .or_else(|| global.target(kind))
        .cloned()
}

/// Why a sandbox may not use this target, or `None` when it may.
///
/// The message is a sentence an operator can act on, and it names no internal
/// id: the console already has the provider for display.
pub fn refusal(global: &MediaSettings, kind: MediaKind, target: &MediaTarget, provider_exists: bool) -> Option<String> {
    if !global.enabled {
        return Some("Media rendering is disabled for this environment.".into());
    }
    if !provider_exists {
        return Some("The selected provider no longer exists in this environment.".into());
    }
    if !global.permits(target.provider_id, provider_exists) {
        return Some(format!(
            "This provider is not on the allow-list for {} rendering.",
            kind.as_str()
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(model: &str) -> MediaTarget {
        MediaTarget::new(Uuid::nil(), model).expect("valid target")
    }

    #[test]
    fn sandbox_overrides_environment_and_falls_back_per_kind() {
        let mut global = MediaSettings { enabled: true, ..Default::default() };
        global.image = Some(target("global-image"));
        global.video = Some(target("global-video"));
        let sandbox = SandboxMediaSettings { image: Some(target("sandbox-image")), ..Default::default() };

        assert_eq!(effective(&global, Some(&sandbox), MediaKind::Image).unwrap().model, "sandbox-image");
        assert_eq!(effective(&global, Some(&sandbox), MediaKind::Video).unwrap().model, "global-video");
        assert_eq!(effective(&global, Some(&sandbox), MediaKind::Music), None);
    }

    #[test]
    fn an_empty_allow_list_permits_every_existing_provider() {
        let global = MediaSettings { enabled: true, ..Default::default() };
        assert!(global.permits(Uuid::nil(), true));
        assert!(!global.permits(Uuid::nil(), false));
    }

    #[test]
    fn a_non_empty_allow_list_is_a_real_restriction() {
        let allowed = Uuid::nil();
        let global = MediaSettings { enabled: true, allowed_provider_ids: vec![allowed], ..Default::default() };
        assert!(global.permits(allowed, true));
        assert!(!global.permits(Uuid::max(), true));
        assert!(refusal(&global, MediaKind::Image, &target("m"), true).is_none());
        assert!(refusal(&global, MediaKind::Image, &MediaTarget { provider_id: Uuid::max(), model: "m".into() }, true).is_some());
    }

    #[test]
    fn a_disabled_environment_refuses_everything() {
        let global = MediaSettings { enabled: false, ..Default::default() };
        assert!(refusal(&global, MediaKind::Video, &target("m"), true).is_some());
    }

    #[test]
    fn target_validation_rejects_empty_and_overlong_names() {
        assert!(MediaTarget::new(Uuid::nil(), "  ").is_err());
        assert!(MediaTarget::new(Uuid::nil(), &"x".repeat(201)).is_err());
        assert_eq!(MediaTarget::new(Uuid::nil(), " flux-1.1 ").unwrap().model, "flux-1.1");
    }

    #[test]
    fn tool_names_match_the_three_kinds() {
        assert_eq!(MediaKind::Image.tool_name(), "render_image");
        assert_eq!(MediaKind::Video.tool_name(), "render_video");
        assert_eq!(MediaKind::Music.tool_name(), "render_music");
    }
}
