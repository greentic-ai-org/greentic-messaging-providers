use serde::{Deserialize, Serialize};

/// Represents a normalized message flowing through providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub content: String,
}

impl Message {
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_message() {
        let msg = Message::new("id-123", "hello");
        assert_eq!(msg.id, "id-123");
        assert_eq!(msg.content, "hello");
    }
}
