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
    let install_action = setup_actions
        .iter()
        .find(|action| action.get("id").and_then(Value::as_str) == Some("add_to_slack"))
        .ok_or_else(|| anyhow!("slack setup.yaml missing add_to_slack action"))?;
    assert_eq!(
        install_action.get("label").and_then(Value::as_str),
        Some("Setup Slack App"),
        "Slack setup-flow action must be labelled Setup Slack App; Add to Slack is reserved for the generic final action"
    );

    assert_eq!(
        registration.get("app_id_output").and_then(Value::as_str),
        Some("slack_app_id"),
        "Slack setup must persist the app id under the key setup_webhook reads at startup"
    );
    assert_eq!(
        registration.get("client_id_output").and_then(Value::as_str),
        None,
        "Slack manifest.update never returns client_id on reuse; registration must not depend on it"
    );
    assert_eq!(
        registration
            .get("signing_secret_output")
            .and_then(Value::as_str),
        Some("slack_signing_secret"),
        "Slack registration must expose Slack's signing secret for secret persistence"
    );
    assert_eq!(
        install_action.get("kind").and_then(Value::as_str),
        Some("open_url"),
        "Slack setup button must be a plain open_url link keyed on slack_app_id, not an OAuth-authorize button needing client_id"
    );
    assert_eq!(
        install_action.get("url_template").and_then(Value::as_str),
        Some("https://api.slack.com/apps/{slack_app_id}/install-on-team?"),
        "Slack setup button must link to the app's install-on-team console action using its app id"
    );

    Ok(())
}

#[test]
fn provider_final_setup_actions_are_generic_add_to_x_descriptors() -> Result<()> {
    let root = workspace_root();
    for (pack, expected_id, expected_label, expected_requires) in [
        (
            "messaging-slack",
            "add-to-slack",
            "Add to Slack",
            "slack_app_url",
        ),
        (
            "messaging-webex",
            "add-to-webex",
            "Add to WebEx",
            "bot_email",
        ),
        (
            "messaging-telegram",
            "add-to-telegram",
            "Add to Telegram",
            "bot_username",
        ),
    ] {
        let pack_yaml = root.join("packs").join(pack).join("pack.yaml");
        let value: Value = serde_yaml_bw::from_str(&fs::read_to_string(&pack_yaml)?)?;
        let inline = value
            .get("extensions")
            .and_then(|extensions| extensions.get("greentic.setup.actions.v1"))
            .and_then(|entry| entry.get("inline"))
            .ok_or_else(|| anyhow!("{pack} missing greentic.setup.actions.v1 inline metadata"))?;
        assert_eq!(
            inline.get("schema_id").and_then(Value::as_str),
            Some("greentic.setup.actions.v1"),
            "{pack} action descriptor must declare schema id"
        );
        let action = inline
            .get("actions")
            .and_then(Value::as_sequence)
            .and_then(|actions| {
                actions
                    .iter()
                    .find(|action| action.get("id").and_then(Value::as_str) == Some(expected_id))
            })
            .ok_or_else(|| anyhow!("{pack} missing final action {expected_id}"))?;
        assert_eq!(
            action.get("label").and_then(Value::as_str),
            Some(expected_label)
        );
        assert_eq!(
            action.get("kind").and_then(Value::as_str),
            Some("deep_link")
        );
        assert!(
            action
                .get("url_template")
                .and_then(Value::as_str)
                .is_some_and(|template| template.contains(&format!("{{{expected_requires}}}"))),
            "{pack} final action URL template must reference {expected_requires}"
        );
        let requires = action
            .get("requires")
            .and_then(Value::as_sequence)
            .ok_or_else(|| anyhow!("{pack} final action missing requires"))?;
        assert!(
            requires
                .iter()
                .any(|item| item.as_str() == Some(expected_requires)),
            "{pack} final action must require {expected_requires}"
        );

        let setup_yaml = root.join("packs").join(pack).join("assets/setup.yaml");
        let setup: Value = serde_yaml_bw::from_str(&fs::read_to_string(&setup_yaml)?)?;
        let setup_actions = setup
            .get("setup_actions")
            .and_then(Value::as_sequence)
            .ok_or_else(|| anyhow!("{pack} setup.yaml missing setup_actions"))?;
        if pack == "messaging-slack" {
            assert!(
                setup_actions
                    .iter()
                    .any(|action| action.get("kind").and_then(Value::as_str)
                        == Some("oauth_install_button")),
                "messaging-slack setup.yaml must declare the generic registration/OAuth setup action"
            );
        } else {
            let setup_action = setup_actions
                .iter()
                .find(|action| action.get("id").and_then(Value::as_str) == Some(expected_id))
                .ok_or_else(|| anyhow!("{pack} setup.yaml missing final action {expected_id}"))?;
            assert_eq!(
                setup_action.get("label").and_then(Value::as_str),
                Some(expected_label),
                "{pack} setup.yaml final action label mismatch"
            );
            assert_eq!(
                setup_action.get("kind").and_then(Value::as_str),
                Some("deep_link"),
                "{pack} setup.yaml final action must be a generic deep_link"
            );
            assert!(
                setup_action
                    .get("requires")
                    .and_then(Value::as_sequence)
                    .is_some_and(|requires| requires
                        .iter()
                        .any(|item| item.as_str() == Some(expected_requires))),
                "{pack} setup.yaml final action must require {expected_requires}"
            );
        }
    }

    let teams_answer: JsonValue = serde_json::from_str(&fs::read_to_string(
        root.join("messaging-teams").join("build-answer.json"),
    )?)?;
    let teams_actions = teams_answer
        .pointer("/setup_api/actions")
        .ok_or_else(|| anyhow!("messaging-teams build answer missing setup_api.actions"))?;
    assert_eq!(
        teams_actions.get("schema_id").and_then(JsonValue::as_str),
        Some("greentic.setup.actions.v1")
    );
    let add_to_teams = teams_actions
        .get("actions")
        .and_then(JsonValue::as_array)
        .and_then(|actions| {
            actions
                .iter()
                .find(|action| action.get("id").and_then(JsonValue::as_str) == Some("add-to-teams"))
        })
        .ok_or_else(|| anyhow!("messaging-teams missing add-to-teams final action"))?;
    assert_eq!(
        add_to_teams.get("label").and_then(JsonValue::as_str),
        Some("Add to Teams")
    );
    assert_eq!(
        add_to_teams.get("kind").and_then(JsonValue::as_str),
        Some("deep_link")
    );
    assert!(
        add_to_teams
            .get("requires")
            .and_then(JsonValue::as_array)
            .is_some_and(|requires| requires
                .iter()
                .any(|item| item.as_str() == Some("add_to_teams_url")))
    );
    let teams_setup: Value = serde_yaml_bw::from_str(&fs::read_to_string(
        root.join("messaging-teams/assets/setup.yaml"),
    )?)?;
    let teams_setup_action = teams_setup
        .get("setup_actions")
        .and_then(Value::as_sequence)
        .and_then(|actions| {
            actions
                .iter()
                .find(|action| action.get("id").and_then(Value::as_str) == Some("add-to-teams"))
        })
        .ok_or_else(|| anyhow!("messaging-teams setup.yaml missing add-to-teams final action"))?;
    assert_eq!(
        teams_setup_action.get("kind").and_then(Value::as_str),
        Some("deep_link")
    );
    assert!(
        teams_setup_action
            .get("requires")
            .and_then(Value::as_sequence)
            .is_some_and(|requires| requires
                .iter()
                .any(|item| item.as_str() == Some("add_to_teams_url"))),
        "messaging-teams setup.yaml Add to Teams must require add_to_teams_url"
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
        .join("messaging-teams-graph")
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
        setup_modes.get("bot_framework").is_none(),
        "Teams Graph setup must not advertise Bot Framework mode"
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
    let lookup = provisioning
        .get("graph")
        .and_then(|graph| graph.get("lookup"))
        .ok_or_else(|| anyhow!("Teams provisioning missing existing-channel lookup metadata"))?;
    assert_eq!(lookup.get("method").and_then(Value::as_str), Some("GET"));
    assert_eq!(
        lookup.get("resource_template").and_then(Value::as_str),
        Some("/teams/{team_id}/channels")
    );
    assert_eq!(
        lookup
            .get("match")
            .and_then(|matcher| matcher.get("field"))
            .and_then(Value::as_str),
        Some("displayName")
    );
    assert_eq!(
        lookup
            .get("match")
            .and_then(|matcher| matcher.get("source_key"))
            .and_then(Value::as_str),
        Some("desired_channel_name")
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
        .join("messaging-teams-graph")
        .join("pack.manifest.json");
    let desired_fixture_path = root
        .join("packs")
        .join("messaging-teams-graph")
        .join("fixtures")
        .join("subscriptions.desired-state-metadata.expected.json");
    let component_config_fixture_path = root
        .join("packs")
        .join("messaging-teams-graph")
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
    assert_eq!(
        desired_state
            .get("lifecycle_notification_url")
            .and_then(|value| value.get("template"))
            .and_then(JsonValue::as_str),
        Some("{public_base_url}/v1/messaging/ingress/{provider_id}/{tenant}/{team}")
    );
    assert_eq!(
        desired_state
            .get("lifecycle_notification_url")
            .and_then(|value| value.get("required_when_expiration_over_minutes"))
            .and_then(JsonValue::as_i64),
        Some(60)
    );
    assert_eq!(
        desired_state
            .get("expiration_policy")
            .and_then(|value| value.get("max_minutes"))
            .and_then(JsonValue::as_i64),
        Some(4320)
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
    assert!(
        templates.iter().all(|template| {
            template
                .get("lifecycle_notification_url")
                .and_then(JsonValue::as_str)
                == Some("{lifecycle_notification_url}")
        }),
        "Teams desired_state templates must pass lifecycle_notification_url to Graph subscriptions"
    );

    Ok(())
}

#[test]
fn pack_local_dist_directories_do_not_contain_stale_gtpack_artifacts() -> Result<()> {
    let root = workspace_root();
    let mut stale_artifacts = Vec::new();
    for entry in glob(root.join("packs/*/dist/*.gtpack").to_str().unwrap())? {
        stale_artifacts.push(entry?);
    }

    if !stale_artifacts.is_empty() {
        let list = stale_artifacts
            .iter()
            .map(|path| {
                path.strip_prefix(&root)
                    .unwrap_or(path)
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n  ");
        return Err(anyhow!(
            "pack-local dist directories must not contain stale .gtpack artifacts; \
             use dist/packs as the single built pack output directory:\n  {list}"
        ));
    }

    Ok(())
}

#[test]
fn teams_pack_manifest_declares_channel_provisioning_contract() -> Result<()> {
    let root = workspace_root();
    let manifest_path = root
        .join("packs")
        .join("messaging-teams-graph")
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
    let lookup = provisioning
        .get("graph")
        .and_then(|graph| graph.get("lookup"))
        .ok_or_else(|| anyhow!("Teams manifest provisioning missing existing-channel lookup"))?;
    assert_eq!(
        lookup.get("method").and_then(JsonValue::as_str),
        Some("GET")
    );
    assert_eq!(
        lookup
            .get("match")
            .and_then(|matcher| matcher.get("field"))
            .and_then(JsonValue::as_str),
        Some("displayName")
    );
    assert_eq!(
        lookup
            .get("match")
            .and_then(|matcher| matcher.get("source_key"))
            .and_then(JsonValue::as_str),
        Some("desired_channel_name")
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

#[test]
fn webchat_gui_direct_line_cache_is_scoped_and_401_safe() -> Result<()> {
    let root = workspace_root();
    let runtime_path = root
        .join("packs")
        .join("messaging-webchat-gui")
        .join("assets")
        .join("webchat-gui")
        .join("runtime-bootstrap.js");
    let runtime = fs::read_to_string(&runtime_path)?;

    assert!(
        runtime.contains("DIRECT_LINE_CACHE_VERSION"),
        "webchat-gui runtime must version Direct Line cache keys"
    );
    assert!(
        runtime.contains("directLineCacheKey"),
        "webchat-gui runtime must build scoped Direct Line cache keys"
    );
    assert!(
        runtime.contains("stableCachePart(window.location.origin)")
            && runtime.contains("stableCachePart(tenant)")
            && runtime.contains("stableCachePart(env)")
            && runtime.contains("stableCachePart(directLineTokenUrl())")
            && runtime.contains("stableCachePart(directLineDomain())"),
        "webchat-gui Direct Line cache key must include origin, tenant, env, token URL, and domain"
    );
    assert!(
        runtime.contains("LEGACY_TOKEN_CACHE_KEY = 'greentic_dl_token'")
            && runtime.contains("LEGACY_CONVERSATION_CACHE_KEY = 'greentic_dl_conversation'")
            && runtime.contains("clearLegacyDirectLineCache"),
        "webchat-gui runtime must clear legacy global Direct Line cache keys"
    );
    assert!(
        !runtime.contains("localStorage.getItem('greentic_dl_token')")
            && !runtime.contains("localStorage.setItem('greentic_dl_token'")
            && !runtime.contains("localStorage.getItem('greentic_dl_conversation')")
            && !runtime.contains("localStorage.setItem('greentic_dl_conversation'"),
        "webchat-gui runtime must not read or write global Direct Line cache keys"
    );
    assert!(
        runtime.contains("reloadOnceAfterDirectLineAuthFailure")
            && runtime.contains("response.status === 401")
            && runtime.contains("xhr.status === 401")
            && runtime.contains("sessionStorage.setItem(DIRECT_LINE_AUTH_RETRY_KEY, '1')"),
        "webchat-gui runtime must clear Direct Line cache and retry once after 401"
    );

    Ok(())
}
