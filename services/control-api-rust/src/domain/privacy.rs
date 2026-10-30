//! Content privacy: what a detector found, what the policy does with it, and
//! what the caller may send on.
//!
//! The rule this module exists for: **chat content is inspected before it
//! leaves a sandbox, and each finding is recorded as a masked excerpt plus a
//! fingerprint — never as the value.** Everything here is pure, so the whole
//! action matrix (off, observe, review, strict) is testable without a database
//! and without a network.
//!
//! Two things are deliberately elsewhere. The **hash** that turns a value into
//! a token is a service concern (`privacy_service::fingerprint`), so this layer
//! holds no crypto and a caller cannot substitute one it did not choose. The
//! **scanning** that finds a value in a text is also a service concern, because
//! it works on bytes and has no invariant a type can carry. The domain receives
//! [`Finding`]s with a caller-supplied token and answers what happens to the
//! text.

use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// The default retention of an incident, in days.
///
/// Kept next to the type so the built-in default and the column default in
/// `0056_privacy_layer.sql` cannot drift.
pub const DEFAULT_RETENTION_DAYS: i32 = 30;

/// How many characters of a value the mask keeps at each end.
const MASK_EDGE: usize = 2;

/// The longest masked value the incident column accepts.
const MASK_LIMIT: usize = 300;

/// How hard the privacy layer works on a piece of content.
///
/// The four modes are a ladder, and each rung is a promise an operator can
/// explain: `off` does nothing, `observe` only writes evidence, `review` cleans
/// the content and lets it through, `strict` may refuse the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyMode {
    /// No inspection at all, and no evidence either.
    Off,
    /// Inspect and record, change nothing. Used while the detectors are tuned.
    Observe,
    /// Rewrite the content and let the rewritten version through. Never refuses.
    Review,
    /// Rewrite, and refuse when a blocking detector fired.
    Strict,
}

impl Default for PrivacyMode {
    /// The safe rung: a settings row that does not name a mode is strict.
    fn default() -> Self {
        Self::Strict
    }
}

impl PrivacyMode {
    /// The database and wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Observe => "observe",
            Self::Review => "review",
            Self::Strict => "strict",
        }
    }

    /// Read the database or wire spelling; unknown input is `None`, never a
    /// silent default.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "observe" => Some(Self::Observe),
            "review" => Some(Self::Review),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }
}

/// What may happen to one matched value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Refuse the request. Only `strict` mode enforces it.
    Block,
    /// Replace with a mask that keeps the shape (`ab****yz`).
    Redact,
    /// Replace with a labelled placeholder (`<API_KEYS_1a2b3c4d>`).
    Replace,
    /// Leave it alone; the finding is still recorded.
    Allow,
}

impl Action {
    /// The spelling used in a settings `actions` object and in the audit detail.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Redact => "redact",
            Self::Replace => "replace",
            Self::Allow => "allow",
        }
    }

    /// Read the settings spelling; unknown input is `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "block" => Some(Self::Block),
            "redact" => Some(Self::Redact),
            "replace" => Some(Self::Replace),
            "allow" => Some(Self::Allow),
            _ => None,
        }
    }

    /// The incident action this action produces.
    ///
    /// One mapping for the whole codebase, so "the policy said replace" and
    /// "the incident says replaced" cannot disagree.
    pub fn incident_action(self) -> IncidentAction {
        match self {
            Self::Block => IncidentAction::Blocked,
            Self::Redact => IncidentAction::Redacted,
            Self::Replace => IncidentAction::Replaced,
            Self::Allow => IncidentAction::Allowed,
        }
    }
}

/// What an incident row records as having happened.
///
/// Separate from [`Action`] because a policy asks for one thing and the mode may
/// do another: in `review` a configured `block` becomes `replaced`, and the
/// incident must say what happened, not what was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IncidentAction {
    Blocked,
    Redacted,
    Replaced,
    Allowed,
}

impl IncidentAction {
    /// The database and wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Redacted => "redacted",
            Self::Replaced => "replaced",
            Self::Allowed => "allowed",
        }
    }

    /// Read the database spelling; unknown input is `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "blocked" => Some(Self::Blocked),
            "redacted" => Some(Self::Redacted),
            "replaced" => Some(Self::Replaced),
            "allowed" => Some(Self::Allowed),
            _ => None,
        }
    }

    /// The severity an incident row carries.
    ///
    /// A block is an event an operator must look at, a rewrite is a warning, and
    /// an observed pass-through is information. Deriving it here keeps the
    /// severity out of the caller's hands, so two call sites cannot disagree.
    pub fn severity(self) -> &'static str {
        match self {
            Self::Blocked => "critical",
            Self::Redacted | Self::Replaced => "warning",
            Self::Allowed => "info",
        }
    }
}

/// What kind of sensitive content a detector looks for.
///
/// The set is closed on purpose: the database CHECK, the `detectors` array and
/// the scraper in `privacy_service` all name the same ten things, and a new one
/// is a code change in all three places rather than a free-form string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Detector {
    /// A password, token or private key in an assignment or a PEM block.
    Credentials,
    /// A provider key: `sk-…`, `ghp_…`, `AKIA…`, a JWT.
    ApiKeys,
    EmailAddresses,
    /// Two capitalised words in a row. A heuristic, and the weakest detector.
    PersonNames,
    IpAddresses,
    Hostnames,
    PhoneNumbers,
    Iban,
    CreditCards,
    /// An amount with a currency: `$1,240.00`, `900 EUR`.
    FinancialData,
}

impl Detector {
    /// Every detector, in the order a report lists them.
    pub const ALL: [Detector; 10] = [
        Detector::Credentials,
        Detector::ApiKeys,
        Detector::EmailAddresses,
        Detector::PersonNames,
        Detector::IpAddresses,
        Detector::Hostnames,
        Detector::PhoneNumbers,
        Detector::Iban,
        Detector::CreditCards,
        Detector::FinancialData,
    ];

    /// The database and wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Credentials => "credentials",
            Self::ApiKeys => "api_keys",
            Self::EmailAddresses => "email_addresses",
            Self::PersonNames => "person_names",
            Self::IpAddresses => "ip_addresses",
            Self::Hostnames => "hostnames",
            Self::PhoneNumbers => "phone_numbers",
            Self::Iban => "iban",
            Self::CreditCards => "credit_cards",
            Self::FinancialData => "financial_data",
        }
    }

    /// Read the database or wire spelling; unknown input is `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "credentials" => Some(Self::Credentials),
            "api_keys" => Some(Self::ApiKeys),
            "email_addresses" => Some(Self::EmailAddresses),
            "person_names" => Some(Self::PersonNames),
            "ip_addresses" => Some(Self::IpAddresses),
            "hostnames" => Some(Self::Hostnames),
            "phone_numbers" => Some(Self::PhoneNumbers),
            "iban" => Some(Self::Iban),
            "credit_cards" => Some(Self::CreditCards),
            "financial_data" => Some(Self::FinancialData),
            _ => None,
        }
    }

    /// The label a placeholder carries, so a reader of the rewritten text knows
    /// what was removed without seeing it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Credentials => "CREDENTIALS",
            Self::ApiKeys => "API_KEYS",
            Self::EmailAddresses => "EMAIL_ADDRESSES",
            Self::PersonNames => "PERSON_NAMES",
            Self::IpAddresses => "IP_ADDRESSES",
            Self::Hostnames => "HOSTNAMES",
            Self::PhoneNumbers => "PHONE_NUMBERS",
            Self::Iban => "IBAN",
            Self::CreditCards => "CREDIT_CARDS",
            Self::FinancialData => "FINANCIAL_DATA",
        }
    }

    /// What this detector does when the policy does not say otherwise.
    ///
    /// A credential, a key, an account number and a card number are secrets and
    /// are blocked by default. Everything else is personal or competitive data
    /// whose loss spoils the answer: replacing it keeps the conversation usable,
    /// which is what makes a strict default deployable at all.
    pub fn default_action(self) -> Action {
        match self {
            Self::Credentials | Self::ApiKeys | Self::Iban | Self::CreditCards => Action::Block,
            Self::EmailAddresses
            | Self::PersonNames
            | Self::IpAddresses
            | Self::Hostnames
            | Self::PhoneNumbers
            | Self::FinancialData => Action::Replace,
        }
    }
}

/// One matched value and the detector that matched it.
///
/// The value is held here only for the duration of one inspection: it is what
/// the substitution in [`apply`] needs. It is never stored, never logged, and
/// the store only ever receives [`Finding::masked`] and a fingerprint — which is
/// why `Debug` is implemented by hand below.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// What matched.
    pub detector: Detector,
    /// The matched text. Held for the substitution only; never stored or logged.
    pub value: String,
}

impl Finding {
    /// A finding for one matched span.
    pub fn new(detector: Detector, value: impl Into<String>) -> Self {
        Self {
            detector,
            value: value.into(),
        }
    }

    /// The label the rewritten text carries: `<DETECTOR_xxxxxxxx>`.
    ///
    /// The eight characters come from the caller's token — the SHA-256 of the
    /// value, computed in the service — so the same value yields the same
    /// placeholder in every message and a reader can tell two occurrences apart
    /// without either being recoverable. A token shorter than eight characters
    /// (or none at all) is padded rather than panicking: this runs inside a
    /// request, where a panic is a 500 with no context.
    pub fn placeholder(&self, token: &str) -> String {
        format!("<{}_{}>", self.detector.label(), short_token(token))
    }

    /// A masked excerpt: first two and last two characters, stars between.
    ///
    /// Never the value: for a value of four characters or fewer the result is
    /// all stars, because keeping the edges of a short secret is keeping most of
    /// it. The result is capped at [`MASK_LIMIT`] so a pathological match cannot
    /// become a stored copy of the content through the incident row.
    pub fn masked(&self) -> String {
        let characters: Vec<char> = self.value.chars().collect();
        let length = characters.len();
        if length == 0 {
            return String::new();
        }
        if length <= MASK_EDGE * 2 {
            return "*".repeat(length);
        }
        // The middle is capped, so the result never exceeds the incident column
        // even when a detector matched a very long block.
        let stars = (length - MASK_EDGE * 2).min(MASK_LIMIT - MASK_EDGE * 2);
        let mut masked = String::with_capacity(MASK_EDGE * 2 + stars);
        masked.extend(&characters[..MASK_EDGE]);
        masked.push_str(&"*".repeat(stars));
        masked.extend(&characters[length - MASK_EDGE..]);
        masked
    }
}

impl fmt::Debug for Finding {
    /// Prints the detector and the mask, never the value.
    ///
    /// A derived `Debug` would put the secret into a `tracing::debug!` line, an
    /// error message, or a test failure — three places a value is not allowed
    /// to appear.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Finding")
            .field("detector", &self.detector)
            .field("masked", &self.masked())
            .finish()
    }
}

/// The policy for one scope: an environment default, or one sandbox's override.
///
/// `sandbox_id: None` is the environment default row (see
/// `0056_privacy_layer.sql`); a `Some` is the override that wins for that
/// sandbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PrivacySettings {
    /// `None` for the environment default row, `Some` for a sandbox override.
    pub sandbox_id: Option<Uuid>,
    /// How hard the policy works on the content.
    pub mode: PrivacyMode,
    /// Enabled detectors. Empty means every detector: a save that omitted the
    /// field must not switch detection off, which only `mode = off` does.
    pub detectors: Vec<Detector>,
    /// Per-detector overrides, keyed by detector name in JSON.
    #[schema(value_type = Object)]
    pub actions: BTreeMap<Detector, Action>,
    /// Whether a blocking action is enforced rather than downgraded to a rewrite.
    pub block_cloud_egress: bool,
    /// How long an incident is kept, in days.
    pub retention_days: i32,
    /// Who saved the row last; `None` for the built-in default.
    pub updated_by: Option<Uuid>,
    /// `None` for the built-in default, which has no row.
    pub updated_at: Option<DateTime<Utc>>,
}

impl PrivacySettings {
    /// The built-in policy: strict, every detector, nothing overridden.
    ///
    /// This is what an environment that has never saved anything gets. It is
    /// strict on purpose — the safe state is the one nobody had to choose.
    pub fn strict_default() -> Self {
        Self {
            sandbox_id: None,
            mode: PrivacyMode::Strict,
            detectors: Detector::ALL.to_vec(),
            actions: BTreeMap::new(),
            block_cloud_egress: true,
            retention_days: DEFAULT_RETENTION_DAYS,
            updated_by: None,
            updated_at: None,
        }
    }

    /// The detectors to run: the configured set, or all of them when the set is
    /// empty.
    pub fn enabled_detectors(&self) -> Vec<Detector> {
        if self.detectors.is_empty() {
            Detector::ALL.to_vec()
        } else {
            self.detectors.clone()
        }
    }

    /// The action for one detector: the override, else the detector's default.
    pub fn action_for(&self, detector: Detector) -> Action {
        self.actions
            .get(&detector)
            .copied()
            .unwrap_or_else(|| detector.default_action())
    }
}

/// What a caller may change about a scope.
///
/// Separate from [`PrivacySettings`] so the wire shape cannot carry `sandbox_id`
/// or `updated_by`: the scope comes from the path and the actor from the
/// principal, and a body that could name them is a body that could write another
/// tenant's row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PrivacyUpdate {
    /// Defaults to `strict` when the body omits it.
    #[serde(default)]
    pub mode: PrivacyMode,
    /// Defaults to the empty set, which means every detector.
    #[serde(default)]
    pub detectors: Vec<Detector>,
    /// Per-detector overrides; an absent key keeps the detector's default action.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub actions: BTreeMap<Detector, Action>,
    /// Defaults to `true`: a blocking action refuses, it does not just rewrite.
    #[serde(default = "default_block_cloud_egress")]
    pub block_cloud_egress: bool,
    /// Defaults to 30 days.
    #[serde(default = "default_retention_days")]
    pub retention_days: i32,
}

impl Default for PrivacyUpdate {
    fn default() -> Self {
        let settings = PrivacySettings::strict_default();
        Self {
            mode: settings.mode,
            detectors: settings.detectors,
            actions: settings.actions,
            block_cloud_egress: settings.block_cloud_egress,
            retention_days: settings.retention_days,
        }
    }
}

fn default_block_cloud_egress() -> bool {
    true
}

fn default_retention_days() -> i32 {
    DEFAULT_RETENTION_DAYS
}

impl PrivacyUpdate {
    /// Whether this update can be stored.
    ///
    /// The only rule the database does not already carry as a sentence a user
    /// can act on is the retention window; it is repeated here so a refusal is a
    /// 422 with a reason rather than a constraint name.
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=3650).contains(&self.retention_days) {
            return Err("retention_days must be between 1 and 3650.".into());
        }
        Ok(())
    }

    /// The storable policy for one scope.
    ///
    /// Detectors are de-duplicated and ordered, so saving the same policy twice
    /// produces the same row and the same audit detail.
    pub fn into_settings(self, sandbox_id: Option<Uuid>) -> PrivacySettings {
        let mut detectors: Vec<Detector> = Vec::new();
        for detector in self.detectors {
            if !detectors.contains(&detector) {
                detectors.push(detector);
            }
        }
        detectors.sort();
        PrivacySettings {
            sandbox_id,
            mode: self.mode,
            detectors,
            actions: self.actions,
            block_cloud_egress: self.block_cloud_egress,
            retention_days: self.retention_days,
            updated_by: None,
            updated_at: None,
        }
    }
}

/// A finding together with the token the caller derived from its value.
///
/// The token is the SHA-256 of the value, computed in the service. Passing it in
/// is what keeps this layer free of crypto while still producing a placeholder
/// that is stable across messages.
#[derive(Debug, Clone)]
pub struct Marked {
    pub finding: Finding,
    pub token: String,
}

impl Marked {
    /// A finding and its token.
    pub fn new(finding: Finding, token: impl Into<String>) -> Self {
        Self {
            finding,
            token: token.into(),
        }
    }
}

/// What the mode makes of one configured action.
///
/// This is the whole enforcement rule in four lines, and every mode difference
/// in the product is one of them:
///
/// * `off` and `observe` do nothing to the content.
/// * `review` turns a `block` into a `replace` — it rewrites and lets the result
///   through, because refusing is not a reviewer's decision to make.
/// * `strict` keeps a `block`, except when `block_cloud_egress` is false; then an
///   environment that wants the evidence without the outage gets the rewrite.
pub fn effective_action(mode: PrivacyMode, configured: Action, block_cloud_egress: bool) -> Action {
    match mode {
        PrivacyMode::Off | PrivacyMode::Observe => Action::Allow,
        PrivacyMode::Review => match configured {
            Action::Block => Action::Replace,
            other => other,
        },
        PrivacyMode::Strict => match configured {
            Action::Block if !block_cloud_egress => Action::Replace,
            other => other,
        },
    }
}

/// What one finding produced, in the words the incident row uses.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    /// The detector that fired.
    pub detector: Detector,
    /// The action the policy configured.
    pub action: Action,
    /// The action the mode actually applies.
    pub effective: Action,
    /// The masked excerpt, safe to store and to show.
    pub masked_value: String,
    /// The fingerprint the placeholder is built from.
    pub token: String,
}

impl Decision {
    /// What the incident row records for this finding.
    pub fn incident_action(&self) -> IncidentAction {
        self.effective.incident_action()
    }

    /// How the rewritten content names what was removed.
    pub fn replacement(&self) -> String {
        if self.effective == Action::Redact {
            self.masked_value.clone()
        } else {
            format!(
                "<{}_{}>",
                self.detector.label(),
                short_token(&self.token)
            )
        }
    }
}

/// The first eight characters of a fingerprint, padded when it is short.
fn short_token(token: &str) -> String {
    let mut short: String = token
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(8)
        .collect();
    while short.len() < 8 {
        short.push('0');
    }
    short
}

/// The result of applying a policy to one piece of content.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The aggregated answer: the strongest thing that happened.
    pub action: Action,
    /// False only when a finding is blocked.
    pub permitted: bool,
    /// The content with every non-allowed finding rewritten.
    ///
    /// For a blocked request this is the redacted text: the caller refuses the
    /// request, but a log or a reviewer may still see *that* something was
    /// found without seeing it.
    pub transformed: String,
    /// One decision per finding, in the order the findings arrived.
    pub decisions: Vec<Decision>,
}

impl Outcome {
    /// The wire spelling of [`Outcome::action`]: `allowed`, `replaced`, `blocked`.
    pub fn action_str(&self) -> &'static str {
        match self.action {
            Action::Allow => "allowed",
            Action::Block => "blocked",
            Action::Redact | Action::Replace => "replaced",
        }
    }
}

/// Apply a policy to the findings of one piece of content.
///
/// Pure substitution: no scanning, no hashing, no database. The two properties
/// that matter are that the same finding and the same token always produce the
/// same text, and that a value never survives in `transformed` when its action
/// is anything but `allow`.
pub fn apply(settings: &PrivacySettings, marked: &[Marked], text: &str) -> Outcome {
    if settings.mode == PrivacyMode::Off || marked.is_empty() {
        return Outcome {
            action: Action::Allow,
            permitted: true,
            transformed: text.to_string(),
            decisions: Vec::new(),
        };
    }

    let mut transformed = text.to_string();
    let mut decisions = Vec::with_capacity(marked.len());
    for entry in marked {
        let configured = settings.action_for(entry.finding.detector);
        let effective = effective_action(settings.mode, configured, settings.block_cloud_egress);
        let decision = Decision {
            detector: entry.finding.detector,
            action: configured,
            effective,
            masked_value: entry.finding.masked(),
            token: entry.token.clone(),
        };
        // A block still rewrites what it refuses: the caller discards the text,
        // and the redacted copy is what a log may keep.
        if effective != Action::Allow && !entry.finding.value.is_empty() {
            let replacement = decision.replacement();
            transformed = transformed.replace(entry.finding.value.as_str(), replacement.as_str());
        }
        decisions.push(decision);
    }

    let mut action = Action::Allow;
    for decision in &decisions {
        match decision.effective {
            Action::Block => {
                action = Action::Block;
                break;
            }
            Action::Redact | Action::Replace => action = Action::Replace,
            Action::Allow => {}
        }
    }

    Outcome {
        action,
        permitted: action != Action::Block,
        transformed,
        decisions,
    }
}

/// One incident row: what a detector found, what happened, and how to recognise
/// a repeat without being able to read the value.
///
/// `masked_value` and `value_fingerprint` are the only two traces of the match
/// that exist after the request; the value itself is gone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PrivacyIncident {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub sandbox_id: Uuid,
    /// What fired.
    pub detector: Detector,
    /// What the policy did, not what it asked for.
    pub action: IncidentAction,
    /// `info`, `warning` or `critical`, derived from the action.
    pub severity: String,
    /// First and last characters of the value, stars between.
    pub masked_value: Option<String>,
    /// SHA-256 of the value, lowercase hex: recognises a repeat without
    /// revealing the value.
    pub value_fingerprint: Option<String>,
    /// Where the content was going, when the caller named a destination.
    pub destination: Option<String>,
    /// Whether the content counted as a delivery to a cloud destination.
    pub cloud_delivery: bool,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: PrivacyMode) -> PrivacySettings {
        PrivacySettings {
            mode,
            ..PrivacySettings::strict_default()
        }
    }

    fn marked(detector: Detector, value: &str, token: &str) -> Marked {
        Marked::new(Finding::new(detector, value), token)
    }

    #[test]
    fn masking_never_returns_the_value() {
        for value in [
            "a",
            "ab",
            "abc",
            "abcd",
            "hunter2",
            "sk-live-0123456789abcdef",
            &"x".repeat(400),
        ] {
            let masked = Finding::new(Detector::ApiKeys, value).masked();
            assert!(!masked.is_empty(), "{value} must not mask to nothing");
            assert!(!masked.contains(value), "{masked} still contains the value");
            assert!(masked.chars().count() <= MASK_LIMIT);
            let characters: Vec<char> = value.chars().collect();
            if characters.len() > 4 {
                assert!(masked.starts_with(&characters[..2].iter().collect::<String>()));
                assert!(masked.ends_with(&characters[characters.len() - 2..].iter().collect::<String>()));
            }
        }
    }

    #[test]
    fn a_placeholder_carries_the_first_eight_characters_of_the_token() {
        let finding = Finding::new(Detector::ApiKeys, "sk-live-0123456789abcdef");
        assert_eq!(
            finding.placeholder("1a2b3c4d5e6f708192a3b4c5d6e7f809"),
            "<API_KEYS_1a2b3c4d>"
        );
        // A token that is too short is padded instead of panicking.
        assert_eq!(finding.placeholder("ab"), "<API_KEYS_ab000000>");
        assert_eq!(finding.placeholder(""), "<API_KEYS_00000000>");
    }

    #[test]
    fn replacement_is_deterministic_for_the_same_token() {
        let policy = settings(PrivacyMode::Strict);
        let token = "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";
        let findings = vec![marked(Detector::EmailAddresses, "ada@example.com", token)];
        let text = "mail ada@example.com please";

        let first = apply(&policy, &findings, text);
        let second = apply(&policy, &findings, text);

        assert_eq!(first.transformed, second.transformed);
        assert_eq!(
            first.transformed,
            "mail <EMAIL_ADDRESSES_0f1e2d3c> please".to_string()
        );
        assert_eq!(first.action, Action::Replace);
        assert!(first.permitted);
        assert!(!first.transformed.contains("ada@example.com"));
    }

    #[test]
    fn the_same_value_yields_the_same_placeholder_in_every_message() {
        let policy = settings(PrivacyMode::Strict);
        let token = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let one = apply(&policy, &[marked(Detector::Iban, "DE89370400440532013000", token)], "a DE89370400440532013000");
        let two = apply(&policy, &[marked(Detector::Iban, "DE89370400440532013000", token)], "b DE89370400440532013000");
        assert_eq!(one.transformed, "a <IBAN_aaaaaaaa>");
        assert_eq!(two.transformed, "b <IBAN_aaaaaaaa>");
    }

    #[test]
    fn a_block_is_blocking_only_in_strict_mode() {
        let findings = vec![marked(Detector::ApiKeys, "sk-live-0123456789abcdef", "0123456789abcdef")];
        let text = "key sk-live-0123456789abcdef";

        let strict = apply(&settings(PrivacyMode::Strict), &findings, text);
        assert_eq!(strict.action, Action::Block);
        assert!(!strict.permitted);
        assert_eq!(strict.action_str(), "blocked");
        assert!(!strict.transformed.contains("sk-live-0123456789abcdef"));

        let review = apply(&settings(PrivacyMode::Review), &findings, text);
        assert_eq!(review.action, Action::Replace);
        assert!(review.permitted);
        assert_eq!(review.action_str(), "replaced");
        assert_eq!(review.decisions[0].incident_action(), IncidentAction::Replaced);

        let observe = apply(&settings(PrivacyMode::Observe), &findings, text);
        assert_eq!(observe.action, Action::Allow);
        assert!(observe.permitted);
        assert_eq!(observe.transformed, text);
        assert_eq!(observe.decisions[0].incident_action(), IncidentAction::Allowed);
    }

    #[test]
    fn off_inspects_nothing_at_all() {
        let findings = vec![marked(Detector::ApiKeys, "sk-live-0123456789abcdef", "0123456789abcdef")];
        let outcome = apply(&settings(PrivacyMode::Off), &findings, "key sk-live-0123456789abcdef");
        assert_eq!(outcome.action, Action::Allow);
        assert!(outcome.permitted);
        assert!(outcome.decisions.is_empty());
        assert_eq!(outcome.transformed, "key sk-live-0123456789abcdef");
    }

    #[test]
    fn strict_without_the_egress_gate_rewrites_instead_of_refusing() {
        let policy = PrivacySettings {
            block_cloud_egress: false,
            ..settings(PrivacyMode::Strict)
        };
        let findings = vec![marked(Detector::CreditCards, "4111111111111111", "4111111111111111")];
        let outcome = apply(&policy, &findings, "card 4111111111111111");
        assert_eq!(outcome.action, Action::Replace);
        assert!(outcome.permitted);
        assert!(!outcome.transformed.contains("4111111111111111"));
    }

    #[test]
    fn a_configured_redact_keeps_the_shape_and_still_hides_the_value() {
        let mut policy = settings(PrivacyMode::Strict);
        policy
            .actions
            .insert(Detector::EmailAddresses, Action::Redact);
        let findings = vec![marked(Detector::EmailAddresses, "ada@example.com", "0f1e2d3c")];
        let outcome = apply(&policy, &findings, "mail ada@example.com");
        assert_eq!(outcome.transformed, "mail ad***********om");
        assert!(!outcome.transformed.contains("ada@example.com"));
    }

    #[test]
    fn a_configured_allow_records_the_finding_and_lets_it_through() {
        let mut policy = settings(PrivacyMode::Strict);
        policy.actions.insert(Detector::PersonNames, Action::Allow);
        let findings = vec![marked(Detector::PersonNames, "Ada Lovelace", "abc")];
        let outcome = apply(&policy, &findings, "hi Ada Lovelace");
        assert_eq!(outcome.action, Action::Allow);
        assert!(outcome.permitted);
        assert_eq!(outcome.transformed, "hi Ada Lovelace");
        assert_eq!(outcome.decisions.len(), 1);
        assert_eq!(outcome.decisions[0].incident_action(), IncidentAction::Allowed);
    }

    #[test]
    fn every_strongest_decision_wins_the_aggregate() {
        let findings = vec![
            marked(Detector::EmailAddresses, "ada@example.com", "aaaaaaaa"),
            marked(Detector::ApiKeys, "sk-live-0123456789abcdef", "bbbbbbbb"),
        ];
        let outcome = apply(&settings(PrivacyMode::Strict), &findings, "x");
        assert_eq!(outcome.action, Action::Block);
        assert_eq!(outcome.decisions.len(), 2);
    }

    #[test]
    fn detectors_default_to_blocking_only_the_four_secret_kinds() {
        for detector in Detector::ALL {
            let expected = match detector {
                Detector::Credentials
                | Detector::ApiKeys
                | Detector::Iban
                | Detector::CreditCards => Action::Block,
                _ => Action::Replace,
            };
            assert_eq!(detector.default_action(), expected, "{}", detector.as_str());
        }
    }

    #[test]
    fn an_empty_detector_list_means_every_detector() {
        let policy = PrivacySettings {
            detectors: Vec::new(),
            ..PrivacySettings::strict_default()
        };
        assert_eq!(policy.enabled_detectors().len(), Detector::ALL.len());
    }

    #[test]
    fn the_built_in_default_is_strict_with_every_detector() {
        let policy = PrivacySettings::strict_default();
        assert_eq!(policy.mode, PrivacyMode::Strict);
        assert!(policy.block_cloud_egress);
        assert_eq!(policy.retention_days, DEFAULT_RETENTION_DAYS);
        assert_eq!(policy.enabled_detectors().len(), 10);
        assert!(policy.actions.is_empty());
        assert_eq!(policy.sandbox_id, None);
    }

    #[test]
    fn an_update_de_duplicates_and_orders_its_detectors() {
        let update = PrivacyUpdate {
            detectors: vec![
                Detector::Hostnames,
                Detector::ApiKeys,
                Detector::Hostnames,
            ],
            ..PrivacyUpdate::default()
        };
        let settings = update.into_settings(Some(Uuid::nil()));
        assert_eq!(settings.detectors, vec![Detector::ApiKeys, Detector::Hostnames]);
        assert_eq!(settings.sandbox_id, Some(Uuid::nil()));
        assert_eq!(settings.mode, PrivacyMode::Strict);
    }

    #[test]
    fn a_retention_outside_the_window_is_refused_with_a_sentence() {
        let update = PrivacyUpdate {
            retention_days: 0,
            ..PrivacyUpdate::default()
        };
        let message = update.validate().unwrap_err();
        assert!(message.contains("retention_days"));
        assert!(PrivacyUpdate::default().validate().is_ok());
    }

    #[test]
    fn the_action_map_round_trips_through_json() {
        let mut policy = settings(PrivacyMode::Review);
        policy
            .actions
            .insert(Detector::EmailAddresses, Action::Redact);
        let json = serde_json::to_value(&policy).unwrap();
        assert_eq!(json["actions"]["email_addresses"], serde_json::json!("redact"));
        let back: PrivacySettings = serde_json::from_value(json).unwrap();
        assert_eq!(back.actions.get(&Detector::EmailAddresses), Some(&Action::Redact));
        assert_eq!(back, policy);
    }

    #[test]
    fn debug_never_prints_the_value() {
        let finding = Finding::new(Detector::Credentials, "password=hunter2");
        let printed = format!("{finding:?}");
        assert!(!printed.contains("hunter2"), "{printed}");
        assert!(printed.contains("CREDENTIALS") || printed.contains("Credentials"));
    }

    #[test]
    fn every_mode_and_action_has_a_stable_spelling() {
        for mode in [
            PrivacyMode::Off,
            PrivacyMode::Observe,
            PrivacyMode::Review,
            PrivacyMode::Strict,
        ] {
            assert_eq!(PrivacyMode::parse(mode.as_str()), Some(mode));
        }
        for action in [Action::Block, Action::Redact, Action::Replace, Action::Allow] {
            assert_eq!(Action::parse(action.as_str()), Some(action));
        }
        for incident in [
            IncidentAction::Blocked,
            IncidentAction::Redacted,
            IncidentAction::Replaced,
            IncidentAction::Allowed,
        ] {
            assert_eq!(IncidentAction::parse(incident.as_str()), Some(incident));
        }
        assert_eq!(PrivacyMode::parse("paranoid"), None);
        assert_eq!(Action::parse(""), None);
        assert_eq!(IncidentAction::parse("ignored"), None);
    }
}
