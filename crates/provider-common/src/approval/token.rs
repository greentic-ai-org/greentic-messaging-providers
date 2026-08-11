//! The approval rail's decision token.

use core::fmt;
use serde::{Deserialize, Serialize};

/// Single-use credential minted by the designer, one per publish.
///
/// The contract forbids this value reaching a log line at any level, in any
/// form — so there is no `Display`, `Debug` is redacted, and the only way to
/// read the value is [`DecisionToken::expose`], which is greppable.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DecisionToken(String);

impl DecisionToken {
    /// Wrap a token value. A blank value is not a token.
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Every call site of this is a place the credential can escape.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DecisionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DecisionToken(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_token() {
        let token = DecisionToken::new("s3cr3t-token-value").expect("token");
        let rendered = format!("{token:?}");
        assert_eq!(rendered, "DecisionToken(<redacted>)");
        assert!(!rendered.contains("s3cr3t"));
    }

    #[test]
    fn blank_values_are_not_tokens() {
        assert!(DecisionToken::new("").is_none());
        assert!(DecisionToken::new("   ").is_none());
    }

    #[test]
    fn serialises_transparently_for_the_wire() {
        let token = DecisionToken::new("abc").expect("token");
        assert_eq!(
            serde_json::to_value(&token).expect("json"),
            serde_json::json!("abc")
        );
    }
}
