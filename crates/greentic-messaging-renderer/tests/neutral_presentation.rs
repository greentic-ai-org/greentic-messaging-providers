use greentic_messaging_renderer::{
    AdaptivePresentationModel, PlannerCapabilities, RenderItem, RenderTier,
    adaptive_card_from_presentation, render_plan_from_presentation,
};

fn sample_port_utilisation_presentation() -> AdaptivePresentationModel {
    serde_json::from_value(serde_json::json!({
      "playbook_id": "tx.playbook.port_utilisation",
      "result": "success",
      "summary": "Found 1 ports with peak utilisation at or above 85.0%",
      "severity": "warning",
      "sections": [
        {
          "section_id": "summary",
          "section_type": "facts",
          "title": "Port utilisation summary",
          "items": [
            { "label": "device_id", "value": "aci-p1-n2201" },
            { "label": "threshold_percent", "value": 85.0 },
            { "label": "source_system", "value": "apic" }
          ]
        },
        {
          "section_id": "affected_ports",
          "section_type": "list",
          "title": "Overutilised ports",
          "items": [
            {
              "interface": "aci-p1-n2201:eth1/2",
              "peak_percent": 92.5,
              "timestamp": "2026-03-07T11:00:00Z"
            }
          ]
        },
        {
          "section_id": "ranking",
          "section_type": "table",
          "title": "Overutilised port ranking",
          "columns": ["interface", "peak_percent", "timestamp"],
          "rows": [
            {
              "interface": "aci-p1-n2201:eth1/2",
              "peak_percent": 92.5,
              "timestamp": "2026-03-07T11:00:00Z"
            }
          ]
        }
      ],
      "recommended_actions": [
        "Inspect the affected interface for congestion or traffic shifts.",
        "Check recent changes on the device before escalating."
      ]
    }))
    .expect("sample presentation should parse")
}

#[test]
fn presentation_maps_to_adaptive_card() {
    let presentation = sample_port_utilisation_presentation();
    let card = adaptive_card_from_presentation(&presentation);

    assert_eq!(card["type"], "AdaptiveCard");
    assert_eq!(card["body"][0]["text"], "port utilisation");
    assert_eq!(
        card["body"][1]["text"],
        "Found 1 ports with peak utilisation at or above 85.0%"
    );
    assert_eq!(card["body"][3]["text"], "Port utilisation summary");
}

#[test]
fn presentation_maps_to_tier_a_render_plan() {
    let presentation = sample_port_utilisation_presentation();
    let capabilities = PlannerCapabilities {
        supports_adaptive_cards: true,
        supports_buttons: true,
        supports_images: true,
        supports_markdown: true,
        supports_html: true,
        max_text_len: None,
        max_payload_bytes: None,
    };

    let plan = render_plan_from_presentation(&presentation, &capabilities);

    assert_eq!(plan.tier, RenderTier::TierA);
    assert!(
        plan.summary_text
            .as_ref()
            .is_some_and(|text| text.contains("Found 1 ports"))
    );
    assert!(
        plan.items
            .iter()
            .any(|item| matches!(item, RenderItem::AdaptiveCard(_)))
    );
    assert!(
        plan.items
            .iter()
            .any(|item| matches!(item, RenderItem::Text(_)))
    );
}
