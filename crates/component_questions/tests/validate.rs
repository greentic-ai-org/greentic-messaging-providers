use serde_json::json;

use questions::spec::{
    QuestionKind, QuestionSpecItem, QuestionValidate, QuestionsSpec, SkipCompound, SkipCondition,
    SkipExpression,
};

// ---------------------------------------------------------------------------
// Helper to create a minimal question spec item
// ---------------------------------------------------------------------------

fn make_question(name: &str, kind: QuestionKind, required: bool) -> QuestionSpecItem {
    QuestionSpecItem {
        name: name.to_string(),
        title: name.to_string(),
        kind,
        required,
        default: None,
        help: None,
        choices: vec![],
        validate: None,
        secret: false,
        skip_if: None,
    }
}

fn make_question_with_validate(
    name: &str,
    kind: QuestionKind,
    required: bool,
    validate: QuestionValidate,
) -> QuestionSpecItem {
    QuestionSpecItem {
        name: name.to_string(),
        title: name.to_string(),
        kind,
        required,
        default: None,
        help: None,
        choices: vec![],
        validate: Some(validate),
        secret: false,
        skip_if: None,
    }
}

// ---------------------------------------------------------------------------
// Existing tests (updated for new skip_if field)
// ---------------------------------------------------------------------------

#[test]
fn validate_required_and_regex() {
    let spec = QuestionsSpec {
        id: "webex-setup".to_string(),
        title: "Webex provider setup".to_string(),
        actions: vec![],
        questions: vec![
            QuestionSpecItem {
                name: "webhook_base_url".to_string(),
                title: "Public base URL".to_string(),
                kind: QuestionKind::String,
                required: true,
                default: None,
                help: None,
                choices: vec![],
                validate: Some(QuestionValidate {
                    regex: Some("^https://".to_string()),
                    min: None,
                    max: None,
                    json_schema: None,
                    file_types: vec![],
                    base_path: None,
                    check_exists: true,
                }),
                secret: false,
                skip_if: None,
            },
            QuestionSpecItem {
                name: "bot_token".to_string(),
                title: "Webex bot token".to_string(),
                kind: QuestionKind::String,
                required: true,
                default: None,
                help: None,
                choices: vec![],
                validate: None,
                secret: true,
                skip_if: None,
            },
        ],
    };

    let answers = json!({
        "webhook_base_url": "http://not-https"
    });
    let output =
        questions::validate_answers_for_spec(&spec.questions, answers.as_object().unwrap());
    assert_eq!(output.len(), 2);
    assert!(output.iter().any(|err| err.path == "bot_token"));
    assert!(output.iter().any(|err| err.path == "webhook_base_url"));
}

// ---------------------------------------------------------------------------
// Skip expression tests
// ---------------------------------------------------------------------------

#[test]
fn skip_simple_equals_condition() {
    let questions = vec![
        make_question("mode", QuestionKind::Choice, true),
        QuestionSpecItem {
            skip_if: Some(SkipExpression::Condition(SkipCondition {
                field: "mode".to_string(),
                equals: Some(json!("simple")),
                not_equals: None,
                is_empty: false,
                is_not_empty: false,
            })),
            ..make_question("advanced_setting", QuestionKind::String, true)
        },
    ];

    // When mode is "simple", advanced_setting should be skipped
    let answers = json!({ "mode": "simple" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(
        errors.is_empty(),
        "advanced_setting should be skipped when mode is simple"
    );

    // When mode is "advanced", advanced_setting should be required
    let answers = json!({ "mode": "advanced" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "advanced_setting");
}

#[test]
fn skip_not_equals_condition() {
    let questions = vec![
        make_question("auth_method", QuestionKind::Choice, true),
        QuestionSpecItem {
            skip_if: Some(SkipExpression::Condition(SkipCondition {
                field: "auth_method".to_string(),
                equals: None,
                not_equals: Some(json!("oauth")),
                is_empty: false,
                is_not_empty: false,
            })),
            ..make_question("oauth_client_id", QuestionKind::String, true)
        },
    ];

    // When auth_method is "token", oauth_client_id should be skipped
    let answers = json!({ "auth_method": "token" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());

    // When auth_method is "oauth", oauth_client_id should be required
    let answers = json!({ "auth_method": "oauth" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "oauth_client_id");
}

#[test]
fn skip_is_empty_condition() {
    let questions = vec![
        make_question("optional_url", QuestionKind::String, false),
        QuestionSpecItem {
            skip_if: Some(SkipExpression::Condition(SkipCondition {
                field: "optional_url".to_string(),
                equals: None,
                not_equals: None,
                is_empty: true,
                is_not_empty: false,
            })),
            ..make_question("url_auth_token", QuestionKind::String, true)
        },
    ];

    // When optional_url is empty, url_auth_token should be skipped
    let answers = json!({});
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());

    // When optional_url has value, url_auth_token should be required
    let answers = json!({ "optional_url": "https://example.com" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "url_auth_token");
}

#[test]
fn skip_is_not_empty_condition() {
    let questions = vec![
        make_question("custom_webhook", QuestionKind::String, false),
        QuestionSpecItem {
            skip_if: Some(SkipExpression::Condition(SkipCondition {
                field: "custom_webhook".to_string(),
                equals: None,
                not_equals: None,
                is_empty: false,
                is_not_empty: true,
            })),
            ..make_question("default_webhook", QuestionKind::String, true)
        },
    ];

    // When custom_webhook has value, default_webhook should be skipped
    let answers = json!({ "custom_webhook": "https://custom.com" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());

    // When custom_webhook is empty, default_webhook should be required
    let answers = json!({});
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "default_webhook");
}

#[test]
fn skip_and_expression() {
    let questions = vec![
        make_question("enable_notifications", QuestionKind::Bool, true),
        make_question("notification_channel", QuestionKind::Choice, true),
        QuestionSpecItem {
            skip_if: Some(SkipExpression::Compound(SkipCompound {
                and: Some(vec![
                    SkipExpression::Condition(SkipCondition {
                        field: "enable_notifications".to_string(),
                        equals: Some(json!(false)),
                        not_equals: None,
                        is_empty: false,
                        is_not_empty: false,
                    }),
                    SkipExpression::Condition(SkipCondition {
                        field: "notification_channel".to_string(),
                        equals: Some(json!("email")),
                        not_equals: None,
                        is_empty: false,
                        is_not_empty: false,
                    }),
                ]),
                or: None,
                not: None,
            })),
            ..make_question("slack_webhook", QuestionKind::String, true)
        },
    ];

    // Both conditions true => skip
    let answers = json!({
        "enable_notifications": false,
        "notification_channel": "email"
    });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());

    // Only one condition true => don't skip
    let answers = json!({
        "enable_notifications": true,
        "notification_channel": "email"
    });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "slack_webhook");
}

#[test]
fn skip_or_expression() {
    let questions = vec![
        make_question("auth_method", QuestionKind::Choice, true),
        QuestionSpecItem {
            skip_if: Some(SkipExpression::Compound(SkipCompound {
                and: None,
                or: Some(vec![
                    SkipExpression::Condition(SkipCondition {
                        field: "auth_method".to_string(),
                        equals: Some(json!("none")),
                        not_equals: None,
                        is_empty: false,
                        is_not_empty: false,
                    }),
                    SkipExpression::Condition(SkipCondition {
                        field: "auth_method".to_string(),
                        equals: Some(json!("token")),
                        not_equals: None,
                        is_empty: false,
                        is_not_empty: false,
                    }),
                ]),
                not: None,
            })),
            ..make_question("oauth_config", QuestionKind::String, true)
        },
    ];

    // auth_method is "none" => skip
    let answers = json!({ "auth_method": "none" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());

    // auth_method is "token" => skip
    let answers = json!({ "auth_method": "token" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());

    // auth_method is "oauth" => don't skip, require oauth_config
    let answers = json!({ "auth_method": "oauth" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "oauth_config");
}

#[test]
fn skip_not_expression() {
    let questions = vec![
        make_question("use_default", QuestionKind::Bool, true),
        QuestionSpecItem {
            skip_if: Some(SkipExpression::Compound(SkipCompound {
                and: None,
                or: None,
                not: Some(Box::new(SkipExpression::Condition(SkipCondition {
                    field: "use_default".to_string(),
                    equals: Some(json!(false)),
                    not_equals: None,
                    is_empty: false,
                    is_not_empty: false,
                }))),
            })),
            ..make_question("custom_value", QuestionKind::String, true)
        },
    ];

    // use_default is true => NOT(false) = NOT(false) => true => skip
    let answers = json!({ "use_default": true });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());

    // use_default is false => NOT(true) = false => don't skip
    let answers = json!({ "use_default": false });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "custom_value");
}

#[test]
fn skip_nested_expression() {
    // Skip if: (provider is "basic") OR (provider is "advanced" AND webhooks disabled)
    let questions = vec![
        make_question("provider_type", QuestionKind::Choice, true),
        make_question("enable_webhooks", QuestionKind::Bool, true),
        QuestionSpecItem {
            skip_if: Some(SkipExpression::Compound(SkipCompound {
                and: None,
                or: Some(vec![
                    SkipExpression::Condition(SkipCondition {
                        field: "provider_type".to_string(),
                        equals: Some(json!("basic")),
                        not_equals: None,
                        is_empty: false,
                        is_not_empty: false,
                    }),
                    SkipExpression::Compound(SkipCompound {
                        and: Some(vec![
                            SkipExpression::Condition(SkipCondition {
                                field: "provider_type".to_string(),
                                equals: Some(json!("advanced")),
                                not_equals: None,
                                is_empty: false,
                                is_not_empty: false,
                            }),
                            SkipExpression::Condition(SkipCondition {
                                field: "enable_webhooks".to_string(),
                                equals: Some(json!(false)),
                                not_equals: None,
                                is_empty: false,
                                is_not_empty: false,
                            }),
                        ]),
                        or: None,
                        not: None,
                    }),
                ]),
                not: None,
            })),
            ..make_question("webhook_secret", QuestionKind::String, true)
        },
    ];

    // provider is "basic" => skip
    let answers = json!({ "provider_type": "basic", "enable_webhooks": true });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());

    // provider is "advanced" AND webhooks disabled => skip
    let answers = json!({ "provider_type": "advanced", "enable_webhooks": false });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());

    // provider is "advanced" AND webhooks enabled => don't skip
    let answers = json!({ "provider_type": "advanced", "enable_webhooks": true });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "webhook_secret");
}

// ---------------------------------------------------------------------------
// InlineJson tests
// ---------------------------------------------------------------------------

#[test]
fn inline_json_valid_json_string() {
    let questions = vec![make_question("card_json", QuestionKind::InlineJson, true)];

    let answers = json!({
        "card_json": r#"{"type": "AdaptiveCard", "version": "1.5"}"#
    });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());
}

#[test]
fn inline_json_valid_json_object() {
    let questions = vec![make_question("card_json", QuestionKind::InlineJson, true)];

    let answers = json!({
        "card_json": {"type": "AdaptiveCard", "version": "1.5"}
    });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());
}

#[test]
fn inline_json_valid_json_array() {
    let questions = vec![make_question("items", QuestionKind::InlineJson, true)];

    let answers = json!({
        "items": [{"id": 1}, {"id": 2}]
    });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());
}

#[test]
fn inline_json_invalid_syntax() {
    let questions = vec![make_question("card_json", QuestionKind::InlineJson, true)];

    let answers = json!({
        "card_json": "{ invalid json }"
    });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("invalid JSON syntax"));
}

#[test]
fn inline_json_wrong_type() {
    let questions = vec![make_question("card_json", QuestionKind::InlineJson, true)];

    let answers = json!({
        "card_json": 12345
    });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0]
            .message
            .contains("expected JSON string, object, or array")
    );
}

#[test]
fn inline_json_with_schema_valid() {
    let questions = vec![make_question_with_validate(
        "card_json",
        QuestionKind::InlineJson,
        true,
        QuestionValidate {
            regex: None,
            min: None,
            max: None,
            json_schema: Some(json!({
                "type": "object",
                "required": ["type", "version"],
                "properties": {
                    "type": { "type": "string" },
                    "version": { "type": "string" }
                }
            })),
            file_types: vec![],
            base_path: None,
            check_exists: true,
        },
    )];

    let answers = json!({
        "card_json": {"type": "AdaptiveCard", "version": "1.5"}
    });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());
}

#[test]
fn inline_json_with_schema_invalid() {
    let questions = vec![make_question_with_validate(
        "card_json",
        QuestionKind::InlineJson,
        true,
        QuestionValidate {
            regex: None,
            min: None,
            max: None,
            json_schema: Some(json!({
                "type": "object",
                "required": ["type", "version"],
                "properties": {
                    "type": { "type": "string" },
                    "version": { "type": "string" }
                }
            })),
            file_types: vec![],
            base_path: None,
            check_exists: true,
        },
    )];

    // Missing required "version" field
    let answers = json!({
        "card_json": {"type": "AdaptiveCard"}
    });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].path.starts_with("card_json"));
}

// ---------------------------------------------------------------------------
// AssetRef tests
// ---------------------------------------------------------------------------

#[test]
fn asset_ref_valid_extension() {
    let questions = vec![make_question_with_validate(
        "card_file",
        QuestionKind::AssetRef,
        true,
        QuestionValidate {
            regex: None,
            min: None,
            max: None,
            json_schema: None,
            file_types: vec!["json".to_string(), "yaml".to_string()],
            base_path: None,
            check_exists: false, // Don't check existence in test
        },
    )];

    let answers = json!({ "card_file": "cards/my-card.json" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());
}

#[test]
fn asset_ref_invalid_extension() {
    let questions = vec![make_question_with_validate(
        "card_file",
        QuestionKind::AssetRef,
        true,
        QuestionValidate {
            regex: None,
            min: None,
            max: None,
            json_schema: None,
            file_types: vec!["json".to_string(), "yaml".to_string()],
            base_path: None,
            check_exists: false,
        },
    )];

    let answers = json!({ "card_file": "cards/my-card.txt" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("file type"));
    assert!(errors[0].message.contains("not allowed"));
}

#[test]
fn asset_ref_case_insensitive_extension() {
    let questions = vec![make_question_with_validate(
        "card_file",
        QuestionKind::AssetRef,
        true,
        QuestionValidate {
            regex: None,
            min: None,
            max: None,
            json_schema: None,
            file_types: vec!["json".to_string()],
            base_path: None,
            check_exists: false,
        },
    )];

    let answers = json!({ "card_file": "cards/my-card.JSON" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());
}

#[test]
fn asset_ref_with_base_path() {
    let questions = vec![make_question_with_validate(
        "card_file",
        QuestionKind::AssetRef,
        true,
        QuestionValidate {
            regex: None,
            min: None,
            max: None,
            json_schema: None,
            file_types: vec![],
            base_path: Some("assets/cards/".to_string()),
            check_exists: false,
        },
    )];

    let answers = json!({ "card_file": "my-card.json" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert!(errors.is_empty());
}

#[test]
fn asset_ref_file_not_found() {
    let questions = vec![make_question_with_validate(
        "card_file",
        QuestionKind::AssetRef,
        true,
        QuestionValidate {
            regex: None,
            min: None,
            max: None,
            json_schema: None,
            file_types: vec![],
            base_path: None,
            check_exists: true, // Enable file existence check
        },
    )];

    let answers = json!({ "card_file": "/nonexistent/path/to/file.json" });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("file not found"));
}

#[test]
fn asset_ref_wrong_type() {
    let questions = vec![make_question("card_file", QuestionKind::AssetRef, true)];

    let answers = json!({ "card_file": 12345 });
    let errors = questions::validate_answers_for_spec(&questions, answers.as_object().unwrap());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("expected file path string"));
}

// ---------------------------------------------------------------------------
// Example answers tests
// ---------------------------------------------------------------------------

#[test]
fn example_answers_includes_new_types() {
    let questions = vec![
        make_question("name", QuestionKind::String, true),
        make_question("enabled", QuestionKind::Bool, true),
        make_question("count", QuestionKind::Number, true),
        make_question("card_json", QuestionKind::InlineJson, true),
        make_question("card_file", QuestionKind::AssetRef, true),
    ];

    let examples = questions::example_answers_for_spec(&questions);
    let map = examples.as_object().unwrap();

    assert!(map.contains_key("name"));
    assert!(map.contains_key("enabled"));
    assert!(map.contains_key("count"));
    assert!(map.contains_key("card_json"));
    assert!(map.contains_key("card_file"));

    // Check InlineJson example is valid JSON
    let card_json = map.get("card_json").unwrap().as_str().unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(card_json).is_ok());

    // Check AssetRef example is a path string
    assert!(map.get("card_file").unwrap().is_string());
}
