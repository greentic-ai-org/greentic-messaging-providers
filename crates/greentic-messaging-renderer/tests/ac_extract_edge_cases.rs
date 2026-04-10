//! Edge-case and property-style integration tests for `extract_planner_card`.
//!
//! These tests live outside the `src/` unit tests to exercise the public
//! surface the way providers consume it, and to cover adversarial or
//! malformed Adaptive Card inputs that must not panic, hang, or exhaust
//! the stack.

use greentic_messaging_renderer::extract_planner_card;
use serde_json::{Value, json};

/// Build an AdaptiveCard whose body is a single `Container` chain nested
/// `depth` levels deep, with a `TextBlock` at the innermost layer.
fn nested_card(depth: usize) -> Value {
    let mut current = json!({"type": "TextBlock", "text": "leaf"});
    for _ in 0..depth {
        current = json!({
            "type": "Container",
            "items": [current]
        });
    }
    json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [current]
    })
}

/// Iteratively flatten a deeply nested `Container`/`items` chain so that
/// `Drop` on the resulting `Value` is O(1) in stack depth. Without this,
/// `serde_json::Value`'s recursive `Drop` — not our extractor — can blow
/// the test thread's stack on adversarially nested inputs.
fn flatten_in_place(mut value: Value) {
    // Walk down the body -> container -> items chain, moving each
    // inner `items` array out and replacing it so no single `Value` owns
    // a long recursive chain at once.
    loop {
        let next = value
            .get_mut("body")
            .and_then(Value::as_array_mut)
            .and_then(|arr| arr.get_mut(0))
            .and_then(|v| v.get_mut("items"))
            .and_then(Value::as_array_mut)
            .and_then(|arr| arr.get_mut(0))
            .map(std::mem::take);
        match next {
            Some(inner) if inner.is_object() => {
                // Pivot: wrap the inner object as the new "root" we drain.
                value = json!({"body": [inner]});
            }
            _ => break,
        }
    }
    drop(value);
}

#[test]
fn deeply_nested_containers_do_not_panic_or_hang() {
    // 100 levels is well beyond the internal MAX_AC_DEPTH (32). The walker
    // must silently drop content past the limit without panicking or hanging.
    let ac = nested_card(100);
    let card = extract_planner_card(&ac);
    // Leaf is unreachable past the depth limit.
    assert!(card.text.is_none());
    assert!(card.title.is_none());
    assert!(card.actions.is_empty());
    assert!(card.images.is_empty());
    flatten_in_place(ac);
}

#[test]
fn adversarially_deep_nesting_is_safe() {
    // 500 levels — far past MAX_AC_DEPTH. Our extractor must be bounded.
    // We keep the depth modest enough that `serde_json::Value::drop` (which
    // is itself recursive and not something we own) does not blow the stack
    // on a default 2 MiB test thread.
    let ac = nested_card(500);
    let card = extract_planner_card(&ac);
    assert!(card.text.is_none());
    flatten_in_place(ac);
}

#[test]
fn malformed_card_missing_body_does_not_panic() {
    let ac = json!({"type": "AdaptiveCard"});
    let card = extract_planner_card(&ac);
    assert!(card.title.is_none());
    assert!(card.text.is_none());
    assert!(card.actions.is_empty());
    assert!(card.images.is_empty());
}

#[test]
fn empty_body_array_does_not_panic() {
    let ac = json!({
        "type": "AdaptiveCard",
        "body": []
    });
    let card = extract_planner_card(&ac);
    assert!(card.text.is_none());
    assert!(card.actions.is_empty());
    assert!(card.images.is_empty());
}

#[test]
fn element_missing_type_field_is_skipped() {
    let ac = json!({
        "type": "AdaptiveCard",
        "body": [
            {"text": "no type field here"},
            {"type": "TextBlock", "text": "keeper"}
        ]
    });
    let card = extract_planner_card(&ac);
    // The untyped element is skipped; the well-formed one survives.
    assert_eq!(card.text.as_deref(), Some("keeper"));
}

#[test]
fn columnset_with_empty_columns_does_not_panic() {
    let ac = json!({
        "type": "AdaptiveCard",
        "body": [
            {"type": "ColumnSet", "columns": []}
        ]
    });
    let card = extract_planner_card(&ac);
    assert!(card.text.is_none());
}

#[test]
fn columnset_with_columns_lacking_items_does_not_panic() {
    let ac = json!({
        "type": "AdaptiveCard",
        "body": [
            {"type": "ColumnSet", "columns": [
                {},
                {"items": []},
                {"items": [{"type": "TextBlock", "text": "only me"}]}
            ]}
        ]
    });
    let card = extract_planner_card(&ac);
    assert_eq!(card.text.as_deref(), Some("only me"));
}

#[test]
fn factset_with_empty_facts_does_not_panic() {
    let ac = json!({
        "type": "AdaptiveCard",
        "body": [
            {"type": "FactSet", "facts": []}
        ]
    });
    let card = extract_planner_card(&ac);
    assert!(card.text.is_none());
}

#[test]
fn factset_with_blank_fact_entries_is_skipped() {
    let ac = json!({
        "type": "AdaptiveCard",
        "body": [
            {"type": "FactSet", "facts": [
                {"title": "", "value": ""},
                {"title": "Name", "value": "Ada"}
            ]}
        ]
    });
    let card = extract_planner_card(&ac);
    let text = card.text.expect("has text");
    assert!(text.contains("Name: Ada"));
    // Blank entry should not emit a ": " line.
    assert!(!text.contains("\n: \n"));
}

#[test]
fn mixed_elements_all_extracted() {
    let ac = json!({
        "type": "AdaptiveCard",
        "version": "1.6",
        "body": [
            {"type": "TextBlock", "text": "Big Title", "size": "Large"},
            {"type": "TextBlock", "text": "Intro body"},
            {"type": "Image", "url": "https://example.com/a.png"},
            {
                "type": "Container",
                "items": [
                    {"type": "TextBlock", "text": "Nested line"},
                    {
                        "type": "ColumnSet",
                        "columns": [
                            {"items": [
                                {"type": "Image", "url": "https://example.com/b.png"}
                            ]},
                            {"items": [
                                {"type": "TextBlock", "text": "Col 2 body"}
                            ]}
                        ]
                    }
                ]
            },
            {
                "type": "ActionSet",
                "actions": [
                    {"type": "Action.OpenUrl", "title": "Go", "url": "https://example.com/go"}
                ]
            }
        ]
    });
    let card = extract_planner_card(&ac);

    assert_eq!(card.title.as_deref(), Some("Big Title"));
    let text = card.text.expect("has text");
    assert!(text.contains("Intro body"));
    assert!(text.contains("Nested line"));
    assert!(text.contains("Col 2 body"));

    assert_eq!(card.images.len(), 2);
    assert!(card.images.iter().any(|u| u == "https://example.com/a.png"));
    assert!(card.images.iter().any(|u| u == "https://example.com/b.png"));

    assert_eq!(card.actions.len(), 1);
    assert_eq!(card.actions[0].title, "Go");
    assert_eq!(
        card.actions[0].url.as_deref(),
        Some("https://example.com/go")
    );
}

#[test]
fn action_openurl_without_url_field_is_still_kept_without_crash() {
    // Action has a title so it is kept, but the url should be `None`.
    // The walker must not crash on the missing `url` key.
    let ac = json!({
        "type": "AdaptiveCard",
        "body": [],
        "actions": [
            {"type": "Action.OpenUrl", "title": "Broken"},
            {"type": "Action.OpenUrl", "title": "Working", "url": "https://example.com"}
        ]
    });
    let card = extract_planner_card(&ac);
    assert_eq!(card.actions.len(), 2);
    assert_eq!(card.actions[0].title, "Broken");
    assert!(card.actions[0].url.is_none());
    assert_eq!(card.actions[1].title, "Working");
    assert_eq!(card.actions[1].url.as_deref(), Some("https://example.com"));
}

#[test]
fn action_without_title_is_dropped() {
    let ac = json!({
        "type": "AdaptiveCard",
        "body": [],
        "actions": [
            {"type": "Action.OpenUrl", "url": "https://example.com"},
            {"type": "Action.Submit"}
        ]
    });
    let card = extract_planner_card(&ac);
    assert!(card.actions.is_empty());
}
