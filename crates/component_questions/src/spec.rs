use anyhow::anyhow;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Skip expression types (YAML parsing)
// ---------------------------------------------------------------------------

/// Skip expression — supports nested AND/OR logic.
///
/// Parsed from YAML using `#[serde(untagged)]` to allow both shorthand
/// (simple condition) and compound expressions.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum SkipExpression {
    /// Compound expression: `{ and: [...] }`, `{ or: [...] }`, or `{ not: {...} }`.
    Compound(SkipCompound),
    /// Simple condition (shorthand): `{ field: "x", equals: "y" }`.
    Condition(SkipCondition),
}

/// Compound skip expression with AND/OR/NOT operators.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SkipCompound {
    /// All conditions must be true.
    #[serde(default)]
    pub and: Option<Vec<SkipExpression>>,
    /// At least one condition must be true.
    #[serde(default)]
    pub or: Option<Vec<SkipExpression>>,
    /// Negate the inner expression.
    #[serde(default)]
    pub not: Option<Box<SkipExpression>>,
}

/// Single skip condition for field comparison.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SkipCondition {
    /// The field name to check in the answers.
    pub field: String,
    /// Skip if field equals this value.
    #[serde(default)]
    pub equals: Option<Value>,
    /// Skip if field does not equal this value.
    #[serde(default)]
    pub not_equals: Option<Value>,
    /// Skip if field is empty (null, missing, or empty string).
    #[serde(default)]
    pub is_empty: bool,
    /// Skip if field is not empty.
    #[serde(default)]
    pub is_not_empty: bool,
}

// ---------------------------------------------------------------------------
// Spec types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SetupSpec {
    pub provider_id: String,
    pub version: u32,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub questions: Vec<QuestionDef>,
}

#[derive(Debug, Deserialize)]
pub struct QuestionDef {
    pub name: String,
    pub title: String,
    pub kind: QuestionKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub choices: Vec<Value>,
    #[serde(default)]
    pub validate: Option<QuestionValidate>,
    #[serde(default)]
    pub secret: bool,
    /// Skip this question if the expression evaluates to true.
    #[serde(default)]
    pub skip_if: Option<SkipExpression>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    String,
    Bool,
    Number,
    Choice,
    /// Inline JSON input with optional JSON Schema validation.
    InlineJson,
    /// Asset file/directory path reference with optional existence check.
    AssetRef,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct QuestionValidate {
    /// Regex pattern for string validation.
    #[serde(default)]
    pub regex: Option<String>,
    /// Minimum value for number validation.
    #[serde(default)]
    pub min: Option<f64>,
    /// Maximum value for number validation.
    #[serde(default)]
    pub max: Option<f64>,
    /// JSON Schema for InlineJson validation (Draft 2020-12).
    #[serde(default)]
    pub json_schema: Option<Value>,
    /// Allowed file extensions for AssetRef (e.g., `["json", "yaml"]`).
    #[serde(default)]
    pub file_types: Vec<String>,
    /// Base path for AssetRef resolution (e.g., `"assets/"`).
    #[serde(default)]
    pub base_path: Option<String>,
    /// Check if file exists for AssetRef (default: `true`).
    #[serde(default = "default_true")]
    pub check_exists: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuestionsSpec {
    pub id: String,
    pub title: String,
    pub questions: Vec<QuestionSpecItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QuestionSpecItem {
    pub name: String,
    pub title: String,
    pub kind: QuestionKind,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub choices: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate: Option<QuestionValidate>,
    pub secret: bool,
    /// Skip this question if the expression evaluates to true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_if: Option<SkipExpression>,
}

impl TryFrom<&QuestionDef> for QuestionSpecItem {
    type Error = anyhow::Error;

    fn try_from(value: &QuestionDef) -> Result<Self, Self::Error> {
        if let Some(validate) = value.validate.as_ref()
            && let Some(regex) = validate.regex.as_ref()
        {
            Regex::new(regex).map_err(|e| anyhow!("invalid regex for {}: {e}", value.name))?;
        }
        Ok(Self {
            name: value.name.clone(),
            title: value.title.clone(),
            kind: value.kind.clone(),
            required: value.required,
            default: value.default.clone(),
            help: value.help.clone(),
            choices: value.choices.clone(),
            validate: value.validate.clone(),
            secret: value.secret,
            skip_if: value.skip_if.clone(),
        })
    }
}
