//! Contract tests for `channelData.rag`.
//!
//! The provider carries the field through opaquely, so the schema is the only
//! place the shape is written down. These tests keep it honest.

use std::fs;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn schema() -> Result<Value> {
    let path = workspace_root()
        .join("schemas")
        .join("messaging")
        .join("channel-data")
        .join("rag.v1.json");
    Ok(serde_json::from_str(&fs::read_to_string(&path)?)?)
}

fn validator() -> Result<jsonschema::Validator> {
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema()?)
        .map_err(|e| anyhow!("rag.v1.json is not a valid JSON Schema: {e}"))
}

// The cap lives in the component crate, which this one cannot depend on. Read it
// from its one definition so a tightened cap fails here instead of silently
// leaving the schema promising more than the wire accepts.
fn channel_data_cap_bytes() -> Result<usize> {
    let source = fs::read_to_string(
        workspace_root()
            .join("components")
            .join("messaging-provider-webchat")
            .join("src")
            .join("directline")
            .join("http.rs"),
    )?;
    source
        .lines()
        .find_map(|line| {
            let kib: usize = line
                .trim()
                .strip_prefix("const MAX_CHANNEL_DATA_BYTES: usize = ")?
                .strip_suffix(" * 1024;")?
                .parse()
                .ok()?;
            Some(kib * 1024)
        })
        .ok_or_else(|| {
            anyhow!("MAX_CHANNEL_DATA_BYTES not found in directline/http.rs — this test's cap would be a guess")
        })
}

#[test]
fn every_documented_example_validates() -> Result<()> {
    let schema = schema()?;
    let validator = validator()?;
    let examples = schema
        .get("examples")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("rag.v1.json must document at least one example"))?;
    assert!(!examples.is_empty(), "examples must not be empty");
    for (i, example) in examples.iter().enumerate() {
        if let Err(err) = validator.validate(example) {
            return Err(anyhow!(
                "examples[{i}] does not satisfy its own schema: {err}"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_citation_set_at_every_limit_stays_inside_the_channel_data_cap() -> Result<()> {
    // The provider rejects oversized channelData with a 400 rather than truncating,
    // so a payload the schema accepts must never be one the wire refuses.
    // `rag` is one key among several, so leave room for the siblings beside it.
    const SIBLING_HEADROOM_BYTES: usize = 2 * 1024;
    let cap = channel_data_cap_bytes()?;

    let schema = schema()?;
    let items = &schema["properties"]["citations"];
    let max_items = items["maxItems"].as_u64().expect("maxItems") as usize;
    let props = &items["items"]["properties"];
    let max_len = |field: &str| props[field]["maxLength"].as_u64().expect("maxLength") as usize;
    let model_max_len = schema["properties"]["model"]["maxLength"]
        .as_u64()
        .expect("model maxLength") as usize;

    // maxLength counts code points; the provider counts UTF-8 bytes. A Latin-only
    // fixture is the cheapest case the schema allows and certifies nothing for a
    // deployment answering in Japanese.
    let wide = |field: &str| "設".repeat(max_len(field));
    let citation = json!({
        "index": 9999,
        "doc": wide("doc"),
        "source_file": wide("source_file"),
        "source_url": format!("https://example.com/{}", "u".repeat(max_len("source_url") - 20)),
        "source_type": "tool",
        "page": 9999,
        "section": wide("section"),
        "excerpt": wide("excerpt"),
        "text": wide("text"),
        "relevance_score": 0.5,
    });
    let worst_case = json!({
        "type": "rag",
        "confidence": 0.5,
        "model": "設".repeat(model_max_len),
        "search_time_ms": 999_999,
        "llm_time_ms": 999_999,
        "citations": vec![citation; max_items],
    });

    validator()?
        .validate(&worst_case)
        .map_err(|e| anyhow!("the worst case the schema allows must validate: {e}"))?;

    let encoded = serde_json::to_string(&json!({ "rag": worst_case }))?.len();
    assert!(
        encoded + SIBLING_HEADROOM_BYTES <= cap,
        "a maximal citation set serialises to {encoded} bytes of UTF-8; with \
         {SIBLING_HEADROOM_BYTES} reserved for the other channelData keys that ride \
         alongside it, that is over the {cap} byte cap the provider enforces — lower \
         maxItems or citations[].text.maxLength"
    );
    Ok(())
}

#[test]
fn unknown_fields_are_rejected() -> Result<()> {
    let validator = validator()?;
    let with_stray_root_key = json!({
        "type": "rag",
        "citations": [{"index": 1, "doc": "Runbook"}],
        "tool_id": "epnm",
    });
    assert!(
        validator.validate(&with_stray_root_key).is_err(),
        "a new top-level key must go through a schema bump, not ride along unannounced"
    );

    let with_stray_citation_key = json!({
        "type": "rag",
        "citations": [{"index": 1, "doc": "Runbook", "snippet": "..."}],
    });
    assert!(
        validator.validate(&with_stray_citation_key).is_err(),
        "`snippet` was this repo's old fixture name; the wire name is `excerpt`"
    );
    Ok(())
}

#[test]
fn a_citation_without_a_document_is_rejected() -> Result<()> {
    let validator = validator()?;
    for bad in [
        json!({"type": "rag", "citations": [{"doc": "Runbook"}]}),
        json!({"type": "rag", "citations": [{"index": 1}]}),
        json!({"type": "rag", "citations": [{"index": 0, "doc": "Runbook"}]}),
        json!({"type": "rag", "citations": [{"index": 1, "doc": ""}]}),
    ] {
        assert!(
            validator.validate(&bad).is_err(),
            "a citation the UI cannot render a pill for must not validate: {bad}"
        );
    }
    Ok(())
}

#[test]
fn the_discriminator_and_enum_are_pinned() -> Result<()> {
    let validator = validator()?;
    let citations = json!([{"index": 1, "doc": "Runbook"}]);
    assert!(
        validator
            .validate(&json!({"citations": citations}))
            .is_err(),
        "`type` must stay required — an untyped blob is not a rag payload"
    );
    assert!(
        validator
            .validate(&json!({"type": "tool", "citations": citations}))
            .is_err(),
        "`type: tool` is the shape this schema replaced; it must not validate again"
    );
    assert!(
        validator
            .validate(&json!({"type": "rag", "citations": [
                {"index": 1, "doc": "Runbook", "source_type": "sharepoint"}
            ]}))
            .is_err(),
        "source_type is a closed enum"
    );
    assert!(
        validator
            .validate(&json!({"type": "rag", "citations": [
                {"index": 1, "doc": "Runbook", "source_url": "javascript:alert(1)"}
            ]}))
            .is_err(),
        "a citation URL the UI turns into a link must be http(s)"
    );
    assert!(
        validator
            .validate(&json!({"type": "rag", "citations": []}))
            .is_ok(),
        "an answer that retrieved nothing is still a rag answer"
    );
    Ok(())
}
