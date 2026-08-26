//! WebChat GUI messaging provider component.

use base64::{Engine as _, engine::general_purpose};
use provider_common::component_v0_6::{
    DescribePayload, SchemaIr, canonical_cbor_bytes, decode_cbor, schema_hash,
};
use provider_common::helpers::{i18n_bundle_from_pairs, json_bytes};
use provider_common::redact;
use provider_common::telemetry::{self, Field, Level, Span, field};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

mod bindings {
    wit_bindgen::generate!({
        path: "wit/messaging-provider-webchat-gui",
        world: "component-v0-v6-v0",
        generate_all
    });
}

pub(crate) mod config;
#[allow(dead_code)]
#[path = "../../messaging-provider-webchat/src/describe.rs"]
mod describe;
pub(crate) mod directline;
#[path = "../../messaging-provider-webchat/src/ops/mod.rs"]
mod ops;

pub(crate) const PROVIDER_ID: &str = "messaging-provider-webchat-gui";
pub(crate) const PROVIDER_TYPE: &str = "messaging.webchat-gui";
pub(crate) const WORLD_ID: &str = "component-v0-v6-v0";

use config::{
    PresentationMode, ProviderConfig, ProviderConfigOut, default_config_out, default_mode,
    normalize_nav_links, validate_config_out, validate_provider_config,
};
use describe::{
    DEFAULT_KEYS, I18N_KEYS, I18N_PAIRS, SETUP_QUESTIONS, build_describe_payload, build_qa_spec,
};
use ops::{encode_op, handle_ingest, handle_send, ingest_http, render_plan, send_payload};

struct Component;

impl bindings::exports::greentic::component::descriptor::Guest for Component {
    fn describe() -> Vec<u8> {
        canonical_cbor_bytes(&build_gui_describe_payload())
    }
}

impl bindings::exports::greentic::component::runtime::Guest for Component {
    fn invoke(op: String, input_cbor: Vec<u8>) -> Vec<u8> {
        let input_value: Value = match decode_cbor(&input_cbor) {
            Ok(value) => value,
            Err(err) => {
                let detail = redact::error_message(&err.to_string());
                telemetry::emit(
                    Level::Error,
                    PROVIDER_TYPE,
                    "invalid input cbor",
                    &[
                        Field {
                            key: "op",
                            value: op.as_str(),
                        },
                        Field {
                            key: field::ERROR,
                            value: &detail,
                        },
                    ],
                );
                return canonical_cbor_bytes(
                    &json!({"ok": false, "error": format!("invalid input cbor: {detail}")}),
                );
            }
        };
        let input_json = serde_json::to_vec(&input_value).unwrap_or_default();
        let output_json = dispatch_json_invoke(&op, &input_json);
        let output_value: Value = serde_json::from_slice(&output_json)
            .unwrap_or_else(|_| json!({"ok": false, "error": "provider produced invalid json"}));
        canonical_cbor_bytes(&output_value)
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
        apply_answers_impl(mode, answers_cbor)
    }
}

impl bindings::exports::greentic::component::component_i18n::Guest for Component {
    fn i18n_keys() -> Vec<String> {
        filtered_i18n_keys()
    }

    fn i18n_bundle(locale: String) -> Vec<u8> {
        i18n_bundle_from_pairs(locale, &filtered_i18n_pairs())
    }
}

impl bindings::exports::greentic::provider_schema_core::schema_core_api::Guest for Component {
    fn describe() -> Vec<u8> {
        serde_json::to_vec(&build_gui_describe_payload()).unwrap_or_default()
    }

    fn validate_config(config_json: Vec<u8>) -> Vec<u8> {
        match serde_json::from_slice::<ProviderConfig>(&config_json)
            .map_err(|err| format!("invalid config: {err}"))
            .and_then(validate_provider_config)
        {
            Ok(_) => json_bytes(&json!({"ok": true})),
            Err(error) => json_bytes(&json!({"ok": false, "error": error})),
        }
    }

    fn healthcheck() -> Vec<u8> {
        json_bytes(&json!({"status": "healthy"}))
    }

    fn invoke(op: String, input_json: Vec<u8>) -> Vec<u8> {
        if let Some(result) = provider_common::qa_invoke_bridge::dispatch_qa_ops_with_i18n(
            &op,
            &input_json,
            "webchat",
            SETUP_QUESTIONS,
            DEFAULT_KEYS,
            &filtered_i18n_key_refs(),
            &filtered_i18n_pairs(),
            apply_answers_bridge,
        ) {
            return result;
        }
        dispatch_json_invoke(&op, &input_json)
    }
}

impl bindings::exports::provider::common::ingress::Guest for Component {
    /// Inbound WebChat DirectLine ingress. The runtime (`greentic-start`)
    /// dispatches HTTP requests for `auth/config`, `/v3/directline/*` and the
    /// `/token` shorthand through `provider:common/ingress@0.0.2#handle-webhook`.
    ///
    /// It packs `method`/`path`/`query` into the headers object and passes the
    /// raw request body separately. We rebuild the operator-format `HttpInV1`
    /// the shared `ingest_http` router already understands, then hand its
    /// `HttpOutV1` JSON straight back — the runtime's `parse_http_response`
    /// reads `status`/`headers`/`body_b64` from it verbatim.
    fn handle_webhook(headers_json: String, body_json: String) -> Result<String, String> {
        let headers: serde_json::Map<String, Value> = serde_json::from_str(&headers_json)
            .map_err(|err| format!("invalid ingress headers json: {err}"))?;

        let header_str = |key: &str| {
            headers
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let method = match header_str("method") {
            m if m.is_empty() => "POST".to_string(),
            m => m,
        };
        let path = header_str("path");
        let query = header_str("query");
        let header_pairs: Vec<Value> = headers
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "method" | "path" | "query"))
            .map(|(key, value)| json!([key, value.as_str().unwrap_or_default()]))
            .collect();

        let operator_input = json!({
            "method": method,
            "path": path,
            "query": query,
            "headers": header_pairs,
            "body_b64": general_purpose::STANDARD.encode(body_json.as_bytes()),
        });
        let input_bytes = serde_json::to_vec(&operator_input)
            .map_err(|err| format!("encode ingress request: {err}"))?;

        let output = ingest_http(&input_bytes);
        String::from_utf8(output).map_err(|err| format!("ingress response not utf-8: {err}"))
    }
}

impl bindings::exports::greentic::provider_instance_identity::instance_identity_api::Guest
    for Component
{
    fn identify_instance(input_json: Vec<u8>) -> Option<String> {
        ops::extract_recipient_id(&input_json)
    }
}

impl bindings::exports::greentic::provider_instance_identity::instance_identity_describe_api::Guest
    for Component
{
    fn describe_identify_instance() -> Option<Vec<u8>> {
        Some(ops::IDENTIFY_HINT_JSON.to_vec())
    }
}

bindings::export!(Component with_types_in bindings);

fn build_gui_describe_payload() -> DescribePayload {
    let mut payload = build_describe_payload();
    if let SchemaIr::Object { fields, .. } = &mut payload.config_schema {
        fields.remove("jwt_signing_key");
    }
    payload
        .redactions
        .retain(|rule| rule.path != "$.jwt_signing_key");
    payload.schema_hash = schema_hash(
        &payload.input_schema,
        &payload.output_schema,
        &payload.config_schema,
    );
    payload
}

fn is_generated_secret_i18n_key(key: &str) -> bool {
    key.contains("jwt_signing_key")
}

fn filtered_i18n_keys() -> Vec<String> {
    I18N_KEYS
        .iter()
        .copied()
        .filter(|key| !is_generated_secret_i18n_key(key))
        .map(ToOwned::to_owned)
        .collect()
}

fn filtered_i18n_key_refs() -> Vec<&'static str> {
    I18N_KEYS
        .iter()
        .copied()
        .filter(|key| !is_generated_secret_i18n_key(key))
        .collect()
}

fn filtered_i18n_pairs() -> Vec<(&'static str, &'static str)> {
    I18N_PAIRS
        .iter()
        .copied()
        .filter(|(key, _)| !is_generated_secret_i18n_key(key))
        .collect()
}

fn apply_answers_bridge(mode: &str, answers_cbor: Vec<u8>) -> Vec<u8> {
    use bindings::exports::greentic::component::qa::Mode;
    let mode = match mode {
        "setup" => Mode::Setup,
        "upgrade" => Mode::Upgrade,
        "remove" => Mode::Remove,
        _ => Mode::Default,
    };
    apply_answers_impl(mode, answers_cbor)
}

fn dispatch_json_invoke(op: &str, input_json: &[u8]) -> Vec<u8> {
    // One span per op so the host can correlate the webchat-gui-specific
    // invocation with the downstream events emitted by the shared webchat
    // ops modules. The PROVIDER_TYPE tag distinguishes gui from plain webchat
    // in the same telemetry stream.
    let _span = Span::enter(
        op,
        PROVIDER_TYPE,
        &[Field {
            key: field::STEP,
            value: op,
        }],
    );
    match op {
        "run" | "send" => handle_send(input_json),
        "ingest" => handle_ingest(input_json),
        "ingest_http" | "ingest-http" => ingest_http(input_json),
        "render_plan" | "render-plan" => render_plan(input_json),
        "encode" => encode_op(input_json),
        "send_payload" | "send-payload" => send_payload(input_json),
        other => {
            telemetry::emit(
                Level::Warn,
                PROVIDER_TYPE,
                "unsupported op",
                &[Field {
                    key: "op",
                    value: other,
                }],
            );
            json_bytes(&json!({"ok": false, "error": format!("unsupported op: {other}")}))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApplyAnswersResult {
    ok: bool,
    config: Option<ProviderConfigOut>,
    secrets_patch: Option<SecretsPatch>,
    remove: Option<RemovePlan>,
    diagnostics: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SecretsPatch {
    set: BTreeMap<String, String>,
    delete: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemovePlan {
    remove_all: bool,
    cleanup: Vec<String>,
}

fn apply_answers_impl(
    mode: bindings::exports::greentic::component::qa::Mode,
    answers_cbor: Vec<u8>,
) -> Vec<u8> {
    use bindings::exports::greentic::component::qa::Mode;

    let mode_str = match mode {
        Mode::Setup => "setup",
        Mode::Upgrade => "upgrade",
        Mode::Remove => "remove",
        Mode::Default => "default",
    };
    let _span = Span::enter(
        "apply_answers",
        PROVIDER_TYPE,
        &[Field {
            key: "mode",
            value: mode_str,
        }],
    );

    let answers: Value = match decode_cbor(&answers_cbor) {
        Ok(value) => value,
        Err(err) => {
            let detail = redact::error_message(&err.to_string());
            telemetry::emit(
                Level::Error,
                PROVIDER_TYPE,
                "apply_answers invalid cbor",
                &[
                    Field {
                        key: "mode",
                        value: mode_str,
                    },
                    Field {
                        key: field::ERROR,
                        value: &detail,
                    },
                ],
            );
            return canonical_cbor_bytes(&ApplyAnswersResult {
                ok: false,
                config: None,
                secrets_patch: None,
                remove: None,
                diagnostics: Vec::new(),
                error: Some(format!("invalid answers cbor: {detail}")),
            });
        }
    };

    if mode == Mode::Remove {
        telemetry::emit(
            Level::Info,
            PROVIDER_TYPE,
            "tenant remove plan",
            &[Field {
                key: "mode",
                value: "remove",
            }],
        );
        return canonical_cbor_bytes(&ApplyAnswersResult {
            ok: true,
            config: None,
            secrets_patch: None,
            remove: Some(RemovePlan {
                remove_all: true,
                cleanup: vec![
                    "delete_config_key".to_string(),
                    "delete_provenance_key".to_string(),
                    "delete_provider_state_namespace".to_string(),
                ],
            }),
            diagnostics: Vec::new(),
            error: None,
        });
    }

    let mut merged = existing_config_from_answers(&answers).unwrap_or_else(default_config_out);
    let mut secrets_set = BTreeMap::new();
    let answer_obj = answers.as_object();
    let has = |key: &str| answer_obj.is_some_and(|obj| obj.contains_key(key));

    if mode == Mode::Setup || mode == Mode::Default {
        merged.enabled = answers
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(merged.enabled);
        merged.public_base_url =
            string_or_default(&answers, "public_base_url", &merged.public_base_url);
        merged.mode = string_or_default(&answers, "mode", &merged.mode);
        if merged.mode.trim().is_empty() {
            merged.mode = default_mode();
        }
        merged.route = optional_string_from(&answers, "route").or(merged.route.clone());
        merged.tenant_channel_id = optional_string_from(&answers, "tenant_channel_id")
            .or(merged.tenant_channel_id.clone());
        merged.base_url = optional_string_from(&answers, "base_url").or(merged.base_url.clone());
        let jwt_key = optional_string_from(&answers, "jwt_signing_key")
            .or_else(|| take_existing_jwt_key(&mut merged))
            .unwrap_or_else(generate_secret_20);
        secrets_set.insert("jwt_signing_key".to_string(), jwt_key);
        merged.jwt_signing_key_b64 = None;
        merged.oauth_enabled = answers
            .get("oauth_enabled")
            .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
            .or(merged.oauth_enabled);
        merged.oauth_providers =
            compose_oauth_providers(&answers).or(merged.oauth_providers.clone());
        if let Some(presentation_mode) = presentation_mode_from_answers(&answers) {
            match presentation_mode {
                Ok(value) => merged.presentation_mode = value,
                Err(error) => {
                    return canonical_cbor_bytes(&ApplyAnswersResult {
                        ok: false,
                        config: None,
                        secrets_patch: None,
                        remove: None,
                        diagnostics: Vec::new(),
                        error: Some(error),
                    });
                }
            }
        }
        merged.skin = string_or_default(&answers, "skin", &merged.skin);
        merged.text_input_enabled = answers
            .get("text_input_enabled")
            .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
            .unwrap_or(merged.text_input_enabled);
        merged.nav_links = nav_links_from_answers(&answers).unwrap_or(merged.nav_links);
    }

    if mode == Mode::Upgrade {
        if has("enabled") {
            merged.enabled = answers
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(merged.enabled);
        }
        if has("public_base_url") {
            merged.public_base_url =
                string_or_default(&answers, "public_base_url", &merged.public_base_url);
        }
        if has("mode") {
            merged.mode = string_or_default(&answers, "mode", &merged.mode);
        }
        if has("route") {
            merged.route = optional_string_from(&answers, "route");
        }
        if has("tenant_channel_id") {
            merged.tenant_channel_id = optional_string_from(&answers, "tenant_channel_id");
        }
        if has("base_url") {
            merged.base_url = optional_string_from(&answers, "base_url");
        }
        if has("jwt_signing_key") {
            if let Some(jwt_key) = optional_string_from(&answers, "jwt_signing_key") {
                secrets_set.insert("jwt_signing_key".to_string(), jwt_key);
            }
            merged.jwt_signing_key_b64 = None;
        } else if let Some(jwt_key) = take_existing_jwt_key(&mut merged) {
            secrets_set.insert("jwt_signing_key".to_string(), jwt_key);
        } else {
            secrets_set.insert("jwt_signing_key".to_string(), generate_secret_20());
        }
        if has("oauth_enabled") {
            merged.oauth_enabled = answers
                .get("oauth_enabled")
                .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")));
        }
        if has("oauth_enable_greentic")
            || has("oauth_enable_google")
            || has("oauth_enable_microsoft")
            || has("oauth_enable_github")
            || has("oauth_enable_custom")
            || has("oauth_providers")
        {
            merged.oauth_providers = compose_oauth_providers(&answers);
        }
        if has("presentation_mode") {
            match presentation_mode_from_answers(&answers) {
                Some(Ok(value)) => merged.presentation_mode = value,
                Some(Err(error)) => {
                    return canonical_cbor_bytes(&ApplyAnswersResult {
                        ok: false,
                        config: None,
                        secrets_patch: None,
                        remove: None,
                        diagnostics: Vec::new(),
                        error: Some(error),
                    });
                }
                None => {}
            }
        }
        if has("skin") {
            merged.skin = string_or_default(&answers, "skin", &merged.skin);
        }
        if has("text_input_enabled") {
            merged.text_input_enabled = answers
                .get("text_input_enabled")
                .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
                .unwrap_or(merged.text_input_enabled);
        }
        if has("nav_links") {
            merged.nav_links = nav_links_from_answers(&answers).unwrap_or_default();
        }
    }

    if has("oauth_greentic_issuer") {
        merged.oidc_issuer = optional_string_from(&answers, "oauth_greentic_issuer");
    }
    if has("oauth_greentic_client_id") {
        merged.oidc_audience = optional_string_from(&answers, "oauth_greentic_client_id");
    }

    normalize_nav_links(&mut merged);

    if let Err(error) = validate_config_out(&merged) {
        return canonical_cbor_bytes(&ApplyAnswersResult {
            ok: false,
            config: None,
            secrets_patch: None,
            remove: None,
            diagnostics: Vec::new(),
            error: Some(error),
        });
    }

    canonical_cbor_bytes(&ApplyAnswersResult {
        ok: true,
        config: Some(merged),
        secrets_patch: (!secrets_set.is_empty()).then_some(SecretsPatch {
            set: secrets_set,
            delete: Vec::new(),
        }),
        remove: None,
        diagnostics: Vec::new(),
        error: None,
    })
}

fn existing_config_from_answers(answers: &Value) -> Option<ProviderConfigOut> {
    answers
        .get("existing_config")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
}

fn string_or_default(answers: &Value, key: &str, fallback: &str) -> String {
    answers
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| fallback.to_string())
}

fn optional_string_from(answers: &Value, key: &str) -> Option<String> {
    answers
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn generate_secret_20() -> String {
    Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(20)
        .collect()
}

fn take_existing_jwt_key(config: &mut ProviderConfigOut) -> Option<String> {
    let encoded = config.jwt_signing_key_b64.take()?;
    general_purpose::STANDARD
        .decode(encoded)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn presentation_mode_from_answers(answers: &Value) -> Option<Result<PresentationMode, String>> {
    let value = optional_string_from(answers, "presentation_mode")?;
    Some(match value.as_str() {
        "standalone" => Ok(PresentationMode::Standalone),
        "embed_webcomponent" => Ok(PresentationMode::EmbedWebcomponent),
        _ => Err(format!(
            "config validation failed: presentation_mode must be standalone|embed_webcomponent, got {value}"
        )),
    })
}

fn nav_links_from_answers(answers: &Value) -> Option<Vec<Value>> {
    answers.get("nav_links").and_then(|value| match value {
        Value::Array(items) => Some(items.clone()),
        _ => None,
    })
}

fn is_truthy(answers: &Value, key: &str) -> bool {
    answers
        .get(key)
        .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
        .unwrap_or(false)
}

/// Compose `oauth_providers` JSON array from individual per-provider answers.
/// Falls back to raw `oauth_providers` field if present (for direct JSON input).
fn compose_oauth_providers(answers: &Value) -> Option<String> {
    let mut providers: Vec<Value> = Vec::new();

    // Greentic SSO (pushed first so it renders as the first login button)
    if is_truthy(answers, "oauth_enable_greentic") {
        let mut p = json!({
            "id": "greentic",
            "label": "Greentic SSO",
            "type": "greentic",
            "client_id": optional_string_from(answers, "oauth_greentic_client_id")
                .unwrap_or_else(|| "webchat-gui".to_string()),
            "scopes": "openid profile email greentic.webchat"
        });
        if let Some(issuer) = optional_string_from(answers, "oauth_greentic_issuer") {
            p["issuer"] = Value::String(issuer);
        }
        providers.push(p);
    }

    // Google (well-known URLs)
    if is_truthy(answers, "oauth_enable_google")
        && let Some(client_id) = optional_string_from(answers, "oauth_google_client_id")
    {
        let mut p = json!({
            "id": "google", "label": "Google",
            "auth_url": "https://accounts.google.com/o/oauth2/v2/auth",
            "token_url": "https://oauth2.googleapis.com/token",
            "client_id": client_id, "scopes": "openid profile email"
        });
        if let Some(s) = optional_string_from(answers, "oauth_google_client_secret") {
            p["client_secret"] = Value::String(s);
        }
        providers.push(p);
    }

    // Microsoft (well-known URLs)
    if is_truthy(answers, "oauth_enable_microsoft")
        && let Some(client_id) = optional_string_from(answers, "oauth_microsoft_client_id")
    {
        let mut p = json!({
            "id": "microsoft", "label": "Microsoft",
            "auth_url": "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            "token_url": "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            "client_id": client_id, "scopes": "openid profile email"
        });
        if let Some(s) = optional_string_from(answers, "oauth_microsoft_client_secret") {
            p["client_secret"] = Value::String(s);
        }
        providers.push(p);
    }

    // GitHub (well-known URLs)
    if is_truthy(answers, "oauth_enable_github")
        && let Some(client_id) = optional_string_from(answers, "oauth_github_client_id")
    {
        let mut p = json!({
            "id": "github", "label": "GitHub",
            "auth_url": "https://github.com/login/oauth/authorize",
            "token_url": "https://github.com/login/oauth/access_token",
            "client_id": client_id, "scopes": "read:user user:email"
        });
        if let Some(s) = optional_string_from(answers, "oauth_github_client_secret") {
            p["client_secret"] = Value::String(s);
        }
        providers.push(p);
    }

    // Custom OIDC (user-provided URLs)
    if is_truthy(answers, "oauth_enable_custom")
        && let Some(client_id) = optional_string_from(answers, "oauth_custom_client_id")
    {
        let label = optional_string_from(answers, "oauth_custom_label")
            .unwrap_or_else(|| "SSO".to_string());
        let auth_url = optional_string_from(answers, "oauth_custom_auth_url").unwrap_or_default();
        let token_url = optional_string_from(answers, "oauth_custom_token_url").unwrap_or_default();
        let scopes = optional_string_from(answers, "oauth_custom_scopes")
            .unwrap_or_else(|| "openid profile email".to_string());
        providers.push(json!({
            "id": "custom",
            "label": label,
            "auth_url": auth_url,
            "token_url": token_url,
            "client_id": client_id,
            "scopes": scopes
        }));
    }

    // If per-provider fields produced results, use them
    if !providers.is_empty() {
        return Some(Value::Array(providers).to_string());
    }

    // Fallback: raw oauth_providers field (JSON string or array)
    answers.get("oauth_providers").and_then(|val| match val {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Array(_) => Some(val.to_string()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use provider_common::component_v0_6::{canonical_cbor_bytes, decode_cbor};
    use serde_json::json;

    fn apply_setup(answers: Value) -> Value {
        let cbor = canonical_cbor_bytes(&answers);
        let out = apply_answers_impl(
            bindings::exports::greentic::component::qa::Mode::Setup,
            cbor,
        );
        decode_cbor(&out).expect("decode apply answers")
    }

    fn error_text(value: &Value) -> String {
        value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn apply_answers_defaults_to_standalone() {
        let value = apply_setup(json!({
            "public_base_url": "https://chat.example.com",
            "route": "webchat",
            "skin": "default"
        }));
        assert_eq!(value["ok"], true);
        assert_eq!(value["config"]["mode"], "websocket");
        assert_eq!(value["config"]["presentation_mode"], "standalone");
        assert_eq!(value["config"]["skin"], "default");
        assert_eq!(value["config"]["text_input_enabled"], true);
        assert!(value["config"].get("jwt_signing_key_b64").is_none());
        let generated = value["secrets_patch"]["set"]["jwt_signing_key"]
            .as_str()
            .expect("generated jwt signing key");
        assert_eq!(generated.len(), 20);
    }

    #[test]
    fn describe_hides_generated_jwt_signing_key() {
        let payload = build_gui_describe_payload();
        let fields = match payload.config_schema {
            SchemaIr::Object { fields, .. } => fields,
            _ => panic!("config schema should be an object"),
        };
        assert!(!fields.contains_key("jwt_signing_key"));
        assert!(
            !payload
                .redactions
                .iter()
                .any(|rule| rule.path == "$.jwt_signing_key")
        );
        assert!(
            !filtered_i18n_keys()
                .iter()
                .any(|key| key.contains("jwt_signing_key"))
        );
    }

    #[test]
    fn apply_answers_migrates_existing_jwt_key_to_secret_patch() {
        let value = apply_setup(json!({
            "existing_config": {
                "enabled": true,
                "public_base_url": "https://chat.example.com",
                "mode": "local_queue",
                "route": "webchat",
                "skin": "default",
                "jwt_signing_key_b64": general_purpose::STANDARD.encode("existing-secret")
            },
            "public_base_url": "https://chat.example.com",
            "mode": "local_queue",
            "route": "webchat"
        }));

        assert_eq!(value["ok"], true);
        assert!(value["config"].get("jwt_signing_key_b64").is_none());
        assert_eq!(
            value["secrets_patch"]["set"]["jwt_signing_key"],
            Value::String("existing-secret".to_string())
        );
    }

    #[test]
    fn apply_answers_embed_mode_does_not_require_nav_links() {
        let value = apply_setup(json!({
            "public_base_url": "https://chat.example.com",
            "mode": "local_queue",
            "route": "webchat",
            "presentation_mode": "embed_webcomponent",
            "skin": "default",
            "text_input_enabled": false
        }));
        assert_eq!(value["ok"], true);
        assert_eq!(value["config"]["presentation_mode"], "embed_webcomponent");
        assert_eq!(value["config"]["text_input_enabled"], false);
        assert!(value["config"].get("nav_links").is_none());
    }

    #[test]
    fn validate_config_rejects_unknown_presentation_mode() {
        let out = <Component as bindings::exports::greentic::provider_schema_core::schema_core_api::Guest>::validate_config(
            serde_json::to_vec(&json!({
                "enabled": true,
                "public_base_url": "https://chat.example.com",
                "mode": "local_queue",
                "route": "webchat",
                "presentation_mode": "modal"
            }))
            .unwrap(),
        );
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["ok"], false);
    }

    #[test]
    fn apply_answers_rejects_unknown_presentation_mode() {
        let value = apply_setup(json!({
            "public_base_url": "https://chat.example.com",
            "mode": "local_queue",
            "route": "webchat",
            "presentation_mode": "skin"
        }));
        assert_eq!(value["ok"], false);
        assert!(error_text(&value).contains("presentation_mode"));
    }

    #[test]
    fn greentic_sso_is_the_first_composed_provider() {
        let answers = json!({
            "oauth_enabled": true,
            "oauth_enable_greentic": true,
            "oauth_greentic_issuer": "https://acme.greentic-id.com",
            "oauth_enable_google": true,
            "oauth_google_client_id": "google-client",
        });
        let composed = compose_oauth_providers(&answers).expect("providers composed");
        let parsed: Value = serde_json::from_str(&composed).expect("valid json");
        let list = parsed.as_array().expect("array");
        assert_eq!(list[0]["id"], json!("greentic"));
        assert_eq!(list[0]["type"], json!("greentic"));
        assert_eq!(list[0]["client_id"], json!("webchat-gui"));
        assert_eq!(list[0]["issuer"], json!("https://acme.greentic-id.com"));
        assert_eq!(list[1]["id"], json!("google"));
    }
}
