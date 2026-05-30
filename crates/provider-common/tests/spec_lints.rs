use std::fs;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use glob::glob;
use serde_json::Value as JsonValue;
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
fn slack_registration_outputs_runtime_app_id_key() -> Result<()> {
    let root = workspace_root();
    let setup_path = root
        .join("packs")
        .join("messaging-slack")
        .join("assets")
        .join("setup.yaml");
    let value: Value = serde_yaml_bw::from_str(&fs::read_to_string(&setup_path)?)?;
    let setup_actions = value
        .get("setup_actions")
        .and_then(Value::as_sequence)
        .ok_or_else(|| anyhow!("slack setup.yaml missing setup_actions"))?;
    let registration = setup_actions
        .iter()
        .find_map(|action| action.get("registration"))
        .ok_or_else(|| anyhow!("slack setup.yaml missing registration action"))?;

    assert_eq!(
        registration.get("app_id_output").and_then(Value::as_str),
        Some("slack_app_id"),
        "Slack setup must persist the app id under the key setup_webhook reads at startup"
    );

    Ok(())
}

#[test]
fn provider_setup_contract_declares_runtime_secret_mappings() -> Result<()> {
    let root = workspace_root();
    let expected = [
        (
            "messaging-telegram",
            vec![("telegram_bot_token", "TELEGRAM_BOT_TOKEN")],
        ),
        (
            "messaging-slack",
            vec![
                ("bot_token", "SLACK_BOT_TOKEN"),
                (
                    "slack_configuration_access_token",
                    "SLACK_CONFIGURATION_ACCESS_TOKEN",
                ),
                (
                    "slack_configuration_refresh_token",
                    "SLACK_CONFIGURATION_REFRESH_TOKEN",
                ),
            ],
        ),
        (
            "messaging-whatsapp",
            vec![
                ("whatsapp_token", "WHATSAPP_TOKEN"),
                ("whatsapp_verify_token", "WHATSAPP_VERIFY_TOKEN"),
            ],
        ),
    ];

    for (pack, mappings) in expected {
        let manifest_path = root.join("packs").join(pack).join("pack.manifest.json");
        let manifest: JsonValue = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
        let providers = manifest
            .get("extensions")
            .and_then(|extensions| extensions.get("greentic.provider-extension.v1"))
            .and_then(|extension| extension.get("inline"))
            .and_then(|inline| inline.get("providers"))
            .and_then(JsonValue::as_array)
            .ok_or_else(|| anyhow!("{pack} manifest missing provider extension"))?;
        let contract = providers
            .first()
            .and_then(|provider| provider.get("setup_contract"))
            .ok_or_else(|| anyhow!("{pack} provider missing setup_contract"))?;
        assert_eq!(
            contract.get("version").and_then(JsonValue::as_u64),
            Some(1),
            "{pack} setup_contract must be versioned"
        );
        let secrets_out = contract
            .get("secrets_out")
            .and_then(JsonValue::as_array)
            .ok_or_else(|| anyhow!("{pack} setup_contract missing secrets_out"))?;

        for (answer_key, secret_key) in mappings {
            assert!(
                secrets_out.iter().any(|mapping| {
                    mapping.get("answer_key").and_then(JsonValue::as_str) == Some(answer_key)
                        && mapping.get("secret_key").and_then(JsonValue::as_str) == Some(secret_key)
                }),
                "{pack} setup_contract must map {answer_key} to {secret_key}"
            );
        }
    }

    Ok(())
}

#[test]
fn teams_setup_persists_discovery_labels_and_modes() -> Result<()> {
    let root = workspace_root();
    let setup_path = root
        .join("packs")
        .join("messaging-teams")
        .join("assets")
        .join("setup.yaml");
    let value: Value = serde_yaml_bw::from_str(&fs::read_to_string(&setup_path)?)?;

    let setup_modes = value
        .get("setup_modes")
        .ok_or_else(|| anyhow!("teams setup.yaml missing setup_modes"))?;
    let graph_channel = setup_modes
        .get("graph_channel")
        .ok_or_else(|| anyhow!("Teams setup must describe Graph channel mode"))?;
    assert!(
        setup_modes.get("bot_framework").is_some(),
        "Teams setup must describe Bot Framework mode"
    );
    let provisioning = graph_channel
        .get("provisioning")
        .and_then(|value| value.get("teams_channel"))
        .ok_or_else(|| anyhow!("Teams graph_channel mode missing channel provisioning metadata"))?;
    assert_eq!(
        provisioning.get("op").and_then(Value::as_str),
        Some("apply-answers")
    );
    assert_eq!(
        provisioning.get("mode").and_then(Value::as_str),
        Some("create_if_missing")
    );
    assert_eq!(
        provisioning
            .get("graph")
            .and_then(|graph| graph.get("method"))
            .and_then(Value::as_str),
        Some("POST")
    );
    assert_eq!(
        provisioning
            .get("graph")
            .and_then(|graph| graph.get("resource_template"))
            .and_then(Value::as_str),
        Some("/teams/{team_id}/channels")
    );

    let action = value
        .get("setup_actions")
        .and_then(Value::as_sequence)
        .and_then(|actions| actions.first())
        .ok_or_else(|| anyhow!("teams setup.yaml missing setup action"))?;
    let discovery = action
        .get("post_login_discovery")
        .and_then(Value::as_sequence)
        .ok_or_else(|| anyhow!("teams setup.yaml missing post_login_discovery"))?;
    let joined_teams = discovery
        .iter()
        .find(|step| step.get("id").and_then(Value::as_str) == Some("joined_teams"))
        .ok_or_else(|| anyhow!("teams setup.yaml missing joined_teams discovery"))?;
    assert_eq!(
        joined_teams
            .get("select")
            .and_then(|select| select.get("save_as"))
            .and_then(Value::as_str),
        Some("team_id")
    );
    assert_eq!(
        joined_teams
            .get("select")
            .and_then(|select| select.get("save_label_as"))
            .and_then(Value::as_str),
        Some("team_name")
    );

    let channels = discovery
        .iter()
        .find(|step| step.get("id").and_then(Value::as_str) == Some("channels"))
        .ok_or_else(|| anyhow!("teams setup.yaml missing channels discovery"))?;
    assert_eq!(
        channels
            .get("select")
            .and_then(|select| select.get("save_as"))
            .and_then(Value::as_str),
        Some("channel_id")
    );
    assert_eq!(
        channels
            .get("select")
            .and_then(|select| select.get("save_label_as"))
            .and_then(Value::as_str),
        Some("channel_name")
    );

    let desired_channel_name = value
        .get("questions")
        .and_then(Value::as_sequence)
        .and_then(|questions| {
            questions.iter().find(|question| {
                question.get("name").and_then(Value::as_str) == Some("desired_channel_name")
            })
        })
        .ok_or_else(|| anyhow!("teams setup.yaml missing desired_channel_name question"))?;
    assert_eq!(
        desired_channel_name
            .get("default_from_context")
            .and_then(Value::as_str),
        Some("bundle_name")
    );
    assert!(
        desired_channel_name
            .get("help")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("create this standard channel"),
        "desired_channel_name should describe channel creation behavior"
    );

    let scopes = action
        .get("scopes")
        .and_then(Value::as_sequence)
        .ok_or_else(|| anyhow!("teams setup action missing scopes"))?;
    assert!(
        scopes
            .iter()
            .any(|scope| scope.as_str() == Some("Channel.Create")),
        "Teams setup must request Channel.Create for missing-channel provisioning"
    );

    Ok(())
}

#[test]
fn teams_subscription_manifest_declares_generic_desired_state_metadata() -> Result<()> {
    let root = workspace_root();
    let manifest_path = root
        .join("packs")
        .join("messaging-teams")
        .join("pack.manifest.json");
    let desired_fixture_path = root
        .join("packs")
        .join("messaging-teams")
        .join("fixtures")
        .join("subscriptions.desired-state-metadata.expected.json");
    let component_config_fixture_path = root
        .join("packs")
        .join("messaging-teams")
        .join("fixtures")
        .join("subscriptions.component-config.expected.json");
    let manifest: JsonValue = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
    let expected_desired_state: JsonValue =
        serde_json::from_str(&fs::read_to_string(&desired_fixture_path)?)?;
    let expected_component_config: JsonValue =
        serde_json::from_str(&fs::read_to_string(&component_config_fixture_path)?)?;
    let subscriptions = manifest
        .get("extensions")
        .and_then(|extensions| extensions.get("messaging.subscriptions.v1"))
        .and_then(|extension| extension.get("inline"))
        .ok_or_else(|| anyhow!("Teams manifest missing subscriptions extension"))?;
    let component_config = subscriptions.get("component_config").ok_or_else(|| {
        anyhow!("Teams subscriptions extension missing component_config metadata")
    })?;
    let desired_state = subscriptions
        .get("desired_state")
        .ok_or_else(|| anyhow!("Teams subscriptions extension missing desired_state metadata"))?;

    assert_eq!(component_config, &expected_component_config);
    assert_eq!(desired_state, &expected_desired_state);

    let component_config_fields = component_config
        .get("include")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("Teams component_config missing include list"))?;
    for field in [
        "tenant_id",
        "client_id",
        "graph_base_url",
        "auth_base_url",
        "token_scope",
    ] {
        assert!(
            component_config_fields
                .iter()
                .any(|value| value.as_str() == Some(field)),
            "Teams component_config must include {field}"
        );
    }
    for field in [
        "team_id",
        "channel_id",
        "chat_id",
        "channel_name",
        "team_name",
    ] {
        assert!(
            !component_config_fields
                .iter()
                .any(|value| value.as_str() == Some(field)),
            "Teams component_config must not pass desired-state field {field}"
        );
    }

    let source_keys = desired_state
        .get("source_keys")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("Teams desired_state missing source_keys"))?;
    for field in ["team_id", "channel_id"] {
        assert!(
            source_keys
                .iter()
                .any(|value| value.as_str() == Some(field)),
            "Teams desired_state must keep {field} available for templating"
        );
    }
    assert_eq!(
        desired_state
            .get("notification_url")
            .and_then(|value| value.get("template"))
            .and_then(JsonValue::as_str),
        Some("{public_base_url}/v1/messaging/ingress/{provider_id}/{tenant}/{team}")
    );
    let templates = desired_state
        .get("templates")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("Teams desired_state missing templates"))?;
    assert!(
        templates.iter().any(|template| {
            template
                .get("resource_template")
                .and_then(JsonValue::as_str)
                == Some("/teams/{team_id}/channels/{channel_id}/messages")
        }),
        "Teams desired_state must declare channel message resource format"
    );
    assert!(
        templates.iter().any(|template| {
            template
                .get("resource_template")
                .and_then(JsonValue::as_str)
                == Some("/chats/{chat_id}/messages")
        }),
        "Teams desired_state must declare chat message resource format"
    );

    Ok(())
}

#[test]
fn teams_pack_manifest_declares_channel_provisioning_contract() -> Result<()> {
    let root = workspace_root();
    let manifest_path = root
        .join("packs")
        .join("messaging-teams")
        .join("pack.manifest.json");
    let manifest: JsonValue = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
    let extensions = manifest
        .get("extensions")
        .ok_or_else(|| anyhow!("Teams manifest missing extensions"))?;

    let providers = extensions
        .get("greentic.provider-extension.v1")
        .and_then(|extension| extension.get("inline"))
        .and_then(|inline| inline.get("providers"))
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("Teams manifest missing provider extension"))?;
    let provider = providers
        .iter()
        .find(|provider| {
            provider.get("provider_type").and_then(JsonValue::as_str)
                == Some("messaging.teams.graph")
        })
        .ok_or_else(|| anyhow!("Teams manifest missing graph provider"))?;
    let ops = provider
        .get("ops")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("Teams graph provider missing ops"))?;
    assert!(
        ops.iter().any(|op| op.as_str() == Some("apply-answers")),
        "Teams graph provider must expose apply-answers"
    );

    let setup = extensions
        .get("messaging.oauth_device_code.v1")
        .and_then(|extension| extension.get("inline"))
        .ok_or_else(|| anyhow!("Teams manifest missing oauth device setup"))?;
    let scopes = setup
        .get("scopes")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| anyhow!("Teams manifest setup missing scopes"))?;
    assert!(
        scopes
            .iter()
            .any(|scope| scope.as_str() == Some("Channel.Create")),
        "Teams manifest setup must request Channel.Create"
    );

    let provisioning = setup
        .get("setup_modes")
        .and_then(|modes| modes.get("graph_channel"))
        .and_then(|graph| graph.get("provisioning"))
        .and_then(|provisioning| provisioning.get("teams_channel"))
        .ok_or_else(|| anyhow!("Teams manifest missing graph channel provisioning metadata"))?;
    assert_eq!(
        provisioning.get("op").and_then(JsonValue::as_str),
        Some("apply-answers")
    );
    assert_eq!(
        provisioning
            .get("graph")
            .and_then(|graph| graph.get("resource_template"))
            .and_then(JsonValue::as_str),
        Some("/teams/{team_id}/channels")
    );

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
