use super::*;

const CONFORMANCE_REQUEST: &str =
    include_str!("../../../../../tests/fixtures/approval_rail/request_v2.json");
const CONFORMANCE_RESPONSE: &str =
    include_str!("../../../../../tests/fixtures/approval_rail/response_v2.json");
const TOKEN: &str = "EXAMPLE-TOKEN-NOT-A-REAL-SECRET";
const CORRELATION_ID: &str = "default::run=RUN-1::node=gate";

fn op_result(input: Value) -> Value {
    serde_json::from_slice(&approval_request_op(input.to_string().as_bytes())).expect("op result")
}

fn slack_body(result: &Value) -> Value {
    let body_b64 = result["payload"]["body_b64"].as_str().expect("body_b64");
    serde_json::from_slice(&STANDARD.decode(body_b64).expect("body")).expect("body json")
}

fn conformance_delivery() -> Value {
    op_result(json!({
        "correlation_id": CORRELATION_ID,
        "request": serde_json::from_str::<Value>(CONFORMANCE_REQUEST).expect("fixture"),
        "channel": "C123"
    }))
}

fn button_value(body: &Value, action_id: &str) -> String {
    body["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .filter(|block| block["type"] == "actions")
        .flat_map(|block| block["elements"].as_array().cloned().unwrap_or_default())
        .find(|element| element["action_id"] == action_id)
        .and_then(|element| element["value"].as_str().map(str::to_string))
        .expect("button state")
}

fn click(state: &str, action_id: &str, email: Option<&str>) -> Value {
    let mut user = json!({"id": "U123", "username": "boss"});
    if let Some(email) = email
        && let Some(object) = user.as_object_mut()
    {
        object.insert("profile".into(), json!({"email": email}));
    }
    json!({
        "type": "block_actions",
        "channel": {"id": "C123"},
        "user": user,
        "message": {"ts": "1700000000.000100", "blocks": [
            {"type": "header", "text": {"type": "plain_text", "text": "Approval needed"}},
            {"type": "section", "text": {"type": "plain_text", "text": "Refund 1200 USD"}}
        ]},
        "actions": [{"action_id": action_id, "value": state}]
    })
}

fn response_extension(envelope: &ChannelMessageEnvelope) -> &Value {
    envelope
        .extensions
        .get(RESPONSE_EXTENSION_KEY)
        .expect("response extension")
}

fn strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => out.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

#[test]
fn the_conformance_request_round_trips_to_the_conformance_response() {
    let delivery = conformance_delivery();
    let state = button_value(&slack_body(&delivery), ACTION_ID_APPROVE);

    let interaction = build_interaction(
        &click(&state, ACTION_ID_APPROVE, Some("boss@acme.test")),
        Some("boss@acme.test".to_string()),
    )
    .expect("interaction");

    let expected: Value = serde_json::from_str(CONFORMANCE_RESPONSE).expect("fixture");
    let extension = response_extension(&interaction.envelope);
    assert_eq!(extension["body"], expected);
    assert_eq!(extension["subject"], RESPONSE_SUBJECT);
    assert_eq!(extension["headers"][CORRELATION_ID_HEADER], CORRELATION_ID);
}

#[test]
fn the_token_is_never_placed_in_a_url() {
    let delivery = conformance_delivery();
    let body = slack_body(&delivery);

    let mut found = Vec::new();
    strings(&body, &mut found);
    strings(&delivery["payload"]["metadata"], &mut found);
    for text in &found {
        if text.contains("://") || text.starts_with('/') || text.contains('?') {
            assert!(
                !text.contains(TOKEN),
                "token reached a URL-shaped string: {text}"
            );
        }
    }

    assert_eq!(
        delivery["payload"]["metadata"]["url"],
        format!("{DEFAULT_API_BASE}/chat.postMessage")
    );
    assert!(!body["text"].as_str().expect("text").contains(TOKEN));
    assert!(
        !serde_json::to_string(&body["metadata"])
            .expect("metadata")
            .contains(TOKEN)
    );
}

#[test]
fn the_click_never_forwards_the_token_into_envelope_metadata_or_the_http_body() {
    let state = button_value(&slack_body(&conformance_delivery()), ACTION_ID_APPROVE);
    let interaction = build_interaction(
        &click(&state, ACTION_ID_APPROVE, Some("boss@acme.test")),
        Some("boss@acme.test".to_string()),
    )
    .expect("interaction");

    for (key, value) in &interaction.envelope.metadata {
        assert!(!value.contains(TOKEN), "token leaked into metadata[{key}]");
    }

    let out: Value = serde_json::from_slice(&interaction.into_http_out()).expect("http out");
    let http_body = String::from_utf8(
        STANDARD
            .decode(out["body_b64"].as_str().expect("body_b64"))
            .expect("body"),
    )
    .expect("utf8");
    assert!(!http_body.contains(TOKEN));
    assert!(http_body.contains("replace_original"));
}

#[test]
fn the_notification_text_escapes_an_author_supplied_title() {
    let delivery = op_result(json!({
        "request": {
            "target": CORRELATION_ID,
            "operation": "request",
            "input": {"title": "Refund <!channel> & friends"},
            "routing": {"decision_token": TOKEN}
        },
        "channel": "C123"
    }));
    let text = slack_body(&delivery)["text"]
        .as_str()
        .expect("text")
        .to_string();

    assert!(!text.contains("<!channel>"));
    assert!(text.contains("&lt;!channel&gt; &amp; friends"));
}

#[test]
fn a_republish_updates_the_existing_message_instead_of_posting_a_second_one() {
    let request: Value = serde_json::from_str(CONFORMANCE_REQUEST).expect("fixture");
    let first = conformance_delivery();
    assert_eq!(first["update"], false);
    assert_eq!(
        first["payload"]["metadata"]["url"],
        format!("{DEFAULT_API_BASE}/chat.postMessage")
    );
    assert!(slack_body(&first).get("ts").is_none());

    let mut republished = request.clone();
    republished["routing"]["decision_token"] = json!("SECOND-TOKEN-FOR-THE-QUORUM");
    let second = op_result(json!({
        "correlation_id": CORRELATION_ID,
        "request": republished,
        "channel": "C123",
        "message_ts": "1700000000.000100"
    }));

    assert_eq!(second["update"], true);
    assert_eq!(second["correlation_id"], CORRELATION_ID);
    assert_eq!(
        second["payload"]["metadata"]["url"],
        format!("{DEFAULT_API_BASE}/chat.update")
    );
    let body = slack_body(&second);
    assert_eq!(body["ts"], "1700000000.000100");
    assert_eq!(body["channel"], "C123");
    assert!(button_value(&body, ACTION_ID_APPROVE).contains("SECOND-TOKEN-FOR-THE-QUORUM"));
    assert!(!button_value(&body, ACTION_ID_APPROVE).contains(TOKEN));
}

#[test]
fn a_token_only_routing_block_delivers_and_answers() {
    let delivery = op_result(json!({
        "request": {
            "target": CORRELATION_ID,
            "operation": "request",
            "input": {"title": "Unresolvable policy"},
            "routing": {"decision_token": TOKEN}
        },
        "channel": "C123"
    }));
    assert_eq!(delivery["ok"], true);

    let state = button_value(&slack_body(&delivery), ACTION_ID_DENY);
    let interaction = build_interaction(
        &click(&state, ACTION_ID_DENY, Some("boss@acme.test")),
        Some("boss@acme.test".to_string()),
    )
    .expect("interaction");

    let body = &response_extension(&interaction.envelope)["body"];
    assert_eq!(body["output"]["decision"], "denied");
    assert_eq!(body["output"]["decision_token"], TOKEN);
}

#[test]
fn a_gate_that_was_never_issued_a_token_answers_without_one() {
    let delivery = op_result(json!({
        "request": {
            "target": CORRELATION_ID,
            "operation": "request",
            "input": {"title": "Legacy gate"}
        },
        "channel": "C123"
    }));

    let state = button_value(&slack_body(&delivery), ACTION_ID_APPROVE);
    let interaction = build_interaction(
        &click(&state, ACTION_ID_APPROVE, Some("boss@acme.test")),
        Some("boss@acme.test".to_string()),
    )
    .expect("interaction");

    let body = &response_extension(&interaction.envelope)["body"];
    assert!(body["output"].get("decision_token").is_none());
    assert_eq!(
        interaction
            .envelope
            .metadata
            .get("greentic.approval.carries_token")
            .map(String::as_str),
        Some("false")
    );
}

#[test]
fn an_unnamed_voter_is_sent_as_no_claimed_identity() {
    let state = button_value(&slack_body(&conformance_delivery()), ACTION_ID_APPROVE);
    let interaction =
        build_interaction(&click(&state, ACTION_ID_APPROVE, None), None).expect("interaction");

    let body = &response_extension(&interaction.envelope)["body"];
    assert_eq!(body["output"]["resolved_by"], Value::Null);
    assert_eq!(body["output"]["decision_token"], TOKEN);
}

#[test]
fn rejecting_uses_the_deny_action_and_not_the_button_state() {
    let state = button_value(&slack_body(&conformance_delivery()), ACTION_ID_DENY);
    let interaction = build_interaction(
        &click(&state, ACTION_ID_DENY, Some("boss@acme.test")),
        Some("boss@acme.test".to_string()),
    )
    .expect("interaction");

    let body = &response_extension(&interaction.envelope)["body"];
    assert_eq!(body["output"]["decision"], "denied");
    assert_eq!(
        interaction.envelope.text.as_deref(),
        Some("[approval:denied]")
    );
}

#[test]
fn other_block_actions_are_left_to_the_existing_path() {
    let unrelated = json!({
        "type": "block_actions",
        "actions": [{"action_id": "ac_action_0", "value": "{\"routeToCardId\":\"card-1\"}"}]
    });
    assert!(!is_approval_interaction(&unrelated));
    assert!(is_approval_interaction(&click(
        &blocks::button_state(CORRELATION_ID, Some(TOKEN)),
        ACTION_ID_APPROVE,
        None
    )));
}

#[test]
fn a_click_with_no_correlation_id_is_refused_rather_than_guessed() {
    let click = click("{\"v\":1}", ACTION_ID_APPROVE, Some("boss@acme.test"));
    assert!(build_interaction(&click, Some("boss@acme.test".to_string())).is_err());
}

#[test]
fn the_op_rejects_input_that_is_not_a_request() {
    let missing_channel = op_result(json!({
        "request": {"target": CORRELATION_ID, "operation": "request", "input": {}}
    }));
    assert_eq!(missing_channel["ok"], false);
    assert_eq!(missing_channel["error"], "destination (to) required");

    let wrong_operation = op_result(json!({
        "request": {"target": CORRELATION_ID, "operation": "response"},
        "channel": "C123"
    }));
    assert_eq!(wrong_operation["error"], "approval request required");

    let invalid: Value =
        serde_json::from_slice(&approval_request_op(b"{")).expect("invalid json result");
    assert_eq!(invalid["ok"], false);
}
