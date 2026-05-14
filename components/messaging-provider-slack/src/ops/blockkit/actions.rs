//! Convert AC actions and `selectAction` into Slack button elements.

use serde_json::{Value, json};

/// Extract a human-readable label from a Container/Column's child items.
/// Recurses into ColumnSet > Column > items and Container > items to find
/// a suitable TextBlock label.
pub(super) fn label_from_items(items: &[Value]) -> String {
    // First try: bold TextBlock at this level
    for item in items {
        if item.get("type").and_then(Value::as_str) == Some("TextBlock")
            && item
                .get("weight")
                .and_then(Value::as_str)
                .is_some_and(|w| w.eq_ignore_ascii_case("bolder"))
            && let Some(text) = item.get("text").and_then(Value::as_str)
        {
            let t = text.trim();
            if !t.is_empty() {
                return t.chars().take(75).collect();
            }
        }
    }
    // Fallback: first non-empty TextBlock at this level
    for item in items {
        if item.get("type").and_then(Value::as_str) == Some("TextBlock")
            && let Some(text) = item.get("text").and_then(Value::as_str)
        {
            let t = text.trim();
            if !t.is_empty() {
                return t.chars().take(75).collect();
            }
        }
    }
    // Recurse into nested structures (ColumnSet > Column, Container)
    for item in items {
        let etype = item.get("type").and_then(Value::as_str).unwrap_or_default();
        let nested = match etype {
            "ColumnSet" => item.get("columns").and_then(Value::as_array).map(|cols| {
                cols.iter()
                    .filter_map(|col| col.get("items").and_then(Value::as_array))
                    .flatten()
                    .cloned()
                    .collect::<Vec<Value>>()
            }),
            "Container" => item.get("items").and_then(Value::as_array).cloned(),
            _ => None,
        };
        if let Some(children) = nested {
            let label = label_from_items(&children);
            if !label.is_empty() {
                return label;
            }
        }
    }
    String::new()
}

/// If an AC element has a `selectAction`, convert it to a Slack button.
pub(super) fn collect_select_action(element: &Value, actions: &mut Vec<Value>) {
    let sa = match element.get("selectAction") {
        Some(sa) => sa,
        None => return,
    };
    let atype = sa.get("type").and_then(Value::as_str).unwrap_or_default();
    if atype != "Action.Submit" && atype != "Action.Execute" {
        return;
    }

    // Derive button label: try selectAction.title first, then child items, then fallback.
    let sa_title = sa
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let label = if !sa_title.is_empty() {
        sa_title
    } else {
        let items = element
            .get("items")
            .and_then(Value::as_array)
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
        label_from_items(items)
    };
    // Fallback: derive from data keys or use generic label.
    let label = if label.is_empty() {
        sa.get("data")
            .and_then(|d| d.get("routeToCardId").and_then(Value::as_str))
            .unwrap_or("Select")
            .to_string()
    } else {
        label
    };

    let btn_text: String = label.chars().take(75).collect();
    let mut btn = json!({
        "type": "button",
        "text": { "type": "plain_text", "text": btn_text },
        "action_id": format!("ac_action_{}", actions.len())
    });
    if let Some(data) = sa.get("data") {
        btn.as_object_mut()
            .unwrap()
            .insert("value".into(), Value::String(data.to_string()));
    }
    actions.push(btn);
}

/// Collect AC actions into Slack button elements.
pub(super) fn collect_slack_actions(action_list: &[Value], actions: &mut Vec<Value>) {
    for action in action_list {
        let atype = action
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let title = action
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        // Slack button text max 75 chars.
        let btn_text: String = title.chars().take(75).collect();
        match atype {
            "Action.OpenUrl" => {
                let url = action.get("url").and_then(Value::as_str).unwrap_or("");
                actions.push(json!({
                    "type": "button",
                    "text": { "type": "plain_text", "text": btn_text },
                    "url": url,
                    "action_id": format!("ac_url_{}", actions.len())
                }));
            }
            _ => {
                // Action.Submit, Action.Execute, etc. → callback button.
                // Include AC action data (routeToCardId, cardId, etc.) in button value.
                let mut btn = json!({
                    "type": "button",
                    "text": { "type": "plain_text", "text": btn_text },
                    "action_id": format!("ac_action_{}", actions.len())
                });
                if let Some(data) = action.get("data") {
                    btn.as_object_mut()
                        .unwrap()
                        .insert("value".into(), Value::String(data.to_string()));
                }
                actions.push(btn);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_from_items_prefers_bold_then_nested_text() {
        let items = vec![
            json!({"type": "TextBlock", "text": "regular"}),
            json!({"type": "TextBlock", "text": "Important", "weight": "Bolder"}),
        ];
        assert_eq!(label_from_items(&items), "Important");

        let nested = vec![json!({
            "type": "Container",
            "items": [{"type": "TextBlock", "text": "Nested label"}]
        })];
        assert_eq!(label_from_items(&nested), "Nested label");
    }

    #[test]
    fn collect_select_action_uses_route_fallback_and_data_value() {
        let element = json!({
            "type": "Container",
            "items": [],
            "selectAction": {
                "type": "Action.Submit",
                "data": {"routeToCardId": "details", "value": 7}
            }
        });
        let mut actions = Vec::new();

        collect_select_action(&element, &mut actions);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["text"]["text"], "details");
        assert_eq!(actions[0]["action_id"], "ac_action_0");
        assert!(
            actions[0]["value"]
                .as_str()
                .unwrap_or("")
                .contains("routeToCardId")
        );
    }

    #[test]
    fn collect_slack_actions_handles_url_and_submit_buttons() {
        let mut actions = Vec::new();
        let action_list = vec![
            json!({"type": "Action.OpenUrl", "title": "Open", "url": "https://example.com"}),
            json!({"type": "Action.Submit", "title": "Save", "data": {"id": "save"}}),
            json!({"type": "Action.Submit"}),
        ];

        collect_slack_actions(&action_list, &mut actions);

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["url"], "https://example.com");
        assert_eq!(actions[0]["action_id"], "ac_url_0");
        assert_eq!(actions[1]["text"]["text"], "Save");
        assert!(actions[1].get("value").is_some());
    }
}
