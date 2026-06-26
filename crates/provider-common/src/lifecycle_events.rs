use greentic_types::MessageMetadata;

pub const USER_ENTERED_EVENT_TYPE: &str = "channel.user.entered";
pub const METADATA_EVENT_TYPE: &str = "event_type";
pub const METADATA_AUTO_START: &str = "autoStart";
pub const METADATA_PROVIDER: &str = "provider";
pub const METADATA_REASON: &str = "reason";
pub const METADATA_IDEMPOTENCY_KEY: &str = "idempotency_key";

pub fn user_entered_idempotency_key(
    provider: &str,
    scope: Option<&str>,
    conversation: Option<&str>,
    user: Option<&str>,
    reason: &str,
) -> String {
    format!(
        "lifecycle.user_entered:{}:{}:{}:{}:{}",
        key_part(provider),
        key_part(scope.unwrap_or_default()),
        key_part(conversation.unwrap_or_default()),
        key_part(user.unwrap_or_default()),
        key_part(reason)
    )
}

pub fn mark_user_entered(
    metadata: &mut MessageMetadata,
    provider: &str,
    reason: &str,
    idempotency_key: impl Into<String>,
) {
    metadata.insert(
        METADATA_EVENT_TYPE.to_string(),
        USER_ENTERED_EVENT_TYPE.to_string(),
    );
    metadata.insert(METADATA_AUTO_START.to_string(), "true".to_string());
    metadata.insert(METADATA_PROVIDER.to_string(), provider.to_string());
    metadata.insert(METADATA_REASON.to_string(), reason.to_string());
    metadata.insert(METADATA_IDEMPOTENCY_KEY.to_string(), idempotency_key.into());
}

pub fn is_user_entered_autostart(metadata: &MessageMetadata) -> bool {
    metadata
        .get(METADATA_EVENT_TYPE)
        .is_some_and(|value| value == USER_ENTERED_EVENT_TYPE)
        || metadata
            .get(METADATA_AUTO_START)
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

pub fn user_entered_fallback_idempotency_key(
    provider: &str,
    session_id: &str,
    user_id: Option<&str>,
    reason: Option<&str>,
) -> String {
    user_entered_idempotency_key(
        provider,
        None,
        Some(session_id),
        user_id,
        reason.unwrap_or("entered"),
    )
}

fn key_part(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "_".to_string()
    } else {
        trimmed.replace(':', "_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_stable_user_entered_idempotency_key() {
        assert_eq!(
            user_entered_idempotency_key(
                "slack",
                Some("T1"),
                Some("C1"),
                Some("U1"),
                "app_home_opened"
            ),
            "lifecycle.user_entered:slack:T1:C1:U1:app_home_opened"
        );
    }

    #[test]
    fn normalizes_empty_and_colon_key_parts() {
        assert_eq!(
            user_entered_idempotency_key("teams", None, Some("19:abc"), Some("u:1"), "bot_added"),
            "lifecycle.user_entered:teams:_:19_abc:u_1:bot_added"
        );
    }

    #[test]
    fn detects_user_entered_event_type() {
        let mut metadata = MessageMetadata::new();
        metadata.insert(
            METADATA_EVENT_TYPE.to_string(),
            USER_ENTERED_EVENT_TYPE.to_string(),
        );

        assert!(is_user_entered_autostart(&metadata));
    }

    #[test]
    fn detects_legacy_autostart_metadata() {
        let mut metadata = MessageMetadata::new();
        metadata.insert(METADATA_AUTO_START.to_string(), "true".to_string());

        assert!(is_user_entered_autostart(&metadata));
    }

    #[test]
    fn ignores_non_lifecycle_metadata() {
        let mut metadata = MessageMetadata::new();
        metadata.insert(METADATA_AUTO_START.to_string(), "false".to_string());

        assert!(!is_user_entered_autostart(&metadata));
    }

    #[test]
    fn builds_fallback_idempotency_key_from_session() {
        assert_eq!(
            user_entered_fallback_idempotency_key(
                "webchat",
                "conversation-1",
                Some("user-1"),
                None
            ),
            "lifecycle.user_entered:webchat:_:conversation-1:user-1:entered"
        );
    }
}
