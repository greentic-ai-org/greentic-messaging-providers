//! Adaptive Card `Input.*` element handling.
//!
//! Extracted from the original monolithic `ac_element_to_html` switch to keep
//! `ac_to_html.rs` below the 500 LOC limit. Behaviour is identical to the
//! previous inline branches — `ac_to_html::ac_element_to_html` delegates to
//! [`handle_ac_input`] for any element whose `type` starts with `Input.`.
//!
//! # `ac_pending_inputs` contract
//!
//! Each `Input.*` element encountered here is pushed into the
//! `TelegramAcContent::inputs` vector. `ops::encode::encode_op` then serialises
//! that vector and stashes it in the envelope metadata under the
//! `ac_pending_inputs` key. Downstream, `ops::send::handle_send` consumes that
//! metadata via `ac_helpers::has_pending_text_inputs` and
//! `ac_helpers::first_input_placeholder` to decide whether to attach a
//! Telegram `ForceReply` reply markup (so that the user's next message is
//! captured as the input value) instead of an inline keyboard. Do not remove
//! the metadata round-trip without also updating `handle_send`.

use serde_json::{Value, json};

use super::ac_helpers::html_escape;
use super::ac_to_html::{AcChoice, AcInput, AcInputKind};

/// Render an `Input.*` element into HTML + inline keyboard buttons and
/// register it in the `inputs` vector for ForceReply routing.
///
/// Returns `true` if the element was handled by this module (i.e. it was an
/// `Input.Text`, `Input.ChoiceSet`, `Input.Toggle`, `Input.Number`,
/// `Input.Date`, or `Input.Time`). Unknown types return `false` so that the
/// caller can fall through to its own default handler.
pub(crate) fn handle_ac_input(
    etype: &str,
    element: &Value,
    parts: &mut Vec<String>,
    actions: &mut Vec<Value>,
    inputs: &mut Vec<AcInput>,
) -> bool {
    match etype {
        "Input.Text" => {
            handle_input_text(element, parts, inputs);
            true
        }
        "Input.ChoiceSet" => {
            handle_input_choice_set(element, parts, actions, inputs);
            true
        }
        "Input.Toggle" => {
            handle_input_toggle(element, actions, inputs);
            true
        }
        "Input.Number" | "Input.Date" | "Input.Time" => {
            handle_input_text_like(element, parts, inputs);
            true
        }
        _ => false,
    }
}

fn handle_input_text(element: &Value, parts: &mut Vec<String>, inputs: &mut Vec<AcInput>) {
    let id = element
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let label = element
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let placeholder = element
        .get("placeholder")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let display_label = if !label.is_empty() {
        label
    } else if !id.is_empty() {
        id
    } else {
        "Input"
    };
    let hint = if !placeholder.is_empty() {
        format!(" <i>({})</i>", html_escape(placeholder))
    } else {
        String::new()
    };
    parts.push(format!(
        "\u{270f}\u{fe0f} <b>{}</b>{}",
        html_escape(display_label),
        hint
    ));
    if !id.is_empty() {
        inputs.push(AcInput {
            id: id.to_string(),
            label: display_label.to_string(),
            placeholder: placeholder.to_string(),
            kind: AcInputKind::Text,
            choices: vec![],
        });
    }
}

fn handle_input_choice_set(
    element: &Value,
    parts: &mut Vec<String>,
    actions: &mut Vec<Value>,
    inputs: &mut Vec<AcInput>,
) {
    let id = element
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let label = element
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let display_label = if !label.is_empty() { label } else { id };
    if !display_label.is_empty() {
        parts.push(format!("<b>{}</b>", html_escape(display_label)));
    }
    let choices: Vec<AcChoice> = element
        .get("choices")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let title = c.get("title").and_then(Value::as_str)?;
                    let value = c.get("value").and_then(Value::as_str)?;
                    Some(AcChoice {
                        title: title.to_string(),
                        value: value.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // Render choices as inline keyboard buttons.
    for choice in &choices {
        let mut btn = json!({"title": &choice.title});
        btn.as_object_mut().unwrap().insert(
            "data".into(),
            json!({"input_id": id, "input_value": &choice.value}),
        );
        actions.push(btn);
    }
    if !id.is_empty() {
        inputs.push(AcInput {
            id: id.to_string(),
            label: display_label.to_string(),
            placeholder: String::new(),
            kind: AcInputKind::Choice,
            choices,
        });
    }
}

fn handle_input_toggle(element: &Value, actions: &mut Vec<Value>, inputs: &mut Vec<AcInput>) {
    let id = element
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let title = element.get("title").and_then(Value::as_str).unwrap_or(id);
    let value_on = element
        .get("valueOn")
        .and_then(Value::as_str)
        .unwrap_or("true");
    let value_off = element
        .get("valueOff")
        .and_then(Value::as_str)
        .unwrap_or("false");
    if !id.is_empty() {
        // Render as two inline keyboard buttons.
        let mut btn_yes = json!({"title": format!("\u{2705} {title}")});
        btn_yes.as_object_mut().unwrap().insert(
            "data".into(),
            json!({"input_id": id, "input_value": value_on}),
        );
        let mut btn_no = json!({"title": format!("\u{274c} {title}")});
        btn_no.as_object_mut().unwrap().insert(
            "data".into(),
            json!({"input_id": id, "input_value": value_off}),
        );
        actions.push(btn_yes);
        actions.push(btn_no);
        inputs.push(AcInput {
            id: id.to_string(),
            label: title.to_string(),
            placeholder: String::new(),
            kind: AcInputKind::Toggle,
            choices: vec![],
        });
    }
}

fn handle_input_text_like(element: &Value, parts: &mut Vec<String>, inputs: &mut Vec<AcInput>) {
    // Treat Input.Number / Input.Date / Input.Time like Input.Text — prompt
    // user to type the value.
    let id = element
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let label = element.get("label").and_then(Value::as_str).unwrap_or(id);
    let placeholder = element
        .get("placeholder")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let hint = if !placeholder.is_empty() {
        format!(" <i>({})</i>", html_escape(placeholder))
    } else {
        String::new()
    };
    if !label.is_empty() {
        parts.push(format!(
            "\u{270f}\u{fe0f} <b>{}</b>{}",
            html_escape(label),
            hint
        ));
    }
    if !id.is_empty() {
        inputs.push(AcInput {
            id: id.to_string(),
            label: label.to_string(),
            placeholder: placeholder.to_string(),
            kind: AcInputKind::Text,
            choices: vec![],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_text_input_with_escaped_label_and_placeholder() {
        let mut parts = Vec::new();
        let mut actions = Vec::new();
        let mut inputs = Vec::new();

        let handled = handle_ac_input(
            "Input.Text",
            &json!({
                "id": "comment",
                "label": "Comment <required>",
                "placeholder": "Type & send",
                "isMultiline": true
            }),
            &mut parts,
            &mut actions,
            &mut inputs,
        );

        assert!(handled);
        assert_eq!(
            parts[0],
            "\u{270f}\u{fe0f} <b>Comment &lt;required&gt;</b> <i>(Type &amp; send)</i>"
        );
        assert!(actions.is_empty());
        assert_eq!(inputs[0].id, "comment");
        assert!(matches!(inputs[0].kind, AcInputKind::Text));
    }

    #[test]
    fn choice_set_adds_buttons_and_choice_input() {
        let mut parts = Vec::new();
        let mut actions = Vec::new();
        let mut inputs = Vec::new();

        handle_ac_input(
            "Input.ChoiceSet",
            &json!({
                "id": "priority",
                "label": "Priority",
                "choices": [
                    {"title": "High", "value": "high"},
                    {"title": "Low", "value": "low"},
                    {"title": "Missing value"}
                ]
            }),
            &mut parts,
            &mut actions,
            &mut inputs,
        );

        assert_eq!(parts[0], "<b>Priority</b>");
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["data"]["input_id"], "priority");
        assert_eq!(inputs[0].choices.len(), 2);
        assert!(matches!(inputs[0].kind, AcInputKind::Choice));
    }

    #[test]
    fn toggle_adds_yes_no_buttons() {
        let mut parts = Vec::new();
        let mut actions = Vec::new();
        let mut inputs = Vec::new();

        handle_ac_input(
            "Input.Toggle",
            &json!({
                "id": "confirm",
                "title": "Confirm",
                "valueOn": "yes",
                "valueOff": "no"
            }),
            &mut parts,
            &mut actions,
            &mut inputs,
        );

        assert!(parts.is_empty());
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["data"]["input_value"], "yes");
        assert_eq!(actions[1]["data"]["input_value"], "no");
        assert!(matches!(inputs[0].kind, AcInputKind::Toggle));
    }

    #[test]
    fn date_number_time_are_text_like_inputs() {
        let mut parts = Vec::new();
        let mut actions = Vec::new();
        let mut inputs = Vec::new();

        let handled = handle_ac_input(
            "Input.Date",
            &json!({"id": "due", "label": "Due date", "placeholder": "YYYY-MM-DD"}),
            &mut parts,
            &mut actions,
            &mut inputs,
        );

        assert!(handled);
        assert!(parts[0].contains("Due date"));
        assert!(actions.is_empty());
        assert!(matches!(inputs[0].kind, AcInputKind::Text));
    }

    #[test]
    fn unknown_input_type_is_not_handled() {
        let mut parts = Vec::new();
        let mut actions = Vec::new();
        let mut inputs = Vec::new();

        assert!(!handle_ac_input(
            "Input.Unknown",
            &json!({}),
            &mut parts,
            &mut actions,
            &mut inputs
        ));
    }
}
