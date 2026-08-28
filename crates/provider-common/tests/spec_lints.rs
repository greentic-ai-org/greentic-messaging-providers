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

// setup.yaml collects a provider's secrets and config; an asset-only pack has none.
fn declares_provider(pack_dir: &std::path::Path) -> Result<bool> {
    let pack_yaml = pack_dir.join("pack.yaml");
    if !pack_yaml.exists() {
        return Ok(false);
    }
    let value: Value = serde_yaml_bw::from_str(&fs::read_to_string(&pack_yaml)?)?;
    Ok(value
        .get("extensions")
        .and_then(|exts| exts.get("greentic.provider-extension.v1"))
        .is_some())
}

#[test]
fn all_provider_packs_have_setup_spec() -> Result<()> {
    let root = workspace_root();
    let packs_dir = root.join("packs");
    let mut checked = 0usize;
    for entry in fs::read_dir(&packs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() || !declares_provider(&path)? {
            continue;
        }
        checked += 1;
        let spec = path.join("assets").join("setup.yaml");
        if !spec.exists() {
            return Err(anyhow!("missing setup spec at {}", spec.display()));
        }
    }
    // Every pack skipped is indistinguishable from every pack passing.
    if checked == 0 {
        return Err(anyhow!("no provider packs found under packs/ to check"));
    }
    Ok(())
}

// Keeps the exemption above narrow: a pack that grows a spec must declare its provider.
#[test]
fn non_provider_packs_ship_no_setup_spec() -> Result<()> {
    let root = workspace_root();
    for entry in fs::read_dir(root.join("packs"))? {
        let path = entry?.path();
        if !path.is_dir() || declares_provider(&path)? {
            continue;
        }
        let spec = path.join("assets").join("setup.yaml");
        if spec.exists() {
            return Err(anyhow!(
                "{} declares no provider but ships a setup spec at {} — either \
                 declare the provider or drop the spec",
                path.display(),
                spec.display()
            ));
        }
    }
    Ok(())
}

// `gtc setup --non-interactive` resolves an unanswered `visible_if` gate as
// visible, so a gated `required: true` question is unconditionally required for
// every answers file that omits the gate field. Enforce such fields at apply
// time instead.
#[test]
fn required_setup_questions_are_not_gated_on_visibility() -> Result<()> {
    let root = workspace_root();
    let pattern = root.join("packs").join("*/assets/setup.yaml");
    let mut found = false;
    for entry in glob(pattern.to_str().unwrap())? {
        let path = entry?;
        let spec: Value = serde_yaml_bw::from_str(&fs::read_to_string(&path)?)?;
        let Some(questions) = spec.get("questions").and_then(Value::as_sequence) else {
            continue;
        };
        found = true;
        for question in questions {
            let required = question
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if required && question.get("visible_if").is_some() {
                return Err(anyhow!(
                    "{}: question '{}' is both required and gated by visible_if — \
                     non-interactive setup fails whenever the gate field is unanswered",
                    path.display(),
                    question
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("<unnamed>")
                ));
            }
        }
    }
    if !found {
        return Err(anyhow!(
            "no setup.yaml files with questions found under packs/"
        ));
    }
    Ok(())
}

// The webchat SPA gates the chat on the tenant file alone — it never consults
// the backend's /auth/config — so an enabled provider in the default tenant
// turns every unconfigured deployment into a login wall.
#[test]
fn default_tenant_ships_no_enabled_auth_provider() -> Result<()> {
    let root = workspace_root();
    let pattern = root
        .join("packs")
        .join("*/assets/webchat-gui/config/tenants/default.json");
    let mut found = false;
    for entry in glob(pattern.to_str().unwrap())? {
        let path = entry?;
        let config: JsonValue = serde_json::from_slice(&fs::read(&path)?)?;
        found = true;
        let providers = config
            .get("auth")
            .and_then(|auth| auth.get("providers"))
            .and_then(|providers| providers.as_array());
        let Some(providers) = providers else {
            continue;
        };
        for provider in providers {
            if provider.get("enabled").and_then(JsonValue::as_bool) == Some(true) {
                return Err(anyhow!(
                    "{}: auth provider '{}' is enabled by default — a tenant that has not \
                     opted into OAuth would render a login wall instead of the chat",
                    path.display(),
                    provider
                        .get("id")
                        .and_then(JsonValue::as_str)
                        .unwrap_or("<unnamed>")
                ));
            }
        }
    }
    if !found {
        return Err(anyhow!(
            "no webchat-gui default tenant config found under packs/"
        ));
    }
    Ok(())
}

// The importer rewrites the default tenant file on every run, so a template
// that enables a provider re-introduces the login wall the lint above forbids.
#[test]
fn asset_importer_template_ships_no_enabled_auth_provider() -> Result<()> {
    let root = workspace_root();
    let script = root.join("tools").join("import_webchat_gui_assets.sh");
    let body = fs::read_to_string(&script)?;
    for needle in ["\"enabled\": True", "\"enabled\":True"] {
        if body.contains(needle) {
            return Err(anyhow!(
                "{}: default tenant template contains `{}` — re-running the importer \
                 would turn every unconfigured deployment into a login wall",
                script.display(),
                needle
            ));
        }
    }
    Ok(())
}

// ops/ingest.rs routes these four suffixes for every webchat variant. An
// endpoint the router handles but no pack declares is unreachable in the runtime.
#[test]
fn webchat_packs_declare_every_routed_endpoint() -> Result<()> {
    const REQUIRED_SUFFIXES: &[&str] = &[
        "/auth/config",
        "/token",
        "/v3/directline/{path*}",
        "/oauth/token-exchange",
    ];

    let root = workspace_root();
    let mut checked = 0usize;
    for entry in glob(root.join("packs").join("*/pack.yaml").to_str().unwrap())? {
        let path = entry?;
        let pack: Value = serde_yaml_bw::from_str(&fs::read_to_string(&path)?)?;
        let routes = pack
            .get("extensions")
            .and_then(|exts| exts.get("greentic.http-routes.v1"))
            .and_then(|ext| ext.get("inline"))
            .and_then(|inline| inline.get("routes"))
            .and_then(Value::as_sequence);
        let Some(routes) = routes else {
            continue;
        };
        let patterns: Vec<&str> = routes
            .iter()
            .filter_map(|route| route.get("pattern").and_then(Value::as_str))
            .collect();
        if !patterns
            .iter()
            .any(|pattern| pattern.contains("/v1/messaging/webchat/"))
        {
            continue;
        }
        checked += 1;
        for suffix in REQUIRED_SUFFIXES {
            if !patterns.iter().any(|pattern| pattern.ends_with(suffix)) {
                return Err(anyhow!(
                    "{}: ops/ingest.rs handles `{}` but no declared route ends with it",
                    path.display(),
                    suffix
                ));
            }
        }
    }
    if checked == 0 {
        return Err(anyhow!(
            "no webchat packs with http routes found under packs/"
        ));
    }
    Ok(())
}

// 3aigent-gui mirrors the webchat-gui SPA at build time, so a shared asset
// missing from its declaration is a drift signal between two copies of one tree.
#[test]
fn gui_packs_declare_the_same_shared_spa_assets() -> Result<()> {
    const PREFIX: &str = "assets/webchat-gui/";

    fn declared_spa_assets(pack_yaml: &std::path::Path) -> Result<Vec<String>> {
        let pack: Value = serde_yaml_bw::from_str(&fs::read_to_string(pack_yaml)?)?;
        let Some(assets) = pack.get("assets").and_then(Value::as_sequence) else {
            return Err(anyhow!("{}: no assets block", pack_yaml.display()));
        };
        Ok(assets
            .iter()
            .filter_map(|asset| asset.get("path").and_then(Value::as_str))
            .filter(|path| path.starts_with(PREFIX))
            .map(str::to_string)
            .collect())
    }

    let root = workspace_root();
    let reference = declared_spa_assets(&root.join("packs/messaging-webchat-gui/pack.yaml"))?;
    let mirrored = declared_spa_assets(&root.join("packs/messaging-3aigent-gui/pack.yaml"))?;

    for asset in &reference {
        if !mirrored.contains(asset) {
            return Err(anyhow!(
                "packs/messaging-3aigent-gui/pack.yaml: shared SPA asset '{}' is declared by \
                 messaging-webchat-gui but missing here",
                asset
            ));
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
        // Keyed on slack_app_id, not slack_app_url: manifest.update (app reuse)
        // never returns client_id, so the app_redirect helper that produced
        // slack_app_url was dropped. app_redirect is built from the app id.
        (
            "messaging-slack",
            "add-to-slack",
            "Add to Slack",
            "slack_app_id",
        ),
        (
            "messaging-webex",
            "add-to-webex",
            "Add to Webex",
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
            // Slack's setup action is a plain open_url install-on-team link, not
            // an oauth_install_button: manifest.update (app reuse) never returns
            // client_id, which the OAuth-authorize flow depended on. The shape is
            // asserted in detail by slack_setup_action_* above.
            assert!(
                setup_actions
                    .iter()
                    .any(|action| action.get("kind").and_then(Value::as_str) == Some("open_url")),
                "messaging-slack setup.yaml must declare the install-on-team setup action"
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
        runtime.contains("stableCachePart(directLineIdentityPart())"),
        "webchat-gui Direct Line cache key must include the authenticated identity"
    );
    let logout_body = runtime
        .split_once("function performLogout()")
        .and_then(|(_, rest)| rest.split_once('}'))
        .map(|(body, _)| body)
        .expect("webchat-gui runtime must define performLogout");
    let clears_cache_at = logout_body.find("clearDirectLineCache()");
    let clears_oauth_at = logout_body.find("clearOAuthSession()");
    assert!(
        matches!(
            (clears_cache_at, clears_oauth_at),
            (Some(cache_idx), Some(oauth_idx)) if cache_idx < oauth_idx
        ),
        "webchat-gui logout must clear the Direct Line cache before clearing the OAuth \
         session — clearing after would compute the cache key against the already-cleared \
         (anonymous) identity and leave the real user's token behind"
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
            && runtime.contains("sessionStorage.setItem(directLineAuthRetryKey(), '1')"),
        "webchat-gui runtime must clear Direct Line cache and retry once after 401"
    );

    Ok(())
}

// The three faults that together made a correctly configured Greentic SSO login
// impossible to complete. Each is one line of source, and each is invisible
// until someone turns SSO on, so pin them here rather than in an e2e run.
#[test]
fn webchat_gui_runtime_completes_the_sso_login_path() -> Result<()> {
    let root = workspace_root();
    let runtime = fs::read_to_string(
        root.join("packs")
            .join("messaging-webchat-gui")
            .join("assets")
            .join("webchat-gui")
            .join("runtime-bootstrap.js"),
    )?;

    // 1. The fetch wrapper matches the Direct Line token endpoint on pathname
    //    alone, so an IdP's `https://<issuer>/oauth/token` matches too. Left
    //    unguarded it answers the PKCE code exchange from the chat-token cache.
    let wrapper = runtime
        .split_once("window.fetch = function (input, init)")
        .map(|(_, rest)| rest)
        .ok_or_else(|| anyhow!("webchat-gui runtime must wrap window.fetch"))?;
    let bypass_at = wrapper
        .find("if (url.origin !== window.location.origin)")
        .ok_or_else(|| {
            anyhow!(
                "the fetch wrapper must pass cross-origin requests straight through — \
                 without it the identity provider's /oauth/token is answered from the \
                 Direct Line token cache and the PKCE exchange can never complete"
            )
        })?;
    let first_rule_at = wrapper
        .find("test(url.pathname)")
        .ok_or_else(|| anyhow!("the fetch wrapper must route on url.pathname"))?;
    assert!(
        bypass_at < first_rule_at,
        "the cross-origin bypass must come before the first pathname rule"
    );
    // The bypass alone is not enough: a same-origin issuer serves /oauth/token
    // from this very origin, and a bare `/token$` rule swallows it there too.
    assert!(
        runtime.contains("/\\/v1\\/messaging\\/webchat\\/[^/]+(?:\\/[^/]+)?\\/token$"),
        "the Direct Line token rule must be anchored to the webchat backend base, not to a \
         bare `/token$` suffix that also matches an identity provider's /oauth/token"
    );

    // 2. Only the SPA's dummy provider ever wrote the key the SPA gates its
    //    login page on, so a completed SSO login left login on screen.
    assert!(
        runtime.contains("function saveAppAuthSession(")
            && runtime.contains("isAuthenticated: true")
            && runtime.contains("function completeAppLogin("),
        "webchat-gui runtime must write the app auth session the SPA reads; nothing \
         in the SPA bundle writes it outside its dummy provider path"
    );
    // Both paths that hand off to the React SPA — the greentic SDK and the
    // OAuth code exchange — need the reload that makes the write visible.
    assert!(
        runtime.matches("if (completeAppLogin(").count() >= 2,
        "the greentic SDK and the OAuth code exchange both hand the login to the \
         React SPA, so both must complete the app session"
    );
    // Guest finishes in-page and deliberately does not reload, but a write that
    // never landed must not pass for a login there either.
    assert!(
        runtime.contains("if (!saveAppAuthSession(provider.id, 'Guest'))"),
        "the guest path writes the session without reloading; it still has to check \
         the write, or a browser blocking site data reports a login that did not happen"
    );
    assert!(
        runtime.contains("return !!getAppAuthSession();"),
        "saveAppAuthSession must read its own write back — reloading on a write that \
         silently failed turns every login attempt into a loop with nothing logged"
    );
    // A session marked authenticated on a failed exchange holds no token, so
    // Direct Line is minted anonymously behind a logout button.
    assert!(
        !runtime.contains("saveOAuthSession('authenticated', 'oauth-code')"),
        "a failed token exchange must clear the session and say so, not record the \
         literal handle 'authenticated' and let Direct Line mint anonymous"
    );

    // 3. A tenant can host several bundles, so both server calls that carry or
    //    mint credentials have to name this deployment.
    assert!(
        runtime.contains("chatApiBase: provider.chat_api_base || bundleScopedBackendBase(tenant)"),
        "the SSO client's chat API base must be bundle-scoped, like the Direct Line \
         token URL — a tenant-scoped mint can resolve to a sibling bundle with no \
         oidc_issuer and reject a bearer whose issuer config is correct"
    );
    assert!(
        runtime.contains("bundleScopedBackendBase(tenant) + '/oauth/token-exchange'"),
        "the token-exchange proxy must be bundle-scoped — it needs this deployment's \
         client secret, not a sibling's"
    );

    Ok(())
}
