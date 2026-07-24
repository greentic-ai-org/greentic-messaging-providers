use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::jwt::DirectLineContext;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoredActivity {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    pub timestamp: i64,
    pub watermark: u64,
    #[serde(default)]
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConversationState {
    pub ctx: DirectLineContext,
    pub next_watermark: u64,
    pub activities: Vec<StoredActivity>,
    #[serde(default)]
    pub flow_binding: Option<String>,
}

impl ConversationState {
    pub fn new(ctx: DirectLineContext) -> Self {
        ConversationState {
            ctx,
            next_watermark: 0,
            activities: Vec::new(),
            flow_binding: None,
        }
    }

    pub fn bump_watermark(&mut self) -> u64 {
        let watermark = self.next_watermark;
        self.next_watermark = self.next_watermark.saturating_add(1);
        watermark
    }
}

pub fn conversation_key(ctx: &DirectLineContext, conversation_id: &str) -> String {
    format!(
        "webchat:conv:{}:{}:{}:{}",
        ctx.env,
        ctx.tenant,
        sanitize_team(ctx.team.as_deref()),
        conversation_id
    )
}

pub fn sanitize_team(team: Option<&str>) -> String {
    team.map(|t| t.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "_".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_key_includes_parts() {
        let ctx = DirectLineContext {
            env: "env1".into(),
            tenant: "tenant42".into(),
            team: Some("team-X".into()),
        };
        let key = conversation_key(&ctx, "conv-1");
        assert_eq!(key, "webchat:conv:env1:tenant42:team-X:conv-1");
    }

    #[test]
    fn sanitize_team_falls_back() {
        assert_eq!(sanitize_team(Some("  ")), "_");
        assert_eq!(sanitize_team(None), "_");
        assert_eq!(sanitize_team(Some(" team ")), "team");
    }

    #[test]
    fn conversation_state_initial() {
        let ctx = DirectLineContext {
            env: "env".into(),
            tenant: "tenant".into(),
            team: None,
        };
        let mut state = ConversationState::new(ctx);
        assert_eq!(state.next_watermark, 0);
        assert_eq!(state.flow_binding, None);
        let first = state.bump_watermark();
        assert_eq!(first, 0);
        assert_eq!(state.next_watermark, 1);
    }

    #[test]
    fn old_persisted_state_without_flow_binding_deserializes() {
        // State persisted by the previous version lacks `flow_binding`.
        // `#[serde(default)]` must make this deserialize without error.
        let json = r#"{
            "ctx": {"env": "prod", "tenant": "acme", "team": null},
            "next_watermark": 5,
            "activities": []
        }"#;
        let state: ConversationState =
            serde_json::from_str(json).expect("old state must deserialize");
        assert_eq!(state.flow_binding, None);
        assert_eq!(state.next_watermark, 5);
    }

    #[test]
    fn state_with_flow_binding_round_trips() {
        let ctx = DirectLineContext {
            env: "env".into(),
            tenant: "tenant".into(),
            team: None,
        };
        let mut state = ConversationState::new(ctx);
        state.flow_binding = Some("welcome-flow".into());
        let serialized = serde_json::to_string(&state).expect("serialize");
        let deserialized: ConversationState =
            serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized.flow_binding, Some("welcome-flow".into()));
    }
}
