//! Collect AC input field specs for Slack modal rendering, and mark
//! Action.Submit buttons as modal triggers when inputs are present.
//!
//! Used by the Slack modal flow (`open_slack_modal` + `handle_view_submission`
//! in `ops/modal.rs`) via `ac_to_slack_blocks` in `ops/blockkit/mod.rs`:
//! `collect_ac_input_fields` produces the input spec list embedded in the
//! outgoing message metadata (`event_type: "ac_modal_inputs"` in
//! `ops/encode.rs`), and `inject_modal_metadata` tags Action.Submit buttons so
//! `ops/ingest.rs` knows to open a modal (`views.open`) instead of producing
//! an envelope directly. On submit, the modal state is drained back into an
//! envelope by `handle_view_submission`.

use serde_json::{Value, json};

/// Recursively collect input field specs (Input.Text + Input.ChoiceSet) from an AC body
/// for rendering in a Slack modal instead of inline in the message.
pub(super) fn collect_ac_input_fields(elements: &[Value], inputs: &mut Vec<Value>) {
    for element in elements {
        let etype = element
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match etype {
            "Input.Text" => {
                let id = element.get("id").and_then(Value::as_str).unwrap_or("input");
                let label = element
                    .get("label")
                    .and_then(Value::as_str)
                    .or_else(|| element.get("placeholder").and_then(Value::as_str))
                    .unwrap_or(id);
                let placeholder = element
                    .get("placeholder")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let is_required = element
                    .get("isRequired")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let is_multiline = element
                    .get("isMultiline")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                inputs.push(json!({
                    "input_type": "text",
                    "id": id,
                    "label": label,
                    "placeholder": placeholder,
                    "required": is_required,
                    "multiline": is_multiline,
                }));
            }
            "Input.ChoiceSet" => {
                let id = element
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("choice");
                let label = element
                    .get("label")
                    .and_then(Value::as_str)
                    .or_else(|| element.get("placeholder").and_then(Value::as_str))
                    .unwrap_or(id);
                let placeholder = element
                    .get("placeholder")
                    .and_then(Value::as_str)
                    .unwrap_or("Select");
                let is_required = element
                    .get("isRequired")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let is_multi = element
                    .get("isMultiSelect")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let choices = element
                    .get("choices")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                inputs.push(json!({
                    "input_type": "choice",
                    "id": id,
                    "label": label,
                    "placeholder": placeholder,
                    "required": is_required,
                    "multi": is_multi,
                    "choices": choices,
                }));
            }
            "Container" => {
                if let Some(items) = element.get("items").and_then(Value::as_array) {
                    collect_ac_input_fields(items, inputs);
                }
            }
            "ColumnSet" => {
                if let Some(cols) = element.get("columns").and_then(Value::as_array) {
                    for col in cols {
                        if let Some(items) = col.get("items").and_then(Value::as_array) {
                            collect_ac_input_fields(items, inputs);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Mark Action.Submit buttons as modal triggers by injecting `ac_modal: true`
/// into the button value. Input specs are stored in message metadata instead
/// of button value to avoid exceeding Slack's 2000 char limit.
pub(super) fn inject_modal_metadata(actions: &mut [Value]) {
    for action in actions.iter_mut() {
        let is_url_button = action.get("url").is_some();
        if is_url_button {
            continue;
        }
        let existing_value = action.get("value").and_then(Value::as_str).unwrap_or("{}");
        let mut val: Value = serde_json::from_str(existing_value).unwrap_or(json!({}));
        if let Some(obj) = val.as_object_mut() {
            obj.insert("ac_modal".into(), json!(true));
        }
        if let Some(obj) = action.as_object_mut() {
            obj.insert("value".into(), Value::String(val.to_string()));
        }
    }
}
