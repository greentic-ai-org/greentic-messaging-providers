use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn build_answers_match_committed_manifests() -> Result<()> {
    for pack in ["messaging-email", "messaging-microsoft-email"] {
        assert_build_answer_matches_committed_manifest(pack)?;
    }
    Ok(())
}

#[test]
fn email_split_has_distinct_legacy_and_microsoft_contracts() -> Result<()> {
    let root = workspace_root();
    let legacy: Value = serde_json::from_slice(
        &fs::read(root.join("packs/messaging-email/build-answer.json"))
            .context("reading legacy email build answer")?,
    )?;
    let microsoft: Value = serde_json::from_slice(
        &fs::read(root.join("packs/messaging-microsoft-email/build-answer.json"))
            .context("reading microsoft email build answer")?,
    )?;

    assert_eq!(
        legacy
            .pointer("/extensions/provider/provider_type")
            .and_then(Value::as_str),
        Some("messaging.email.smtp")
    );
    assert_eq!(
        microsoft
            .pointer("/extensions/provider/provider_type")
            .and_then(Value::as_str),
        Some("messaging.email.microsoft_graph")
    );
    assert!(
        !legacy
            .pointer("/setup/fields")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
            .iter()
            .any(|field| field
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.starts_with("ms_graph") || name == "graph_tenant_id")),
        "legacy SMTP setup must not collect Microsoft Graph fields"
    );
    assert!(
        microsoft
            .pointer("/setup/fields")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
            .iter()
            .any(|field| field
                .get("secret_key")
                .and_then(Value::as_str)
                .is_some_and(|key| key == "MS_GRAPH_REFRESH_TOKEN")),
        "Microsoft provider setup must own Microsoft Graph runtime secrets"
    );

    Ok(())
}

fn assert_build_answer_matches_committed_manifest(pack: &str) -> Result<()> {
    let root = workspace_root();
    let pack_dir = root.join("packs").join(pack);
    let answer: Value = serde_json::from_slice(
        &fs::read(pack_dir.join("build-answer.json"))
            .with_context(|| format!("reading {pack} build answer"))?,
    )
    .with_context(|| format!("parsing {pack} build answer"))?;
    let manifest: Value = serde_json::from_slice(
        &fs::read(pack_dir.join("pack.manifest.json"))
            .with_context(|| format!("reading {pack} manifest"))?,
    )
    .with_context(|| format!("parsing {pack} manifest"))?;

    assert_eq!(
        answer.get("schema_id").and_then(Value::as_str),
        Some("greentic-messaging-provider.build-answer")
    );
    assert_eq!(
        answer.pointer("/provider/id").and_then(Value::as_str),
        manifest.get("name").and_then(Value::as_str)
    );
    assert_eq!(
        answer.pointer("/provider/version").and_then(Value::as_str),
        manifest.get("version").and_then(Value::as_str)
    );
    assert_eq!(
        answer
            .pointer("/provider/description")
            .and_then(Value::as_str),
        manifest.get("description").and_then(Value::as_str)
    );

    let answer_components = answer
        .get("components")
        .and_then(Value::as_array)
        .context("answer components")?;
    let manifest_sources = manifest
        .get("component_sources")
        .and_then(Value::as_array)
        .context("manifest component_sources")?;
    assert_eq!(answer_components.len(), manifest_sources.len());
    for (answer_component, manifest_component) in
        answer_components.iter().zip(manifest_sources.iter())
    {
        assert_eq!(
            answer_component.get("id"),
            manifest_component.get("id"),
            "component id drift"
        );
        assert_eq!(
            answer_component.get("wasm"),
            manifest_component.get("wasm"),
            "component wasm drift"
        );
        assert_eq!(
            answer_component.get("manifest"),
            manifest_component.get("manifest"),
            "component manifest drift"
        );
        assert_eq!(
            answer_component.get("capabilities"),
            manifest_component.get("capabilities"),
            "component capabilities drift"
        );
    }

    assert_eq!(
        answer.pointer("/extensions/provider/provider_type"),
        manifest
            .pointer("/extensions/greentic.provider-extension.v1/inline/providers/0/provider_type"),
        "provider type drift"
    );
    assert_eq!(
        answer.pointer("/extensions/provider/ops"),
        manifest.pointer("/extensions/greentic.provider-extension.v1/inline/providers/0/ops"),
        "provider ops drift"
    );
    assert_eq!(
        answer.get("secret_requirements"),
        manifest.get("secret_requirements"),
        "secret requirements drift"
    );

    Ok(())
}
