use serde_json::json;

use questions::spec::{QuestionKind, QuestionSpecItem, QuestionsSpec};

#[test]
fn example_answers_uses_defaults() {
    let spec = QuestionsSpec {
        id: "example".to_string(),
        title: "Example".to_string(),
        actions: vec![],
        questions: vec![
            QuestionSpecItem {
                name: "flag".to_string(),
                title: "Flag".to_string(),
                kind: QuestionKind::Bool,
                required: false,
                default: Some(json!(true)),
                help: None,
                choices: vec![],
                validate: None,
                secret: false,
                skip_if: None,
            },
            QuestionSpecItem {
                name: "name".to_string(),
                title: "Name".to_string(),
                kind: QuestionKind::String,
                required: true,
                default: None,
                help: None,
                choices: vec![],
                validate: None,
                secret: false,
                skip_if: None,
            },
        ],
    };

    let value = questions::example_answers_for_spec(&spec.questions);
    assert_eq!(value.get("flag"), Some(&json!(true)));
    assert_eq!(value.get("name"), Some(&json!("")));
}
