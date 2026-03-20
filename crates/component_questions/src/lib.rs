#![allow(dead_code)]

pub mod spec;

use jsonschema::Validator;
use regex::Regex;
use spec::{
    QuestionKind, QuestionSpecItem, QuestionsSpec, SetupSpec, SkipCompound, SkipCondition,
    SkipExpression,
};

use anyhow::{Result, anyhow};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::fs;
use std::path::PathBuf;

#[cfg(target_arch = "wasm32")]
use greentic_interfaces_guest::component_v0_6::node::{
    self, ComponentDescriptor, Guest, InvocationEnvelope, InvocationResult, NodeError,
};

#[derive(Debug, Deserialize)]
struct EmitInput {
    id: String,
    spec_ref: String,
    #[serde(default)]
    context: Option<Context>,
}

#[derive(Debug, Deserialize)]
struct Context {
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    env: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ValidateInput {
    spec_json: String,
    answers_json: String,
}

#[derive(Debug, Deserialize)]
struct ExampleInput {
    spec_json: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ValidateOutput {
    ok: bool,
    errors: Vec<ValidationError>,
}

#[derive(Debug, serde::Serialize)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

#[cfg(target_arch = "wasm32")]
struct QuestionsComponent;

#[cfg(target_arch = "wasm32")]
impl Guest for QuestionsComponent {
    fn describe() -> ComponentDescriptor {
        ComponentDescriptor {
            name: "questions".to_string(),
            version: "0.4.38".to_string(),
            summary: Some("Emit and validate setup questions".to_string()),
            capabilities: vec![],
            ops: vec![
                node::Op {
                    name: "emit".to_string(),
                    summary: Some("Emit questions spec from YAML".to_string()),
                    input: node::IoSchema {
                        schema: node::SchemaSource::InlineCbor(vec![]),
                        content_type: "application/json".to_string(),
                        schema_version: None,
                    },
                    output: node::IoSchema {
                        schema: node::SchemaSource::InlineCbor(vec![]),
                        content_type: "application/json".to_string(),
                        schema_version: None,
                    },
                    examples: vec![],
                },
                node::Op {
                    name: "validate".to_string(),
                    summary: Some("Validate answers against spec".to_string()),
                    input: node::IoSchema {
                        schema: node::SchemaSource::InlineCbor(vec![]),
                        content_type: "application/json".to_string(),
                        schema_version: None,
                    },
                    output: node::IoSchema {
                        schema: node::SchemaSource::InlineCbor(vec![]),
                        content_type: "application/json".to_string(),
                        schema_version: None,
                    },
                    examples: vec![],
                },
                node::Op {
                    name: "example-answers".to_string(),
                    summary: Some("Generate example answers".to_string()),
                    input: node::IoSchema {
                        schema: node::SchemaSource::InlineCbor(vec![]),
                        content_type: "application/json".to_string(),
                        schema_version: None,
                    },
                    output: node::IoSchema {
                        schema: node::SchemaSource::InlineCbor(vec![]),
                        content_type: "application/json".to_string(),
                        schema_version: None,
                    },
                    examples: vec![],
                },
            ],
            schemas: vec![],
            setup: None,
        }
    }

    fn invoke(op: String, envelope: InvocationEnvelope) -> Result<InvocationResult, NodeError> {
        // Decode CBOR payload to JSON string
        let input_json = decode_cbor_payload(&envelope.payload_cbor)?;

        let result = match op.as_str() {
            "emit" => emit(input_json),
            "validate" => validate(input_json),
            "example-answers" => example_answers(input_json),
            other => {
                return Err(NodeError {
                    code: "UNKNOWN_OPERATION".to_string(),
                    message: format!("unsupported operation {other}"),
                    retryable: false,
                    backoff_ms: None,
                    details: None,
                });
            }
        };

        match result {
            Ok(output_json) => {
                let output_cbor = encode_json_to_cbor(&output_json)?;
                Ok(InvocationResult {
                    ok: true,
                    output_cbor,
                    output_metadata_cbor: None,
                })
            }
            Err(message) => Err(NodeError {
                code: "INVALID_INPUT".to_string(),
                message,
                retryable: false,
                backoff_ms: None,
                details: None,
            }),
        }
    }
}

#[cfg(target_arch = "wasm32")]
greentic_interfaces_guest::export_component_v060!(QuestionsComponent);

#[cfg(target_arch = "wasm32")]
fn decode_cbor_payload(cbor: &[u8]) -> Result<String, NodeError> {
    if cbor.is_empty() {
        return Ok("{}".to_string());
    }
    let value: ciborium::Value = ciborium::from_reader(cbor).map_err(|e| NodeError {
        code: "CBOR_DECODE_ERROR".to_string(),
        message: format!("failed to decode CBOR payload: {e}"),
        retryable: false,
        backoff_ms: None,
        details: None,
    })?;
    // Convert CBOR value to JSON
    let json_value = cbor_to_json(value);
    serde_json::to_string(&json_value).map_err(|e| NodeError {
        code: "JSON_ENCODE_ERROR".to_string(),
        message: format!("failed to encode to JSON: {e}"),
        retryable: false,
        backoff_ms: None,
        details: None,
    })
}

#[cfg(target_arch = "wasm32")]
fn cbor_to_json(value: ciborium::Value) -> serde_json::Value {
    match value {
        ciborium::Value::Null => serde_json::Value::Null,
        ciborium::Value::Bool(b) => serde_json::Value::Bool(b),
        ciborium::Value::Integer(i) => {
            let n: i128 = i.into();
            if let Ok(n) = i64::try_from(n) {
                serde_json::Value::Number(n.into())
            } else if let Ok(n) = u64::try_from(n) {
                serde_json::Value::Number(n.into())
            } else {
                serde_json::Value::String(n.to_string())
            }
        }
        ciborium::Value::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        ciborium::Value::Bytes(b) => {
            use base64::Engine;
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(b))
        }
        ciborium::Value::Text(s) => serde_json::Value::String(s),
        ciborium::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(cbor_to_json).collect())
        }
        ciborium::Value::Map(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        ciborium::Value::Text(s) => s,
                        ciborium::Value::Integer(i) => {
                            let n: i128 = i.into();
                            n.to_string()
                        }
                        _ => return None,
                    };
                    Some((key, cbor_to_json(v)))
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        ciborium::Value::Tag(_, inner) => cbor_to_json(*inner),
        _ => serde_json::Value::Null,
    }
}

#[cfg(target_arch = "wasm32")]
fn encode_json_to_cbor(json: &str) -> Result<Vec<u8>, NodeError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| NodeError {
        code: "JSON_PARSE_ERROR".to_string(),
        message: format!("failed to parse JSON: {e}"),
        retryable: false,
        backoff_ms: None,
        details: None,
    })?;
    let mut buf = Vec::new();
    ciborium::into_writer(&value, &mut buf).map_err(|e| NodeError {
        code: "CBOR_ENCODE_ERROR".to_string(),
        message: format!("failed to encode to CBOR: {e}"),
        retryable: false,
        backoff_ms: None,
        details: None,
    })?;
    Ok(buf)
}

fn emit(input_json: String) -> Result<String, String> {
    let input: EmitInput = serde_json::from_str(&input_json).map_err(err_string)?;
    touch_context(&input.context);
    let spec = load_spec(&input.spec_ref).map_err(err_string)?;
    let title = spec
        .title
        .clone()
        .unwrap_or_else(|| format!("{} setup", spec.provider_id));
    let questions = spec
        .questions
        .iter()
        .map(QuestionSpecItem::try_from)
        .collect::<Result<Vec<_>>>()
        .map_err(err_string)?;
    let spec = QuestionsSpec {
        id: input.id,
        title,
        questions,
    };
    serde_json::to_string(&spec).map_err(err_string)
}

fn validate(input_json: String) -> Result<String, String> {
    let input: ValidateInput = serde_json::from_str(&input_json).map_err(err_string)?;
    let spec: QuestionsSpec = serde_json::from_str(&input.spec_json).map_err(err_string)?;
    let answers: Value = serde_json::from_str(&input.answers_json).map_err(err_string)?;
    let answer_map = answers.as_object().cloned().unwrap_or_else(Map::new);
    let errors = validate_answers_for_spec(&spec.questions, &answer_map);
    let output = ValidateOutput {
        ok: errors.is_empty(),
        errors,
    };
    serde_json::to_string(&output).map_err(err_string)
}

fn example_answers(input_json: String) -> Result<String, String> {
    let input: ExampleInput = serde_json::from_str(&input_json).map_err(err_string)?;
    let spec: QuestionsSpec = serde_json::from_str(&input.spec_json).map_err(err_string)?;
    let value = example_answers_for_spec(&spec.questions);
    serde_json::to_string(&value).map_err(err_string)
}

fn err_string(err: impl std::fmt::Display) -> String {
    format!("{err}")
}

fn touch_context(context: &Option<Context>) {
    if let Some(ctx) = context {
        let _ = (&ctx.tenant_id, &ctx.env);
    }
}

fn load_spec(spec_ref: &str) -> Result<SetupSpec> {
    let path = resolve_spec_path(spec_ref);
    let contents = fs::read_to_string(&path)
        .map_err(|e| anyhow!("failed to read spec at {}: {}", path.display(), e))?;
    let spec: SetupSpec = serde_yaml_bw::from_str(&contents)?;
    Ok(spec)
}

fn resolve_spec_path(spec_ref: &str) -> PathBuf {
    if let Some(stripped) = spec_ref.strip_prefix("assets/") {
        return PathBuf::from("/assets").join(stripped);
    }
    PathBuf::from(spec_ref)
}

// ---------------------------------------------------------------------------
// Skip expression evaluation
// ---------------------------------------------------------------------------

/// Evaluate a single condition against answers.
fn evaluate_condition(cond: &SkipCondition, answers: &Map<String, Value>) -> bool {
    let field_value = answers.get(&cond.field);

    if let Some(eq_val) = &cond.equals {
        return field_value == Some(eq_val);
    }
    if let Some(ne_val) = &cond.not_equals {
        return field_value != Some(ne_val);
    }
    if cond.is_empty {
        return is_field_empty(field_value);
    }
    if cond.is_not_empty {
        return !is_field_empty(field_value);
    }
    false
}

/// Check if a field value is considered empty.
fn is_field_empty(value: Option<&Value>) -> bool {
    match value {
        None => true,
        Some(Value::Null) => true,
        Some(Value::String(s)) if s.trim().is_empty() => true,
        Some(Value::Array(arr)) if arr.is_empty() => true,
        Some(Value::Object(obj)) if obj.is_empty() => true,
        _ => false,
    }
}

/// Recursively evaluate skip expression.
pub fn evaluate_skip_expression(expr: &SkipExpression, answers: &Map<String, Value>) -> bool {
    match expr {
        SkipExpression::Condition(cond) => evaluate_condition(cond, answers),
        SkipExpression::Compound(compound) => evaluate_compound(compound, answers),
    }
}

/// Evaluate compound expression (AND/OR/NOT).
fn evaluate_compound(compound: &SkipCompound, answers: &Map<String, Value>) -> bool {
    // Handle NOT first (negates inner expression)
    if let Some(inner) = &compound.not {
        return !evaluate_skip_expression(inner, answers);
    }

    // Handle AND (all conditions must be true)
    if let Some(exprs) = &compound.and {
        return exprs.iter().all(|e| evaluate_skip_expression(e, answers));
    }

    // Handle OR (at least one condition must be true)
    if let Some(exprs) = &compound.or {
        return exprs.iter().any(|e| evaluate_skip_expression(e, answers));
    }

    // Empty compound defaults to false
    false
}

/// Check if question should be skipped based on skip_if expression.
pub fn should_skip_question(question: &QuestionSpecItem, answers: &Map<String, Value>) -> bool {
    question
        .skip_if
        .as_ref()
        .map(|expr| evaluate_skip_expression(expr, answers))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_answers_for_spec(
    questions: &[QuestionSpecItem],
    answers: &Map<String, Value>,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for question in questions {
        // Skip validation if question is skipped
        if should_skip_question(question, answers) {
            continue;
        }

        let value = answers.get(&question.name);
        if question.required && is_missing(value) {
            errors.push(ValidationError {
                path: question.name.clone(),
                message: "required".to_string(),
            });
            continue;
        }
        let Some(value) = value else { continue };
        if value.is_null() {
            continue;
        }
        match question.kind {
            QuestionKind::String => {
                let Some(text) = value.as_str() else {
                    errors.push(type_error(&question.name, "string"));
                    continue;
                };
                if let Some(validate) = question.validate.as_ref()
                    && let Some(regex) = validate.regex.as_ref()
                    && let Ok(pattern) = Regex::new(regex)
                    && !pattern.is_match(text)
                {
                    errors.push(ValidationError {
                        path: question.name.clone(),
                        message: "regex".to_string(),
                    });
                }
                if !question.choices.is_empty()
                    && !question.choices.iter().any(|choice| choice == value)
                {
                    errors.push(ValidationError {
                        path: question.name.clone(),
                        message: "choice".to_string(),
                    });
                }
            }
            QuestionKind::Bool => {
                if !value.is_boolean() {
                    errors.push(type_error(&question.name, "bool"));
                }
            }
            QuestionKind::Number => {
                let Some(num) = value.as_f64() else {
                    errors.push(type_error(&question.name, "number"));
                    continue;
                };
                if let Some(validate) = question.validate.as_ref() {
                    if let Some(min) = validate.min
                        && num < min
                    {
                        errors.push(ValidationError {
                            path: question.name.clone(),
                            message: "min".to_string(),
                        });
                    }
                    if let Some(max) = validate.max
                        && num > max
                    {
                        errors.push(ValidationError {
                            path: question.name.clone(),
                            message: "max".to_string(),
                        });
                    }
                }
            }
            QuestionKind::Choice => {
                if question.choices.is_empty() {
                    if !value.is_string() {
                        errors.push(type_error(&question.name, "string"));
                    }
                } else if !question.choices.iter().any(|choice| choice == value) {
                    errors.push(ValidationError {
                        path: question.name.clone(),
                        message: "choice".to_string(),
                    });
                }
            }
            QuestionKind::InlineJson => {
                // Parse JSON from string or accept object/array directly
                let json_value = if let Some(text) = value.as_str() {
                    match serde_json::from_str::<Value>(text) {
                        Ok(v) => v,
                        Err(e) => {
                            errors.push(ValidationError {
                                path: question.name.clone(),
                                message: format!("invalid JSON syntax: {e}"),
                            });
                            continue;
                        }
                    }
                } else if value.is_object() || value.is_array() {
                    value.clone()
                } else {
                    errors.push(type_error(&question.name, "JSON string, object, or array"));
                    continue;
                };

                // Validate against JSON Schema if provided
                if let Some(validate) = &question.validate
                    && let Some(schema) = &validate.json_schema
                {
                    match Validator::new(schema) {
                        Ok(compiled) => {
                            let validation_result = compiled.validate(&json_value);
                            if let Err(validation_error) = validation_result {
                                // ValidationError is a single error, not an iterator
                                errors.push(ValidationError {
                                    path: format!(
                                        "{}.{}",
                                        question.name,
                                        validation_error.instance_path()
                                    ),
                                    message: validation_error.to_string(),
                                });
                            }
                        }
                        Err(e) => {
                            errors.push(ValidationError {
                                path: question.name.clone(),
                                message: format!("invalid JSON schema: {e}"),
                            });
                        }
                    }
                }
            }
            QuestionKind::AssetRef => {
                if let Some(path_str) = value.as_str() {
                    let validate = question.validate.as_ref();

                    // Build full path with base_path if specified
                    let full_path = if let Some(base) = validate.and_then(|v| v.base_path.as_ref())
                    {
                        std::path::PathBuf::from(base).join(path_str)
                    } else {
                        std::path::PathBuf::from(path_str)
                    };

                    // Validate file extension
                    if let Some(v) = validate
                        && !v.file_types.is_empty()
                    {
                        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if !v.file_types.iter().any(|t| t.eq_ignore_ascii_case(ext)) {
                            errors.push(ValidationError {
                                path: question.name.clone(),
                                message: format!(
                                    "file type '{}' not allowed, must be one of: {:?}",
                                    ext, v.file_types
                                ),
                            });
                        }
                    }

                    // Check file existence (default: true)
                    let check_exists = validate.map(|v| v.check_exists).unwrap_or(true);
                    if check_exists && !full_path.exists() {
                        errors.push(ValidationError {
                            path: question.name.clone(),
                            message: format!("file not found: {}", full_path.display()),
                        });
                    }
                } else if !value.is_null() {
                    errors.push(type_error(&question.name, "file path string"));
                }
            }
        }
    }
    errors
}

pub fn example_answers_for_spec(questions: &[QuestionSpecItem]) -> Value {
    let mut out = Map::new();
    for question in questions {
        let value = if let Some(default) = question.default.clone() {
            default
        } else {
            match question.kind {
                QuestionKind::String => Value::String(String::new()),
                QuestionKind::Bool => Value::Bool(false),
                QuestionKind::Number => Value::Number(0.into()),
                QuestionKind::Choice => question
                    .choices
                    .first()
                    .cloned()
                    .unwrap_or_else(|| Value::String(String::new())),
                QuestionKind::InlineJson => Value::String(
                    r#"{"type": "AdaptiveCard", "version": "1.5", "body": []}"#.to_string(),
                ),
                QuestionKind::AssetRef => Value::String("./assets/example.json".to_string()),
            }
        };
        out.insert(question.name.clone(), value);
    }
    Value::Object(out)
}

fn is_missing(value: Option<&Value>) -> bool {
    match value {
        None => true,
        Some(Value::Null) => true,
        Some(Value::String(s)) if s.trim().is_empty() => true,
        _ => false,
    }
}

fn type_error(path: &str, expected: &str) -> ValidationError {
    ValidationError {
        path: path.to_string(),
        message: format!("expected {expected}"),
    }
}
