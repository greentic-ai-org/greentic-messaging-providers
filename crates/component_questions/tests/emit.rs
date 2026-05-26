use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use questions::spec::QuestionsSpec;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("setup")
        .join("webex.yaml")
}

#[test]
fn emit_from_fixture() {
    let spec_text = fs::read_to_string(fixture_path()).expect("read fixture");
    let spec: questions::spec::SetupSpec = serde_yaml_bw::from_str(&spec_text).expect("parse spec");
    let output = QuestionsSpec {
        id: "webex-setup".to_string(),
        title: spec
            .title
            .clone()
            .unwrap_or_else(|| format!("{} setup", spec.provider_id)),
        actions: spec.actions,
        questions: spec
            .questions
            .iter()
            .map(questions::spec::QuestionSpecItem::try_from)
            .collect::<Result<Vec<_>, _>>()
            .expect("convert questions"),
    };
    let json_value: Value = serde_json::to_value(&output).expect("serialize output");
    insta::assert_json_snapshot!(json_value);
}

#[test]
fn emit_preserves_setup_actions() {
    let spec_text = r#"
provider_id: slack
version: 1
title: Slack provider setup
actions:
  - id: add_to_slack
    title: Add to Slack
    kind: oauth_authorize
    authorize_url: https://slack.com/oauth/v2/authorize
    client_id_field: slack_client_id
questions: []
"#;
    let spec: questions::spec::SetupSpec = serde_yaml_bw::from_str(spec_text).expect("parse spec");
    let output = QuestionsSpec {
        id: "slack-setup".to_string(),
        title: spec
            .title
            .clone()
            .unwrap_or_else(|| format!("{} setup", spec.provider_id)),
        actions: spec.actions,
        questions: vec![],
    };
    let json_value: Value = serde_json::to_value(&output).expect("serialize output");
    assert_eq!(json_value["actions"][0]["id"], "add_to_slack");
    assert_eq!(json_value["actions"][0]["title"], "Add to Slack");
    assert_eq!(json_value["actions"][0]["kind"], "oauth_authorize");
    assert_eq!(
        json_value["actions"][0]["client_id_field"],
        "slack_client_id"
    );
}
