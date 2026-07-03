use provider_common::component_v0_6::{canonical_cbor_bytes, decode_cbor};
use provider_common::helpers::{
    cbor_json_invoke_bridge, existing_config_from_answers, json_bytes, optional_string_from,
    schema_core_healthcheck, schema_core_validate_config,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

mod bindings {
    wit_bindgen::generate!({
        path: "wit/messaging-provider-sms",
        world: "component-v0-v6-v0",
        generate_all
    });
}

mod config;
mod describe;
mod ops;

use config::{ProviderConfigOut, default_config_out, validate_config_out};
use describe::{
    DEFAULT_KEYS, I18N_KEYS, I18N_PAIRS, SETUP_QUESTIONS, build_describe_payload, build_qa_spec,
};

const PROVIDER_ID: &str = "messaging-provider-sms";
const PROVIDER_TYPE: &str = "messaging.sms.twilio";
const WORLD_ID: &str = "component-v0-v6-v0";
const ACCOUNT_SID_KEY: &str = "TWILIO_ACCOUNT_SID";
const AUTH_TOKEN_KEY: &str = "TWILIO_AUTH_TOKEN";
const FROM_NUMBER_KEY: &str = "TWILIO_FROM_NUMBER";

#[derive(Debug, Clone, Serialize)]
struct ApplyAnswersResult {
    ok: bool,
    config: Option<ProviderConfigOut>,
    secrets_patch: Option<SecretsPatch>,
    remove: Option<RemovePlan>,
    diagnostics: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SecretsPatch {
    set: BTreeMap<String, String>,
    delete: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RemovePlan {
    remove_all: bool,
    cleanup: Vec<String>,
}

impl ApplyAnswersResult {
    fn decode_error(error: String) -> Self {
        Self {
            ok: false,
            config: None,
            secrets_patch: None,
            remove: None,
            diagnostics: Vec::new(),
            error: Some(error),
        }
    }

    fn success(config: ProviderConfigOut, secrets_set: BTreeMap<String, String>) -> Self {
        Self {
            ok: true,
            config: Some(config),
            secrets_patch: (!secrets_set.is_empty()).then_some(SecretsPatch {
                set: secrets_set,
                delete: Vec::new(),
            }),
            remove: None,
            diagnostics: Vec::new(),
            error: None,
        }
    }

    fn remove_default() -> Self {
        Self {
            ok: true,
            config: None,
            secrets_patch: None,
            remove: Some(RemovePlan {
                remove_all: true,
                cleanup: provider_common::qa_helpers::DEFAULT_REMOVE_CLEANUP
                    .iter()
                    .map(|step| (*step).to_string())
                    .collect(),
            }),
            diagnostics: Vec::new(),
            error: None,
        }
    }
}

struct Component;

impl bindings::exports::greentic::component::descriptor::Guest for Component {
    fn describe() -> Vec<u8> {
        canonical_cbor_bytes(&build_describe_payload())
    }
}

impl bindings::exports::greentic::component::runtime::Guest for Component {
    fn invoke(op: String, input_cbor: Vec<u8>) -> Vec<u8> {
        cbor_json_invoke_bridge(&op, &input_cbor, None, |op, input| {
            dispatch_json_invoke(op, input)
        })
    }
}

impl bindings::exports::greentic::component::qa::Guest for Component {
    fn qa_spec(mode: bindings::exports::greentic::component::qa::Mode) -> Vec<u8> {
        canonical_cbor_bytes(&build_qa_spec(mode))
    }

    fn apply_answers(
        mode: bindings::exports::greentic::component::qa::Mode,
        answers_cbor: Vec<u8>,
    ) -> Vec<u8> {
        use bindings::exports::greentic::component::qa::Mode;
        let mode_str = match mode {
            Mode::Default => "default",
            Mode::Setup => "setup",
            Mode::Upgrade => "upgrade",
            Mode::Remove => "remove",
        };
        apply_answers_impl(mode_str, answers_cbor)
    }
}

impl bindings::exports::greentic::component::component_i18n::Guest for Component {
    fn i18n_keys() -> Vec<String> {
        provider_common::helpers::i18n_keys_from(I18N_KEYS)
    }

    fn i18n_bundle(locale: String) -> Vec<u8> {
        provider_common::helpers::i18n_bundle_from_pairs(locale, I18N_PAIRS)
    }
}

// Backward-compatible schema-core-api export for operator v0.4.x
impl bindings::exports::greentic::provider_schema_core::schema_core_api::Guest for Component {
    fn describe() -> Vec<u8> {
        serde_json::to_vec(&build_describe_payload()).unwrap_or_default()
    }

    fn validate_config(_config_json: Vec<u8>) -> Vec<u8> {
        schema_core_validate_config()
    }

    fn healthcheck() -> Vec<u8> {
        schema_core_healthcheck()
    }

    fn invoke(op: String, input_json: Vec<u8>) -> Vec<u8> {
        if let Some(result) = provider_common::qa_invoke_bridge::dispatch_qa_ops_with_i18n(
            &op,
            &input_json,
            "sms",
            SETUP_QUESTIONS,
            DEFAULT_KEYS,
            I18N_KEYS,
            I18N_PAIRS,
            apply_answers_impl,
        ) {
            return result;
        }
        dispatch_json_invoke(&op, &input_json)
    }
}

impl bindings::exports::greentic::provider_instance_identity::instance_identity_api::Guest
    for Component
{
    fn identify_instance(_input_json: Vec<u8>) -> Option<String> {
        // Twilio's `To` number could double as a per-instance discriminator
        // for multi-number tenants; deferred until the ingest task lands.
        None
    }
}

impl bindings::exports::greentic::provider_instance_identity::instance_identity_describe_api::Guest
    for Component
{
    fn describe_identify_instance() -> Option<Vec<u8>> {
        None
    }
}

bindings::export!(Component with_types_in bindings);

fn apply_answers_impl(mode: &str, answers_cbor: Vec<u8>) -> Vec<u8> {
    let answers: Value = match decode_cbor(&answers_cbor) {
        Ok(value) => value,
        Err(err) => {
            return canonical_cbor_bytes(&ApplyAnswersResult::decode_error(format!(
                "invalid answers cbor: {err}"
            )));
        }
    };

    if mode == "remove" {
        return canonical_cbor_bytes(&ApplyAnswersResult::remove_default());
    }

    let mut merged = existing_config_from_answers(&answers).unwrap_or_else(default_config_out);
    let mut secrets_set = BTreeMap::new();
    let answer_obj = answers.as_object();
    let has = |key: &str| answer_obj.is_some_and(|obj| obj.contains_key(key));

    if mode == "setup" || mode == "default" {
        merged.enabled = answers
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(merged.enabled);
        collect_secret_answer(&answers, "account_sid", ACCOUNT_SID_KEY, &mut secrets_set);
        collect_secret_answer(&answers, "auth_token", AUTH_TOKEN_KEY, &mut secrets_set);
        collect_secret_answer(&answers, "from_number", FROM_NUMBER_KEY, &mut secrets_set);
    }

    if mode == "upgrade" {
        if has("enabled") {
            merged.enabled = answers
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(merged.enabled);
        }
        if has("account_sid") {
            collect_secret_answer(&answers, "account_sid", ACCOUNT_SID_KEY, &mut secrets_set);
        }
        if has("auth_token") {
            collect_secret_answer(&answers, "auth_token", AUTH_TOKEN_KEY, &mut secrets_set);
        }
        if has("from_number") {
            collect_secret_answer(&answers, "from_number", FROM_NUMBER_KEY, &mut secrets_set);
        }
    }

    if let Err(error) = validate_config_out(&merged) {
        return canonical_cbor_bytes(&ApplyAnswersResult::decode_error(error));
    }

    canonical_cbor_bytes(&ApplyAnswersResult::success(merged, secrets_set))
}

fn collect_secret_answer(
    answers: &Value,
    answer_key: &str,
    secret_key: &str,
    secrets_set: &mut BTreeMap<String, String>,
) {
    if let Some(value) = optional_string_from(answers, answer_key) {
        secrets_set.insert(secret_key.to_string(), value);
    }
}

fn dispatch_json_invoke(op: &str, input_json: &[u8]) -> Vec<u8> {
    match op {
        "ingest_http" => ops::ingest_http(input_json),
        "render_plan" => ops::render_plan(input_json),
        "encode" => ops::encode_op(input_json),
        "send_payload" => ops::send_payload(input_json),
        "setup_webhook" => ops::setup_webhook(input_json),
        other => json_bytes(&json!({"ok": false, "error": format!("unsupported op: {other}")})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_reports_sms_identity_and_secrets() {
        let d = build_describe_payload();
        assert_eq!(d.provider, "messaging-provider-sms");
        assert_eq!(d.provider_type, "messaging.sms.twilio");
        for expected_op in ["ingest_http", "render_plan", "encode", "send_payload"] {
            assert!(
                d.operations.iter().any(|o| o.name == expected_op),
                "op {expected_op} present"
            );
        }
        let secret_names: Vec<_> = d
            .secret_requirements
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        for expected_secret in [
            "TWILIO_ACCOUNT_SID",
            "TWILIO_AUTH_TOKEN",
            "TWILIO_FROM_NUMBER",
        ] {
            assert!(
                secret_names.contains(&expected_secret),
                "secret {expected_secret} declared"
            );
            let requirement = d
                .secret_requirements
                .iter()
                .find(|s| s.name == expected_secret)
                .expect("requirement present");
            assert_eq!(requirement.scope, "tenant");
        }
    }

    #[test]
    fn describe_world_matches_shared_world_id() {
        let d = build_describe_payload();
        assert_eq!(d.world, WORLD_ID);
    }

    #[test]
    fn apply_answers_setup_collects_twilio_secrets() {
        use bindings::exports::greentic::component::qa::Guest as QaGuest;
        use bindings::exports::greentic::component::qa::Mode;
        let answers = json!({
            "account_sid": "AC_test",
            "auth_token": "token_test",
            "from_number": "+15551234567"
        });
        let out =
            <Component as QaGuest>::apply_answers(Mode::Setup, canonical_cbor_bytes(&answers));
        let out_json: Value = decode_cbor(&out).expect("decode apply output");
        assert_eq!(out_json.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(
            out_json["secrets_patch"]["set"][ACCOUNT_SID_KEY],
            Value::String("AC_test".to_string())
        );
        assert_eq!(
            out_json["secrets_patch"]["set"][AUTH_TOKEN_KEY],
            Value::String("token_test".to_string())
        );
        assert_eq!(
            out_json["secrets_patch"]["set"][FROM_NUMBER_KEY],
            Value::String("+15551234567".to_string())
        );
    }

    #[test]
    fn apply_answers_remove_returns_cleanup_plan() {
        use bindings::exports::greentic::component::qa::Guest as QaGuest;
        use bindings::exports::greentic::component::qa::Mode;
        let out =
            <Component as QaGuest>::apply_answers(Mode::Remove, canonical_cbor_bytes(&json!({})));
        let out_json: Value = decode_cbor(&out).expect("decode apply output");
        assert_eq!(out_json.get("ok"), Some(&Value::Bool(true)));
        assert!(
            !out_json["remove"]["cleanup"]
                .as_array()
                .expect("cleanup steps")
                .is_empty()
        );
    }
}
