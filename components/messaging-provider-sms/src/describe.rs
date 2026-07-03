use crate::{PROVIDER_ID, PROVIDER_TYPE, WORLD_ID};
use provider_common::component_v0_6::{
    OperationDescriptor, RedactionRule, SchemaIr, SecretRequirement, schema_hash,
};
use provider_common::helpers::{op, schema_bool_ir, schema_obj, schema_secret, schema_str};
use serde::Serialize;

pub(crate) const I18N_KEYS: &[&str] = &[
    "sms.op.ingest_http.title",
    "sms.op.ingest_http.description",
    "sms.op.render_plan.title",
    "sms.op.render_plan.description",
    "sms.op.encode.title",
    "sms.op.encode.description",
    "sms.op.send_payload.title",
    "sms.op.send_payload.description",
    "sms.op.setup_webhook.title",
    "sms.op.setup_webhook.description",
    "sms.schema.input.title",
    "sms.schema.input.description",
    "sms.schema.input.message.title",
    "sms.schema.input.message.description",
    "sms.schema.output.title",
    "sms.schema.output.description",
    "sms.schema.output.ok.title",
    "sms.schema.output.ok.description",
    "sms.schema.output.message_id.title",
    "sms.schema.output.message_id.description",
    "sms.schema.config.title",
    "sms.schema.config.description",
    "sms.schema.config.enabled.title",
    "sms.schema.config.enabled.description",
    "sms.schema.config.account_sid.title",
    "sms.schema.config.account_sid.description",
    "sms.schema.config.auth_token.title",
    "sms.schema.config.auth_token.description",
    "sms.schema.config.from_number.title",
    "sms.schema.config.from_number.description",
    "sms.qa.default.title",
    "sms.qa.setup.title",
    "sms.qa.upgrade.title",
    "sms.qa.remove.title",
    "sms.qa.setup.enabled",
    "sms.qa.setup.account_sid",
    "sms.qa.setup.auth_token",
    "sms.qa.setup.from_number",
];

pub(crate) const SETUP_QUESTIONS: &[provider_common::helpers::QaQuestionDef] = &[
    ("account_sid", "sms.qa.setup.account_sid", true),
    ("auth_token", "sms.qa.setup.auth_token", true),
    ("from_number", "sms.qa.setup.from_number", true),
];

pub(crate) const DEFAULT_KEYS: &[&str] = &["account_sid", "auth_token", "from_number"];

/// Local superset of `provider_common::component_v0_6::DescribePayload` that
/// also carries `provider_type` + `secret_requirements` — fields the shared
/// struct does not yet expose (extending it there would break every other
/// provider's struct-literal `describe()` builder). Task 6 of this epic
/// generates `component.manifest.json` from this shape.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SmsDescribePayload {
    pub provider: String,
    pub world: String,
    pub provider_type: String,
    pub operations: Vec<OperationDescriptor>,
    pub input_schema: SchemaIr,
    pub output_schema: SchemaIr,
    pub config_schema: SchemaIr,
    pub redactions: Vec<RedactionRule>,
    pub secret_requirements: Vec<SecretRequirement>,
    pub schema_hash: String,
}

pub(crate) fn build_describe_payload() -> SmsDescribePayload {
    let input_schema = input_schema();
    let output_schema = output_schema();
    let config_schema = config_schema();
    SmsDescribePayload {
        provider: PROVIDER_ID.to_string(),
        world: WORLD_ID.to_string(),
        provider_type: PROVIDER_TYPE.to_string(),
        operations: vec![
            op(
                "ingest_http",
                "sms.op.ingest_http.title",
                "sms.op.ingest_http.description",
            ),
            op(
                "render_plan",
                "sms.op.render_plan.title",
                "sms.op.render_plan.description",
            ),
            op(
                "encode",
                "sms.op.encode.title",
                "sms.op.encode.description",
            ),
            op(
                "send_payload",
                "sms.op.send_payload.title",
                "sms.op.send_payload.description",
            ),
            op(
                "setup_webhook",
                "sms.op.setup_webhook.title",
                "sms.op.setup_webhook.description",
            ),
        ],
        input_schema: input_schema.clone(),
        output_schema: output_schema.clone(),
        config_schema: config_schema.clone(),
        redactions: vec![
            RedactionRule {
                path: "$.account_sid".to_string(),
                strategy: "replace".to_string(),
            },
            RedactionRule {
                path: "$.auth_token".to_string(),
                strategy: "replace".to_string(),
            },
        ],
        secret_requirements: vec![
            SecretRequirement {
                name: "TWILIO_ACCOUNT_SID".to_string(),
                scope: "tenant".to_string(),
                description: "Twilio Account SID used to authenticate REST API calls."
                    .to_string(),
            },
            SecretRequirement {
                name: "TWILIO_AUTH_TOKEN".to_string(),
                scope: "tenant".to_string(),
                description:
                    "Twilio Auth Token used for REST API calls and inbound webhook signature validation."
                        .to_string(),
            },
            SecretRequirement {
                name: "TWILIO_FROM_NUMBER".to_string(),
                scope: "tenant".to_string(),
                description: "Twilio phone number (E.164) used as the sender for outbound SMS."
                    .to_string(),
            },
        ],
        schema_hash: schema_hash(&input_schema, &output_schema, &config_schema),
    }
}

pub(crate) fn build_qa_spec(
    mode: crate::bindings::exports::greentic::component::qa::Mode,
) -> provider_common::component_v0_6::QaSpec {
    use crate::bindings::exports::greentic::component::qa::Mode;
    let mode_str = match mode {
        Mode::Default => "default",
        Mode::Setup => "setup",
        Mode::Upgrade => "upgrade",
        Mode::Remove => "remove",
    };
    provider_common::helpers::qa_spec_for_mode(mode_str, "sms", SETUP_QUESTIONS, DEFAULT_KEYS)
}

pub(crate) const I18N_PAIRS: &[(&str, &str)] = &[
    ("sms.op.ingest_http.title", "Ingest HTTP"),
    (
        "sms.op.ingest_http.description",
        "Normalize an inbound Twilio SMS webhook payload",
    ),
    ("sms.op.render_plan.title", "Render Plan"),
    (
        "sms.op.render_plan.description",
        "Render universal message plan",
    ),
    ("sms.op.encode.title", "Encode"),
    (
        "sms.op.encode.description",
        "Encode universal payload for the Twilio Messages API",
    ),
    ("sms.op.send_payload.title", "Send Payload"),
    (
        "sms.op.send_payload.description",
        "Send encoded payload to the Twilio Messages API",
    ),
    ("sms.op.setup_webhook.title", "Setup Webhook"),
    (
        "sms.op.setup_webhook.description",
        "Register the tenant's inbound SMS webhook URL with Twilio",
    ),
    ("sms.schema.input.title", "SMS input"),
    ("sms.schema.input.description", "Input for SMS operations"),
    ("sms.schema.input.message.title", "Message"),
    ("sms.schema.input.message.description", "Message text"),
    ("sms.schema.output.title", "SMS output"),
    (
        "sms.schema.output.description",
        "Result of an SMS operation",
    ),
    ("sms.schema.output.ok.title", "Success"),
    (
        "sms.schema.output.ok.description",
        "Whether operation succeeded",
    ),
    ("sms.schema.output.message_id.title", "Message ID"),
    (
        "sms.schema.output.message_id.description",
        "Twilio message SID",
    ),
    ("sms.schema.config.title", "SMS config"),
    (
        "sms.schema.config.description",
        "SMS (Twilio) provider configuration",
    ),
    ("sms.schema.config.enabled.title", "Enabled"),
    (
        "sms.schema.config.enabled.description",
        "Enable this provider",
    ),
    ("sms.schema.config.account_sid.title", "Account SID"),
    (
        "sms.schema.config.account_sid.description",
        "Twilio Account SID",
    ),
    ("sms.schema.config.auth_token.title", "Auth token"),
    (
        "sms.schema.config.auth_token.description",
        "Twilio Auth Token",
    ),
    ("sms.schema.config.from_number.title", "From number"),
    (
        "sms.schema.config.from_number.description",
        "Twilio phone number (E.164) used as the sender",
    ),
    ("sms.qa.default.title", "Default"),
    ("sms.qa.setup.title", "Setup"),
    ("sms.qa.upgrade.title", "Upgrade"),
    ("sms.qa.remove.title", "Remove"),
    ("sms.qa.setup.enabled", "Enable provider"),
    ("sms.qa.setup.account_sid", "Account SID"),
    ("sms.qa.setup.auth_token", "Auth token"),
    ("sms.qa.setup.from_number", "From number"),
];

fn input_schema() -> SchemaIr {
    schema_obj(
        "sms.schema.input.title",
        "sms.schema.input.description",
        vec![(
            "message",
            true,
            schema_str(
                "sms.schema.input.message.title",
                "sms.schema.input.message.description",
            ),
        )],
        true,
    )
}

fn output_schema() -> SchemaIr {
    schema_obj(
        "sms.schema.output.title",
        "sms.schema.output.description",
        vec![
            (
                "ok",
                true,
                schema_bool_ir(
                    "sms.schema.output.ok.title",
                    "sms.schema.output.ok.description",
                ),
            ),
            (
                "message_id",
                false,
                schema_str(
                    "sms.schema.output.message_id.title",
                    "sms.schema.output.message_id.description",
                ),
            ),
        ],
        true,
    )
}

fn config_schema() -> SchemaIr {
    schema_obj(
        "sms.schema.config.title",
        "sms.schema.config.description",
        vec![
            (
                "enabled",
                true,
                schema_bool_ir(
                    "sms.schema.config.enabled.title",
                    "sms.schema.config.enabled.description",
                ),
            ),
            (
                "account_sid",
                true,
                schema_secret(
                    "sms.schema.config.account_sid.title",
                    "sms.schema.config.account_sid.description",
                ),
            ),
            (
                "auth_token",
                true,
                schema_secret(
                    "sms.schema.config.auth_token.title",
                    "sms.schema.config.auth_token.description",
                ),
            ),
            (
                "from_number",
                true,
                schema_secret(
                    "sms.schema.config.from_number.title",
                    "sms.schema.config.from_number.description",
                ),
            ),
        ],
        false,
    )
}
