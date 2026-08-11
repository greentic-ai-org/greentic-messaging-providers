//! Parsing of a `greentic.approval.request.v1` body.
//!
//! Every field is read independently and defensively: the whole `routing`
//! block may be absent, and when present may carry `decision_token` alone.

use serde::Serialize;
use serde_json::Value;

use super::token::DecisionToken;

/// A request as delivered on the rail. Unrecognised keys are ignored.
#[derive(Clone, Debug, Default)]
pub struct ApprovalRequest {
    /// Same string as the `Greentic-Correlation-Id` header.
    pub target: String,
    pub operation: String,
    /// The gate's author-supplied input, kept raw. Never trusted as markup.
    pub input: Value,
    pub routing: Option<Routing>,
}

impl ApprovalRequest {
    pub fn from_value(value: &Value) -> Self {
        Self {
            target: string_field(value, "target").unwrap_or_default(),
            operation: string_field(value, "operation").unwrap_or_default(),
            input: value.get("input").cloned().unwrap_or(Value::Null),
            routing: value.get("routing").and_then(Routing::from_value),
        }
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, String> {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|err| format!("invalid approval request: {err}"))?;
        Ok(Self::from_value(&value))
    }

    pub fn title(&self) -> Option<&str> {
        self.input
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn risk(&self) -> Option<f64> {
        self.input.get("risk").and_then(Value::as_f64)
    }

    pub fn confidence(&self) -> Option<f64> {
        self.input.get("confidence").and_then(Value::as_f64)
    }

    pub fn decision_token(&self) -> Option<&DecisionToken> {
        self.routing
            .as_ref()
            .and_then(|r| r.decision_token.as_ref())
    }
}

/// The additive `routing` sibling key. Any subset of it may be missing.
#[derive(Clone, Debug, Default)]
pub struct Routing {
    pub policy_id: Option<String>,
    pub tier: Option<Tier>,
    pub approvers: Option<Approvers>,
    /// Delivery preference. Advisory — the designer neither enforces nor
    /// verifies it.
    pub channels: Vec<String>,
    pub decision_token: Option<DecisionToken>,
}

impl Routing {
    fn from_value(value: &Value) -> Option<Self> {
        if !value.is_object() {
            return None;
        }
        Some(Self {
            policy_id: string_field(value, "policy_id"),
            tier: value.get("tier").and_then(Tier::from_value),
            approvers: value.get("approvers").and_then(Approvers::from_value),
            channels: value
                .get("channels")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            decision_token: string_field(value, "decision_token").and_then(DecisionToken::new),
        })
    }

    /// The gate's policy could not be resolved; only the token came through.
    pub fn is_token_only(&self) -> bool {
        self.policy_id.is_none()
            && self.tier.is_none()
            && self.approvers.is_none()
            && self.channels.is_empty()
    }
}

/// The governing tier of the escalation chain.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Tier {
    /// The policy author's LABEL. Display only — nothing validates it for
    /// uniqueness, so it is never an identity.
    pub level: Option<i64>,
    /// The 0-based chain position the token binds to. `null` is a bound value
    /// meaning "not escalated", never `0`.
    pub position: Option<i64>,
    pub chain_len: Option<i64>,
    pub min_approvals: Option<i64>,
    /// `null` means this tier never escalates on its own.
    pub deadline_ms: Option<i64>,
}

impl Tier {
    fn from_value(value: &Value) -> Option<Self> {
        if !value.is_object() {
            return None;
        }
        Some(Self {
            level: value.get("level").and_then(Value::as_i64),
            position: value.get("position").and_then(Value::as_i64),
            chain_len: value.get("chain_len").and_then(Value::as_i64),
            min_approvals: value.get("min_approvals").and_then(Value::as_i64),
            deadline_ms: value.get("deadline_ms").and_then(Value::as_i64),
        })
    }

    pub fn escalates(&self) -> bool {
        self.deadline_ms.is_some()
    }

    pub fn needs_quorum(&self) -> bool {
        self.min_approvals.unwrap_or(1) > 1
    }

    /// "2 of 3" when both halves are known, using the position the token binds
    /// to rather than the display label.
    pub fn chain_display(&self) -> Option<String> {
        let position = self.position?;
        let chain_len = self.chain_len?;
        Some(format!("{} of {}", position.saturating_add(1), chain_len))
    }
}

/// Who may approve. Read defensively — a per-approver shape is reserved.
#[derive(Clone, Debug, Default)]
pub struct Approvers {
    /// `any` / `admin` today. Not a closed enum: an unrecognised role means
    /// "a role I cannot evaluate", and the explicit email list stands.
    pub role: Option<String>,
    pub emails: Vec<String>,
}

impl Approvers {
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Object(_) => Some(Self {
                role: string_field(value, "role"),
                emails: emails_from(value.get("emails")),
            }),
            Value::Array(entries) => Some(Self {
                role: entries.iter().find_map(|entry| string_field(entry, "role")),
                emails: entries
                    .iter()
                    .filter_map(|entry| {
                        entry
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| string_field(entry, "email"))
                    })
                    .collect(),
            }),
            _ => None,
        }
    }

    /// No list means no list check — such a gate rests on the token alone.
    pub fn has_explicit_list(&self) -> bool {
        !self.emails.is_empty()
    }
}

fn emails_from(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CONFORMANCE_REQUEST: &str =
        include_str!("../../../../tests/fixtures/approval_rail/request_v2.json");

    fn conformance() -> ApprovalRequest {
        ApprovalRequest::from_slice(CONFORMANCE_REQUEST.as_bytes()).expect("conformance request")
    }

    #[test]
    fn parses_the_conformance_fixture() {
        let request = conformance();
        assert_eq!(request.target, "default::run=RUN-1::node=gate");
        assert_eq!(request.operation, "request");
        assert_eq!(request.title(), Some("Refund 1200 USD"));
        assert_eq!(request.risk(), Some(0.8));

        let routing = request.routing.as_ref().expect("routing");
        assert_eq!(routing.policy_id.as_deref(), Some("refunds"));
        assert_eq!(routing.channels, vec!["slack".to_string()]);
        assert!(!routing.is_token_only());
        assert_eq!(
            request.decision_token().map(DecisionToken::expose),
            Some("EXAMPLE-TOKEN-NOT-A-REAL-SECRET")
        );
    }

    #[test]
    fn position_null_stays_null_and_is_never_level() {
        let tier = conformance()
            .routing
            .and_then(|routing| routing.tier)
            .expect("tier");

        assert_eq!(tier.level, Some(1));
        assert_eq!(tier.position, None);
        assert_eq!(
            serde_json::to_value(&tier).expect("tier json")["position"],
            Value::Null
        );
        assert_eq!(tier.chain_display(), None);
        assert!(tier.needs_quorum());
        assert!(tier.escalates());
    }

    #[test]
    fn explicit_null_deadline_means_never_escalates() {
        let tier = Tier::from_value(&json!({"position": 0, "chain_len": 2, "deadline_ms": null}))
            .expect("tier");
        assert!(!tier.escalates());
        assert_eq!(tier.chain_display().as_deref(), Some("1 of 2"));
    }

    #[test]
    fn a_token_only_routing_block_is_still_usable() {
        let request = ApprovalRequest::from_value(&json!({
            "target": "t::run=R::node=n",
            "operation": "request",
            "input": {},
            "routing": {"decision_token": "tok"}
        }));
        let routing = request.routing.as_ref().expect("routing");
        assert!(routing.is_token_only());
        assert!(routing.tier.is_none());
        assert!(request.decision_token().is_some());
    }

    #[test]
    fn an_absent_routing_block_parses() {
        let request = ApprovalRequest::from_value(&json!({
            "target": "t::run=R::node=n",
            "operation": "request",
            "input": {"title": "Do the thing"}
        }));
        assert!(request.routing.is_none());
        assert!(request.decision_token().is_none());
        assert_eq!(request.title(), Some("Do the thing"));
    }

    #[test]
    fn unknown_keys_and_reserved_shapes_do_not_fail_the_parse() {
        let request = ApprovalRequest::from_value(&json!({
            "target": "t::run=R::node=n",
            "operation": "request",
            "input": {"title": "Later contract"},
            "future_sibling": {"anything": true},
            "routing": {
                "decision_token": "tok",
                "policy_id": "refunds",
                "approvers": [
                    {"email": "boss@acme.test", "role": "reviewer", "decision_token": "per-approver"}
                ],
                "unknown_routing_key": [1, 2, 3]
            }
        }));

        let routing = request.routing.as_ref().expect("routing");
        let approvers = routing.approvers.as_ref().expect("approvers");
        assert_eq!(approvers.emails, vec!["boss@acme.test".to_string()]);
        assert_eq!(approvers.role.as_deref(), Some("reviewer"));
        assert!(approvers.has_explicit_list());
        assert!(request.decision_token().is_some());
    }

    #[test]
    fn malformed_field_types_degrade_that_field_only() {
        let request = ApprovalRequest::from_value(&json!({
            "target": "t::run=R::node=n",
            "operation": "request",
            "routing": {
                "decision_token": "tok",
                "tier": "not-an-object",
                "approvers": 7,
                "channels": "slack"
            }
        }));
        let routing = request.routing.as_ref().expect("routing");
        assert!(routing.tier.is_none());
        assert!(routing.approvers.is_none());
        assert!(routing.channels.is_empty());
        assert!(request.decision_token().is_some());
    }
}
