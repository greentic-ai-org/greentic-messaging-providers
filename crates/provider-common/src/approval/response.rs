//! Building a `greentic.approval.response.v1` body.

use serde_json::{Map, Value, json};

use super::token::DecisionToken;

/// Cap the designer applies on storage; trim here so nothing is silently cut.
const NOTE_MAX_CHARS: usize = 2000;

/// What was decided. `timeout` is a machine outcome, not a person's vote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Denied,
    Timeout,
}

impl Decision {
    pub fn as_wire(self) -> &'static str {
        match self {
            Decision::Approved => "approved",
            Decision::Denied => "denied",
            Decision::Timeout => "timeout",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "approved" => Some(Decision::Approved),
            "denied" => Some(Decision::Denied),
            "timeout" => Some(Decision::Timeout),
            _ => None,
        }
    }

    /// Votes are counted per person; a timeout names nobody.
    pub fn is_vote(self) -> bool {
        !matches!(self, Decision::Timeout)
    }
}

/// The human's answer, ready to publish on [`super::RESPONSE_SUBJECT`].
#[derive(Clone, Debug)]
pub struct ApprovalResponse {
    /// Echoed for symmetry and for other consumers; the designer routes on the
    /// correlation id header instead.
    pub target: String,
    pub decision: Decision,
    /// A claim, not an authenticated identity. A policy-governed gate refuses
    /// a vote that names nobody.
    pub resolved_by: Option<String>,
    /// Absent only for a gate that was never issued one.
    pub decision_token: Option<DecisionToken>,
    pub note: Option<String>,
}

impl ApprovalResponse {
    pub fn new(target: impl Into<String>, decision: Decision) -> Self {
        Self {
            target: target.into(),
            decision,
            resolved_by: None,
            decision_token: None,
            note: None,
        }
    }

    pub fn with_resolved_by(mut self, resolved_by: Option<String>) -> Self {
        self.resolved_by = resolved_by
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self
    }

    pub fn with_token(mut self, token: Option<DecisionToken>) -> Self {
        self.decision_token = token;
        self
    }

    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note
            .map(|value| {
                value
                    .trim()
                    .chars()
                    .take(NOTE_MAX_CHARS)
                    .collect::<String>()
            })
            .filter(|value| !value.is_empty());
        self
    }

    /// Without a token the designer rejects the response and the gate stays
    /// pending — except for a gate that never had one.
    pub fn carries_token(&self) -> bool {
        self.decision_token.is_some()
    }

    pub fn to_value(&self) -> Value {
        let mut output = Map::new();
        output.insert("decision".into(), json!(self.decision.as_wire()));
        output.insert(
            "resolved_by".into(),
            self.resolved_by
                .as_ref()
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        );
        if let Some(token) = &self.decision_token {
            output.insert("decision_token".into(), json!(token.expose()));
        }
        output.insert(
            "note".into(),
            self.note
                .as_ref()
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        );

        json!({
            "target": self.target,
            "operation": "response",
            "output": Value::Object(output),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFORMANCE_RESPONSE: &str =
        include_str!("../../../../tests/fixtures/approval_rail/response_v2.json");

    #[test]
    fn matches_the_conformance_response_fixture() {
        let expected: Value = serde_json::from_str(CONFORMANCE_RESPONSE).expect("fixture");
        let built = ApprovalResponse::new("default::run=RUN-1::node=gate", Decision::Approved)
            .with_resolved_by(Some("boss@acme.test".to_string()))
            .with_token(DecisionToken::new("EXAMPLE-TOKEN-NOT-A-REAL-SECRET"))
            .to_value();

        assert_eq!(built, expected);
    }

    #[test]
    fn a_gate_that_never_had_a_token_omits_the_key() {
        let built = ApprovalResponse::new("t::run=R::node=n", Decision::Denied)
            .with_resolved_by(Some("dev@acme.test".to_string()))
            .to_value();

        assert!(built["output"].get("decision_token").is_none());
        assert_eq!(built["output"]["decision"], "denied");
        assert_eq!(built["output"]["note"], Value::Null);
    }

    #[test]
    fn notes_are_trimmed_dropped_when_empty_and_capped() {
        let empty = ApprovalResponse::new("t", Decision::Approved).with_note(Some("   ".into()));
        assert_eq!(empty.to_value()["output"]["note"], Value::Null);

        let long = ApprovalResponse::new("t", Decision::Approved)
            .with_note(Some("x".repeat(NOTE_MAX_CHARS + 500)));
        let note = long.to_value()["output"]["note"]
            .as_str()
            .expect("note")
            .to_string();
        assert_eq!(note.chars().count(), NOTE_MAX_CHARS);
    }

    #[test]
    fn timeout_is_not_a_vote() {
        assert!(!Decision::Timeout.is_vote());
        assert!(Decision::Approved.is_vote());
        assert_eq!(Decision::from_wire("approved"), Some(Decision::Approved));
        assert_eq!(Decision::from_wire("cancelled"), None);
    }

    #[test]
    fn a_blank_resolved_by_is_no_claimed_identity() {
        let built = ApprovalResponse::new("t", Decision::Approved)
            .with_resolved_by(Some("  ".to_string()))
            .to_value();
        assert_eq!(built["output"]["resolved_by"], Value::Null);
    }
}
