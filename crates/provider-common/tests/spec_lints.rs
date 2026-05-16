use std::fs;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use glob::glob;
use serde_yaml_bw::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn all_packs_have_setup_spec() -> Result<()> {
    let root = workspace_root();
    let packs_dir = root.join("packs");
    for entry in fs::read_dir(&packs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let spec = path.join("assets").join("setup.yaml");
        if !spec.exists() {
            return Err(anyhow!("missing setup spec at {}", spec.display()));
        }
    }
    Ok(())
}

#[test]
fn specs_parse() -> Result<()> {
    let root = workspace_root();
    let pattern = root.join("packs").join("*/assets/setup.yaml");
    let mut found = false;
    for entry in glob(pattern.to_str().unwrap())? {
        let path = entry?;
        let contents = fs::read_to_string(&path)?;
        let _: Value = serde_yaml_bw::from_str(&contents)?;
        found = true;
    }
    if !found {
        return Err(anyhow!("no setup.yaml files found under packs/"));
    }
    Ok(())
}

#[test]
fn secret_questions_not_inlined_in_titles() -> Result<()> {
    let root = workspace_root();
    let pattern = root.join("packs").join("*/assets/setup.yaml");
    for entry in glob(pattern.to_str().unwrap())? {
        let path = entry?;
        let contents = fs::read_to_string(&path)?;
        let value: Value = serde_yaml_bw::from_str(&contents)?;
        let questions = value
            .get("questions")
            .and_then(Value::as_sequence)
            .cloned()
            .unwrap_or_default();
        for question in questions {
            let secret = question
                .get("secret")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !secret {
                continue;
            }
            let title = question.get("title").and_then(Value::as_str).unwrap_or("");
            if title.to_ascii_lowercase().contains("token:") {
                return Err(anyhow!(
                    "secret question title should not include token value hints in {}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn webchat_gui_setup_has_presentation_mode_and_standalone_nav_links() -> Result<()> {
    let root = workspace_root();
    let setup_path = root
        .join("packs")
        .join("messaging-webchat-gui")
        .join("assets")
        .join("setup.yaml");
    let value: Value = serde_yaml_bw::from_str(&fs::read_to_string(&setup_path)?)?;
    let questions = value
        .get("questions")
        .and_then(Value::as_sequence)
        .ok_or_else(|| anyhow!("webchat-gui setup.yaml missing questions"))?;

    let presentation_mode = questions
        .iter()
        .find(|question| {
            question
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name == "presentation_mode")
        })
        .ok_or_else(|| anyhow!("webchat-gui setup.yaml missing presentation_mode"))?;
    assert_eq!(
        presentation_mode.get("default").and_then(Value::as_str),
        Some("standalone")
    );
    assert_eq!(
        presentation_mode.get("group").and_then(Value::as_str),
        Some("Branding")
    );

    let text_input = questions
        .iter()
        .find(|question| {
            question
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name == "text_input_enabled")
        })
        .ok_or_else(|| anyhow!("webchat-gui setup.yaml missing text_input_enabled"))?;
    assert_eq!(
        text_input.get("default").and_then(Value::as_str),
        Some("true")
    );
    assert_eq!(
        text_input
            .get("visible_if")
            .and_then(|visible| visible.get("field"))
            .and_then(Value::as_str),
        Some("presentation_mode")
    );
    assert_eq!(
        text_input
            .get("visible_if")
            .and_then(|visible| visible.get("eq"))
            .and_then(Value::as_str),
        Some("embed_webcomponent")
    );

    let nav_links = questions
        .iter()
        .find(|question| {
            question
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name == "nav_links")
        })
        .ok_or_else(|| anyhow!("webchat-gui setup.yaml missing nav_links"))?;
    assert_eq!(
        nav_links
            .get("visible_if")
            .and_then(|visible| visible.get("field"))
            .and_then(Value::as_str),
        Some("presentation_mode")
    );
    assert_eq!(
        nav_links
            .get("visible_if")
            .and_then(|visible| visible.get("eq"))
            .and_then(Value::as_str),
        Some("standalone")
    );

    Ok(())
}
