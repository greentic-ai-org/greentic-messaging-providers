use crate::{
    ac_extract::extract_planner_card,
    errors::RendererError,
    plan::RenderPlan,
    planner::{PlannerCapabilities, plan_render},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptivePresentationModel {
    pub playbook_id: String,
    pub result: String,
    pub summary: String,
    pub severity: String,
    #[serde(default)]
    pub sections: Vec<AdaptivePresentationSection>,
    #[serde(default)]
    pub recommended_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptivePresentationSection {
    pub section_id: String,
    pub section_type: String,
    pub title: String,
    #[serde(default)]
    pub items: Vec<Value>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub rows: Vec<Value>,
}

pub fn render_plan_from_presentation(
    model: &AdaptivePresentationModel,
    capabilities: &PlannerCapabilities,
) -> RenderPlan {
    let adaptive_card = adaptive_card_from_presentation(model);
    let planner_card = extract_planner_card(&adaptive_card);
    plan_render(&planner_card, capabilities, Some(&adaptive_card))
}

pub fn adaptive_card_from_presentation(model: &AdaptivePresentationModel) -> Value {
    let mut body = vec![
        title_block(&human_title(&model.playbook_id)),
        summary_block(&model.summary),
        severity_factset(&model.severity),
    ];

    for section in &model.sections {
        body.extend(render_section(section));
    }

    if !model.recommended_actions.is_empty() {
        body.push(section_title("Recommended actions"));
        for action in &model.recommended_actions {
            body.push(json!({
                "type": "TextBlock",
                "wrap": true,
                "spacing": "Small",
                "text": format!("- {action}"),
            }));
        }
    }

    json!({
        "type": "AdaptiveCard",
        "version": "1.5",
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "body": body,
    })
}

fn render_section(section: &AdaptivePresentationSection) -> Vec<Value> {
    let mut blocks = vec![section_title(&section.title)];
    match section.section_type.as_str() {
        "facts" => {
            let facts = section
                .items
                .iter()
                .map(|item| {
                    json!({
                        "title": value_to_text(&item["label"]),
                        "value": value_to_text(&item["value"]),
                    })
                })
                .collect::<Vec<_>>();
            blocks.push(json!({
                "type": "FactSet",
                "facts": facts,
            }));
        }
        "list" => {
            for item in &section.items {
                blocks.push(json!({
                    "type": "TextBlock",
                    "wrap": true,
                    "spacing": "Small",
                    "text": object_to_line(item),
                }));
            }
        }
        "table" => {
            if !section.columns.is_empty() {
                blocks.push(json!({
                    "type": "TextBlock",
                    "wrap": true,
                    "spacing": "Small",
                    "weight": "Bolder",
                    "text": section.columns.join(" | "),
                }));
            }
            for row in &section.rows {
                let text = section
                    .columns
                    .iter()
                    .map(|column| format!("{column}: {}", value_to_text(&row[column])))
                    .collect::<Vec<_>>()
                    .join(" | ");
                blocks.push(json!({
                    "type": "TextBlock",
                    "wrap": true,
                    "spacing": "Small",
                    "text": text,
                }));
            }
        }
        "timeline" => {
            for item in &section.items {
                blocks.push(json!({
                    "type": "TextBlock",
                    "wrap": true,
                    "spacing": "Small",
                    "text": object_to_line(item),
                }));
            }
        }
        "narrative" => {
            for item in &section.items {
                blocks.push(json!({
                    "type": "TextBlock",
                    "wrap": true,
                    "spacing": "Small",
                    "text": narrative_item_to_text(item),
                }));
            }
        }
        _ => {
            blocks.push(json!({
                "type": "TextBlock",
                "wrap": true,
                "spacing": "Small",
                "text": format!("Unsupported section type: {}", section.section_type),
            }));
        }
    }
    blocks
}

fn human_title(playbook_id: &str) -> String {
    playbook_id
        .rsplit('.')
        .next()
        .unwrap_or(playbook_id)
        .replace('_', " ")
}

fn title_block(text: &str) -> Value {
    json!({
        "type": "TextBlock",
        "size": "Medium",
        "weight": "Bolder",
        "wrap": true,
        "text": text,
    })
}

fn section_title(text: &str) -> Value {
    json!({
        "type": "TextBlock",
        "weight": "Bolder",
        "spacing": "Medium",
        "wrap": true,
        "text": text,
    })
}

fn summary_block(text: &str) -> Value {
    json!({
        "type": "TextBlock",
        "wrap": true,
        "spacing": "Small",
        "text": text,
    })
}

fn severity_factset(severity: &str) -> Value {
    json!({
        "type": "FactSet",
        "facts": [
            {
                "title": "severity",
                "value": severity,
            }
        ],
    })
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::Null => "-".to_owned(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.clone(),
        Value::Array(v) => v.iter().map(value_to_text).collect::<Vec<_>>().join(", "),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_else(|_| "<object>".to_owned()),
    }
}

fn object_to_line(value: &Value) -> String {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, item)| format!("{key}: {}", value_to_text(item)))
            .collect::<Vec<_>>()
            .join(" | "),
        _ => value_to_text(value),
    }
}

fn narrative_item_to_text(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some("narrative")
                && let Some(text) = map.get("text")
            {
                return value_to_text(text);
            }
            object_to_line(value)
        }
        _ => value_to_text(value),
    }
}

pub fn parse_presentation(value: &Value) -> Result<AdaptivePresentationModel, RendererError> {
    serde_json::from_value(value.clone())
        .map_err(|err| RendererError(format!("failed to parse presentation model: {err}")))
}
