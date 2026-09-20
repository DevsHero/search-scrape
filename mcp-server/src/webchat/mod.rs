//! M6 webchat family: WB-02 C06 readiness, WB-03 declared transports,
//! WB-04 text-tool bridge envelopes, WB-05 parameterized recipes.
//!
//! Everything here is protocol and state logic over an abstract page so it can
//! be driven by the real automation session OR focused tests. Raw browser
//! cookies, passwords, OTP, passkeys, and tokens NEVER enter these types.

pub mod owned;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

// ─────────────────────────────── C06 readiness ───────────────────────────────

pub const C06_READINESS_PROTOCOL_V1: &str = "c06_browser_session_readiness_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessState {
    NeedsUser,
    LoginRequired,
    UserDonePendingProbe,
    SessionReady,
    Unknown,
    Error,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct BrowserSessionReadiness {
    pub protocol_version: String,
    pub lease_id: String,
    pub profile_id: String,
    pub job_id: String,
    pub origin: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub state: ReadinessState,
    pub probe_status: String,
    pub probe_reason: String,
    pub browser_liveness: String,
    pub account_state: String,
    pub capability_state: String,
}

impl BrowserSessionReadiness {
    pub fn needs_user(
        lease_id: impl Into<String>,
        profile_id: impl Into<String>,
        job_id: impl Into<String>,
        origin: impl Into<String>,
    ) -> Self {
        Self {
            protocol_version: C06_READINESS_PROTOCOL_V1.into(),
            lease_id: lease_id.into(),
            profile_id: profile_id.into(),
            job_id: job_id.into(),
            origin: origin.into(),
            created_at_ms: now_ms(),
            expires_at_ms: now_ms() + 600_000,
            state: ReadinessState::NeedsUser,
            probe_status: "not_run".into(),
            probe_reason: "session constructed".into(),
            browser_liveness: "unknown".into(),
            account_state: "unknown".into(),
            capability_state: "unknown".into(),
        }
    }

    pub fn transition_login_required(&mut self) {
        self.state = ReadinessState::LoginRequired;
        self.probe_status = "observed".into();
        self.probe_reason = "login page observed".into();
    }

    /// FINISH & RETURN only marks the user as done-pending-probe. It is
    /// never, by itself, authentication.
    pub fn transition_user_done(&mut self) {
        self.state = ReadinessState::UserDonePendingProbe;
        self.probe_status = "pending".into();
        self.probe_reason = "user clicked finish; owned-host probe required".into();
    }

    /// The ONLY path to SessionReady: a real readiness probe against the
    /// owned origin returned truthfully positive.
    pub fn apply_probe(&mut self, probe: ProbeOutcome, browser_live: bool) {
        if now_ms() >= self.expires_at_ms || self.state == ReadinessState::Expired {
            self.mark_expired();
            return;
        }
        self.account_state = "unknown".into();
        self.capability_state = "unknown".into();
        if !browser_live {
            self.state = ReadinessState::Unknown;
            self.browser_liveness = "dead".into();
            self.probe_status = "unavailable".into();
            return;
        }
        self.browser_liveness = if browser_live { "live" } else { "dead" }.into();
        match probe {
            ProbeOutcome::Ready => {
                self.state = ReadinessState::SessionReady;
                self.probe_status = "ready".into();
                self.account_state = "test_session_cookie_observed".into();
                self.capability_state = "supported".into();
            }
            ProbeOutcome::LoginRequired => {
                self.state = ReadinessState::LoginRequired;
                self.probe_status = "login_required".into();
                self.account_state = "anonymous".into();
            }
            ProbeOutcome::Unprobeable(reason) => {
                self.state = ReadinessState::Unknown;
                self.probe_status = "unprobeable".into();
                self.probe_reason = reason;
            }
            ProbeOutcome::Failed(reason) => {
                self.state = ReadinessState::Error;
                self.probe_status = "failed".into();
                self.probe_reason = reason;
            }
        }
    }

    pub fn mark_expired(&mut self) {
        self.state = ReadinessState::Expired;
        self.probe_status = "expired".into();
    }

    pub fn is_ready(&self) -> bool {
        self.state == ReadinessState::SessionReady && now_ms() < self.expires_at_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    Ready,
    LoginRequired,
    Unprobeable(String),
    Failed(String),
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

// ─────────────────────────── WB-03 components ────────────────────────────────

pub const WB03_COMPONENTS_PROTOCOL_V1: &str = "wb03_webchat_components_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentRole {
    Composer,
    Submit,
    AssistantMessages,
    Generating,
    Error,
    Upload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ComponentLocator {
    pub role: ComponentRole,
    /// selector | semantic | owned-event descriptor kind
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WebchatComponents {
    pub protocol_version: String,
    pub origin: String,
    pub version: u32,
    pub locators: Vec<ComponentLocator>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptorError {
    MissingRole(ComponentRole),
    AmbiguousRole(ComponentRole),
    OriginMismatch { expected: String, observed: String },
}

impl WebchatComponents {
    pub fn validate(&self) -> Result<(), DescriptorError> {
        let mut seen: BTreeMap<&'static str, usize> = BTreeMap::new();
        for locator in &self.locators {
            *seen.entry(role_key(&locator.role)).or_default() += 1;
        }
        for role in [
            ComponentRole::Composer,
            ComponentRole::Submit,
            ComponentRole::AssistantMessages,
            ComponentRole::Generating,
            ComponentRole::Error,
        ] {
            match seen.get(role_key(&role)).copied() {
                None => return Err(DescriptorError::MissingRole(role)),
                Some(n) if n > 1 => return Err(DescriptorError::AmbiguousRole(role)),
                Some(_) => {}
            }
        }
        Ok(())
    }

    pub fn assert_origin(&self, origin: &str) -> Result<(), DescriptorError> {
        if self.origin == origin {
            Ok(())
        } else {
            Err(DescriptorError::OriginMismatch {
                expected: self.origin.clone(),
                observed: origin.into(),
            })
        }
    }

    pub fn locator(&self, role: ComponentRole) -> Option<&ComponentLocator> {
        self.locators
            .iter()
            .find(|l| role_key(&l.role) == role_key(&role))
    }

    pub fn digest(&self) -> String {
        format!(
            "sha256:{}",
            hex::encode(
                sha2::Sha256::digest(serde_json::to_vec(self).unwrap_or_default()).as_slice()
            )
        )
    }
}

fn role_key(role: &ComponentRole) -> &'static str {
    match role {
        ComponentRole::Composer => "composer",
        ComponentRole::Submit => "submit",
        ComponentRole::AssistantMessages => "assistant_messages",
        ComponentRole::Generating => "generating",
        ComponentRole::Error => "error",
        ComponentRole::Upload => "upload",
    }
}

// ─────────────────────────── WB-03 message model ─────────────────────────────

pub const WB03_MESSAGE_PROTOCOL_V1: &str = "wb03_webchat_message_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionStatus {
    Prepared,
    Submitted,
    SubmissionUnknown,
    Generating,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WebchatMessage {
    pub protocol_version: String,
    pub job_id: String,
    pub lease_id: String,
    pub conversation_id: String,
    pub turn_id: String,
    pub message_id: String,
    /// Only an assistant message may become the terminal answer.
    pub role: String,
    pub content: String,
    pub terminal: bool,
    pub truncated: bool,
    pub truncation_reason: Option<String>,
}

// ───────────────────────────── WB-05 recipes ─────────────────────────────────

pub const WB05_RECIPE_PROTOCOL_V1: &str = "wb05_webchat_recipe_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct RecipeStep {
    /// navigate | insert | submit | await_terminal
    pub action: String,
    pub locator_role: Option<ComponentRole>,
    pub parameter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WebchatRecipe {
    pub protocol_version: String,
    pub recipe_id: String,
    pub revision: u32,
    pub required_origin: String,
    pub component_descriptor_digest: String,
    pub steps: Vec<RecipeStep>,
    pub parameter_schema: Vec<String>,
    pub preconditions: Vec<String>,
    pub postconditions: Vec<String>,
    pub approval_points: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeError {
    SensitiveAction(String),
    UnknownParameter(String),
    OriginMismatch { expected: String, observed: String },
    AmbiguousSubmit,
    ParametersRequired,
    DescriptorDigestMismatch,
    Descriptor(DescriptorError),
}

const SENSITIVE_HINTS: [&str; 4] = ["password", "otp", "passkey", "cookie"];

impl WebchatRecipe {
    /// Reject any recorded step that smells like secret handling.
    pub fn sanitized(steps: &[RecipeStep]) -> Result<Vec<RecipeStep>, RecipeError> {
        for step in steps {
            if !matches!(
                step.action.as_str(),
                "navigate" | "insert" | "submit" | "await_terminal"
            ) {
                return Err(RecipeError::SensitiveAction("unsupported action".into()));
            }
            if step.action == "insert" && step.parameter.as_deref() != Some("${prompt}") {
                return Err(RecipeError::UnknownParameter(
                    "only prompt insertion is supported".into(),
                ));
            }
            let hay = format!("{} {:?}", step.action, step.parameter).to_ascii_lowercase();
            if let Some(hint) = SENSITIVE_HINTS.iter().find(|h| hay.contains(*h)) {
                return Err(RecipeError::SensitiveAction((*hint).to_string()));
            }
        }
        Ok(steps.to_vec())
    }

    pub fn bind(&self, parameters: &BTreeMap<String, String>) -> Result<Vec<String>, RecipeError> {
        let mut rendered = Vec::new();
        for step in &self.steps {
            match &step.parameter {
                Some(template) if template.starts_with("${") => {
                    let name = template.trim_start_matches("${").trim_end_matches('}');
                    let value = parameters
                        .get(name)
                        .ok_or_else(|| RecipeError::UnknownParameter(name.to_string()))?;
                    rendered.push(format!("{}={}", step.action, value));
                }
                Some(literal) => rendered.push(format!("{}={}", step.action, literal)),
                None => rendered.push(step.action.clone()),
            }
        }
        Ok(rendered)
    }

    pub fn validate_replay(
        &self,
        origin: &str,
        parameters: &BTreeMap<String, String>,
        components: &WebchatComponents,
    ) -> Result<(), RecipeError> {
        self.assert_origin(origin)?;
        Self::sanitized(&self.steps)?;
        if self.component_descriptor_digest != components.digest() {
            return Err(RecipeError::DescriptorDigestMismatch);
        }
        if !self.parameter_schema.is_empty() && parameters.is_empty() {
            return Err(RecipeError::ParametersRequired);
        }
        for name in &self.parameter_schema {
            if !parameters.contains_key(name) {
                return Err(RecipeError::UnknownParameter(name.clone()));
            }
        }
        components.validate().map_err(RecipeError::Descriptor)?;
        components
            .assert_origin(origin)
            .map_err(|_| RecipeError::OriginMismatch {
                expected: origin.into(),
                observed: components.origin.clone(),
            })?;
        Ok(())
    }

    fn assert_origin(&self, origin: &str) -> Result<(), RecipeError> {
        if self.required_origin == origin {
            Ok(())
        } else {
            Err(RecipeError::OriginMismatch {
                expected: self.required_origin.clone(),
                observed: origin.into(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locator(role: ComponentRole, value: &str) -> ComponentLocator {
        ComponentLocator {
            role,
            kind: "owned_event".into(),
            value: value.into(),
        }
    }

    fn components() -> WebchatComponents {
        WebchatComponents {
            protocol_version: WB03_COMPONENTS_PROTOCOL_V1.into(),
            origin: "http://127.0.0.1:8199".into(),
            version: 1,
            locators: vec![
                locator(ComponentRole::Composer, "composer"),
                locator(ComponentRole::Submit, "send"),
                locator(ComponentRole::AssistantMessages, "thread"),
                locator(ComponentRole::Generating, "generating"),
                locator(ComponentRole::Error, "error"),
            ],
        }
    }

    #[test]
    fn readiness_requires_a_real_probe_never_click_as_auth() {
        let mut r =
            BrowserSessionReadiness::needs_user("lease-1", "profile-1", "job-1", "http://fixture");
        assert_eq!(r.state, ReadinessState::NeedsUser);
        r.transition_login_required();
        r.transition_user_done();
        assert_eq!(r.state, ReadinessState::UserDonePendingProbe);
        assert!(!r.is_ready(), "finish&return is not authentication");
        r.apply_probe(ProbeOutcome::Unprobeable("element missing".into()), true);
        assert_eq!(r.state, ReadinessState::Unknown);
        r.apply_probe(ProbeOutcome::Ready, true);
        assert!(r.is_ready());
        r.mark_expired();
        assert_eq!(r.state, ReadinessState::Expired);
    }

    #[test]
    fn descriptors_stop_on_ambiguous_or_missing_roles() {
        assert_eq!(components().validate(), Ok(()));
        let mut dup = components();
        dup.locators
            .push(locator(ComponentRole::Submit, "alt-send"));
        assert_eq!(
            dup.validate(),
            Err(DescriptorError::AmbiguousRole(ComponentRole::Submit))
        );
        let mut missing = components();
        missing
            .locators
            .retain(|l| l.role != ComponentRole::Composer);
        assert_eq!(
            missing.validate(),
            Err(DescriptorError::MissingRole(ComponentRole::Composer))
        );
    }

    #[test]
    fn recipes_strip_secrets_replay_parameters_and_pin_origin() {
        descriptors_stop_on_ambiguous_or_missing_roles();
        assert!(matches!(
            WebchatRecipe::sanitized(&[RecipeStep {
                action: "insert".into(),
                locator_role: None,
                parameter: Some("password".into()),
            }]),
            Err(RecipeError::SensitiveAction(_) | RecipeError::UnknownParameter(_))
        ));
        let recipe = WebchatRecipe {
            protocol_version: WB05_RECIPE_PROTOCOL_V1.into(),
            recipe_id: "r1".into(),
            revision: 3,
            required_origin: "http://127.0.0.1:8199".into(),
            component_descriptor_digest: components().digest(),
            steps: vec![
                RecipeStep {
                    action: "navigate".into(),
                    locator_role: None,
                    parameter: None,
                },
                RecipeStep {
                    action: "insert".into(),
                    locator_role: Some(ComponentRole::Composer),
                    parameter: Some("${prompt}".into()),
                },
                RecipeStep {
                    action: "submit".into(),
                    locator_role: Some(ComponentRole::Submit),
                    parameter: None,
                },
                RecipeStep {
                    action: "await_terminal".into(),
                    locator_role: Some(ComponentRole::AssistantMessages),
                    parameter: None,
                },
            ],
            parameter_schema: vec!["prompt".into()],
            preconditions: vec!["c06.session_ready".into()],
            postconditions: vec!["wb03.completed".into()],
            approval_points: vec![],
        };
        let mut params = BTreeMap::new();
        params.insert("prompt".to_string(), "line one\nline two\tcode".to_string());
        let rendered = recipe.bind(&params).unwrap();
        assert_eq!(rendered[1], "insert=line one\nline two\tcode");
        recipe
            .validate_replay("http://127.0.0.1:8199", &params, &components())
            .unwrap();
        assert_eq!(
            recipe.validate_replay("http://127.0.0.1:9999", &params, &components()),
            Err(RecipeError::OriginMismatch {
                expected: "http://127.0.0.1:8199".into(),
                observed: "http://127.0.0.1:9999".into(),
            })
        );
        let empty = BTreeMap::new();
        assert!(matches!(
            recipe.validate_replay("http://127.0.0.1:8199", &empty, &components()),
            Err(RecipeError::ParametersRequired)
        ));
        let mut ambiguous = components();
        ambiguous.locators.push(locator(ComponentRole::Submit, "x"));
        assert_eq!(
            ambiguous.validate(),
            Err(DescriptorError::AmbiguousRole(ComponentRole::Submit))
        );
    }
}
