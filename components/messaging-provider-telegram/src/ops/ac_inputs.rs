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
