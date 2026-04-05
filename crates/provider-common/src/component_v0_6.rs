use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationDescriptor {
    pub name: String,
    pub title: I18nText,
    pub description: I18nText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactionRule {
    pub path: String,
    pub strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribePayload {
    pub provider: String,
    pub world: String,
    pub operations: Vec<OperationDescriptor>,
    pub input_schema: SchemaIr,
    pub output_schema: SchemaIr,
    pub config_schema: SchemaIr,
    pub redactions: Vec<RedactionRule>,
    pub schema_hash: String,

    // -- Fields below replace component.manifest.json --
    /// Structured capability contract (WASI + host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<DescribeCapabilities>,

    /// Profile metadata (default + supported list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<DescribeProfiles>,

    /// Secret requirements declared by the component.
    #[serde(default)]
    pub secret_requirements: Vec<DescribeSecretRequirement>,
}

/// Capability contract matching greentic-types ComponentCapabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeCapabilities {
    #[serde(default)]
    pub wasi: DescribeWasiCapabilities,
    #[serde(default)]
    pub host: DescribeHostCapabilities,
}

/// WASI capability declarations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeWasiCapabilities {
    #[serde(default)]
    pub random: bool,
    #[serde(default)]
    pub clocks: bool,
}

/// Host capability declarations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeHostCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<DescribeStateCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secrets: Option<DescribeSecretsCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messaging: Option<DescribeMessagingCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<DescribeEventsCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<DescribeHttpCapabilities>,
}

/// State access requirements.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeStateCapabilities {
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub write: bool,
}

/// Secrets access requirements.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeSecretsCapabilities {
    #[serde(default)]
    pub required: Vec<DescribeSecretRequirement>,
}

/// Messaging capability declarations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeMessagingCapabilities {
    #[serde(default)]
    pub inbound: bool,
    #[serde(default)]
    pub outbound: bool,
}

/// Events capability declarations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeEventsCapabilities {
    #[serde(default)]
    pub inbound: bool,
    #[serde(default)]
    pub outbound: bool,
}

/// HTTP capability declarations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeHttpCapabilities {
    #[serde(default)]
    pub client: bool,
    #[serde(default)]
    pub server: bool,
}

/// Profile metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeProfiles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub supported: Vec<String>,
}

/// Secret requirement entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DescribeSecretRequirement {
    pub key: String,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaIr {
    Bool {
        title: I18nText,
        description: I18nText,
    },
    String {
        title: I18nText,
        description: I18nText,
        format: Option<String>,
        secret: bool,
    },
    Object {
        title: I18nText,
        description: I18nText,
        fields: BTreeMap<String, SchemaField>,
        additional_properties: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaField {
    pub required: bool,
    pub schema: SchemaIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct I18nText {
    pub key: String,
}

/// Question kind — matches `ComponentQaSpec.QuestionKind` in greentic-types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum QuestionKind {
    Text,
    Choice {
        options: Vec<ChoiceOption>,
    },
    Number,
    Bool,
    /// Inline JSON input with optional JSON Schema validation.
    InlineJson {
        /// Optional JSON Schema for validation (Draft 2020-12).
        #[serde(skip_serializing_if = "Option::is_none")]
        schema: Option<serde_json::Value>,
    },
    /// Asset file/directory path reference with optional existence check.
    AssetRef {
        /// Allowed file extensions (e.g., `["json", "yaml"]`).
        #[serde(default)]
        file_types: Vec<String>,
        /// Base path for resolving relative paths (e.g., `"assets/"`).
        #[serde(skip_serializing_if = "Option::is_none")]
        base_path: Option<String>,
        /// Whether to check file existence (default: `true`).
        #[serde(default = "default_true")]
        check_exists: bool,
    },
}

fn default_true() -> bool {
    true
}

/// Choice option for `QuestionKind::Choice`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChoiceOption {
    pub value: String,
    pub label: I18nText,
}

/// Skip condition expression — supports AND/OR with nesting.
///
/// Used for conditional questions that should be skipped based on previous answers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkipExpression {
    /// Single condition: field equals/not_equals/is_empty/is_not_empty.
    Condition(SkipCondition),
    /// All conditions must be true (logical AND).
    And(Vec<SkipExpression>),
    /// At least one condition must be true (logical OR).
    Or(Vec<SkipExpression>),
    /// Negate the inner expression (logical NOT).
    Not(Box<SkipExpression>),
}

/// Single skip condition for field comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkipCondition {
    /// The field name to check in the answers.
    pub field: String,
    /// Skip if field equals this value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<serde_json::Value>,
    /// Skip if field does not equal this value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_equals: Option<serde_json::Value>,
    /// Skip if field is empty (null, missing, or empty string).
    #[serde(default)]
    pub is_empty: bool,
    /// Skip if field is not empty.
    #[serde(default)]
    pub is_not_empty: bool,
}

/// QA question — matches `ComponentQaSpec.Question` in greentic-types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QaQuestionSpec {
    pub id: String,
    pub label: I18nText,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<I18nText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<I18nText>,
    pub kind: QuestionKind,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Condition to skip this question based on previous answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_if: Option<SkipExpression>,
}

/// QA spec — matches `ComponentQaSpec` in greentic-types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QaSpec {
    pub mode: String,
    pub title: I18nText,
    #[serde(default)]
    pub description: Option<I18nText>,
    pub questions: Vec<QaQuestionSpec>,
    #[serde(default)]
    pub defaults: std::collections::BTreeMap<String, serde_json::Value>,
}

pub fn schema_hash(input: &SchemaIr, output: &SchemaIr, config: &SchemaIr) -> String {
    let value = serde_json::json!({
        "input": input,
        "output": output,
        "config": config,
    });
    sha256_hex(&to_canonical_cbor(&value))
}

pub fn canonical_cbor_bytes(value: &impl Serialize) -> Vec<u8> {
    to_canonical_cbor(value)
}

pub fn to_canonical_cbor(value: &impl Serialize) -> Vec<u8> {
    let value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
    let canonical = canonicalize_json(value);
    let mut out = Vec::new();
    let _ = ciborium::ser::into_writer(&canonical, &mut out);
    out
}

pub fn to_canonical_cbor_allow_floats(value: &impl Serialize) -> Vec<u8> {
    to_canonical_cbor(value)
}

pub fn decode_cbor<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    ciborium::de::from_reader(bytes).map_err(|err| err.to_string())
}

pub fn default_en_message_for_key(key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        return "Message".to_string();
    }

    let mut words = key
        .split('.')
        .next_back()
        .unwrap_or(key)
        .split('_')
        .filter_map(|token| {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_ascii_lowercase())
            }
        })
        .collect::<Vec<_>>();

    if words.is_empty() {
        return "Message".to_string();
    }

    for word in &mut words {
        match word.as_str() {
            "qa" | "op" | "schema" | "config" | "input" | "output" => {}
            "id" => *word = "ID".to_string(),
            "url" => *word = "URL".to_string(),
            "http" => *word = "HTTP".to_string(),
            "api" => *word = "API".to_string(),
            "ui" => *word = "UI".to_string(),
            "i18n" => *word = "I18N".to_string(),
            _ => {
                let mut chars = word.chars();
                if let Some(first) = chars.next() {
                    *word = format!("{}{}", first.to_ascii_uppercase(), chars.as_str());
                }
            }
        }
    }

    words.join(" ")
}

pub fn default_en_i18n_messages(keys: &[&str]) -> serde_json::Map<String, serde_json::Value> {
    keys.iter()
        .map(|key| {
            (
                (*key).to_string(),
                serde_json::Value::String(default_en_message_for_key(key)),
            )
        })
        .collect::<serde_json::Map<String, serde_json::Value>>()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn canonicalize_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonicalize_json).collect::<Vec<_>>())
        }
        serde_json::Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (key, value) in map {
                sorted.insert(key, canonicalize_json(value));
            }
            let ordered = sorted
                .into_iter()
                .collect::<serde_json::Map<String, serde_json::Value>>();
            serde_json::Value::Object(ordered)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_human_readable_default_i18n_message() {
        assert_eq!(
            default_en_message_for_key("teams.qa.setup.public_base_url"),
            "Public Base URL"
        );
        assert_eq!(
            default_en_message_for_key("telegram.schema.output.message_id.title"),
            "Title"
        );
        assert_eq!(default_en_message_for_key(""), "Message");
    }

    #[test]
    fn describe_payload_cbor_round_trip_with_new_fields() {
        let payload = DescribePayload {
            provider: "test-provider".into(),
            world: "test-world".into(),
            operations: vec![OperationDescriptor {
                name: "run".into(),
                title: I18nText {
                    key: "op.run.title".into(),
                },
                description: I18nText {
                    key: "op.run.description".into(),
                },
            }],
            input_schema: SchemaIr::Bool {
                title: I18nText {
                    key: "input.title".into(),
                },
                description: I18nText {
                    key: "input.description".into(),
                },
            },
            output_schema: SchemaIr::Bool {
                title: I18nText {
                    key: "output.title".into(),
                },
                description: I18nText {
                    key: "output.description".into(),
                },
            },
            config_schema: SchemaIr::Bool {
                title: I18nText {
                    key: "config.title".into(),
                },
                description: I18nText {
                    key: "config.description".into(),
                },
            },
            redactions: vec![],
            schema_hash: "dummy-hash".into(),
            capabilities: Some(DescribeCapabilities {
                host: DescribeHostCapabilities {
                    http: Some(DescribeHttpCapabilities {
                        client: true,
                        server: false,
                    }),
                    secrets: Some(DescribeSecretsCapabilities {
                        required: vec![DescribeSecretRequirement {
                            key: "API_KEY".into(),
                            scope: Some("tenant".into()),
                            description: Some("API key".into()),
                            required: true,
                        }],
                    }),
                    state: Some(DescribeStateCapabilities {
                        read: true,
                        write: true,
                    }),
                    ..Default::default()
                },
                ..Default::default()
            }),
            profiles: Some(DescribeProfiles {
                default: Some("default".into()),
                supported: vec!["default".into(), "advanced".into()],
            }),
            secret_requirements: vec![DescribeSecretRequirement {
                key: "API_KEY".into(),
                scope: Some("tenant".into()),
                description: Some("API key".into()),
                required: true,
            }],
        };

        let cbor_bytes = to_canonical_cbor(&payload);
        assert!(!cbor_bytes.is_empty(), "CBOR should not be empty");

        let decoded: DescribePayload =
            decode_cbor(&cbor_bytes).expect("should decode CBOR round-trip");

        assert_eq!(decoded.provider, "test-provider");
        assert_eq!(decoded.world, "test-world");
        assert_eq!(decoded.operations.len(), 1);
        assert_eq!(decoded.operations[0].name, "run");

        let capabilities = decoded.capabilities.as_ref().expect("capabilities present");
        let host = &capabilities.host;
        assert_eq!(host.http.as_ref().unwrap().client, true);
        assert_eq!(host.http.as_ref().unwrap().server, false);
        assert_eq!(host.secrets.as_ref().unwrap().required.len(), 1);
        assert_eq!(host.secrets.as_ref().unwrap().required[0].key, "API_KEY");
        assert_eq!(host.state.as_ref().unwrap().read, true);
        assert_eq!(host.state.as_ref().unwrap().write, true);

        assert_eq!(decoded.secret_requirements.len(), 1);
        assert_eq!(decoded.secret_requirements[0].key, "API_KEY");
        assert_eq!(
            decoded.secret_requirements[0].scope.as_deref(),
            Some("tenant")
        );

        let profiles = decoded.profiles.as_ref().expect("profiles present");
        assert_eq!(profiles.default.as_deref(), Some("default"));
        assert_eq!(profiles.supported.len(), 2);
        assert_eq!(profiles.supported, vec!["default", "advanced"]);
    }
}
