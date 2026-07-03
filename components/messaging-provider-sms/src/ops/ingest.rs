use base64::{Engine as _, engine::general_purpose};
use greentic_types::messaging::universal_dto::{HttpInV1, HttpOutV1};
use greentic_types::{
    Actor, ChannelMessageEnvelope, Destination, EnvId, MessageMetadata, TenantCtx, TenantId,
};
use provider_common::http_compat::{http_out_error, http_out_v1_bytes, parse_operator_http_in};
use std::collections::HashMap;

/// Twilio signs each inbound request (`X-Twilio-Signature`); verification is wired
/// in separately. Fail-open (`true`) until then so this stays the single call site.
fn signature_ok(_http: &HttpInV1) -> bool {
    true
}

pub(crate) fn ingest_http(input_json: &[u8]) -> Vec<u8> {
    // Try native greentic-types format first, fall back to operator format
    let request = match serde_json::from_slice::<HttpInV1>(input_json) {
        Ok(req) => req,
        Err(_) => match parse_operator_http_in(input_json) {
            Ok(req) => req,
            Err(err) => return http_out_error(400, &format!("invalid http input: {err}")),
        },
    };

    if request.method.eq_ignore_ascii_case("GET") {
        let out = HttpOutV1 {
            status: 200,
            headers: Vec::new(),
            body_b64: String::new(),
            events: Vec::new(),
        };
        return http_out_v1_bytes(&out);
    }

    if !signature_ok(&request) {
        return http_out_error(403, "invalid twilio signature");
    }

    let body_bytes = match general_purpose::STANDARD.decode(&request.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => return http_out_error(400, &format!("invalid body encoding: {err}")),
    };
    let body = match String::from_utf8(body_bytes) {
        Ok(s) => s,
        Err(err) => return http_out_error(400, &format!("invalid body utf8: {err}")),
    };

    let form = parse_form_urlencoded(&body);
    let (Some(from), Some(to), Some(message_sid)) = (
        form.get("From").cloned(),
        form.get("To").cloned(),
        form.get("MessageSid").cloned(),
    ) else {
        return http_out_error(
            400,
            "missing required Twilio SMS fields (From/To/MessageSid)",
        );
    };
    let text = form.get("Body").cloned().unwrap_or_default();
    let num_media: u32 = form
        .get("NumMedia")
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);

    let envelope = build_sms_envelope(from, to, text, message_sid, num_media);
    let out = HttpOutV1 {
        status: 200,
        headers: Vec::new(),
        body_b64: String::new(),
        events: vec![envelope],
    };
    http_out_v1_bytes(&out)
}

fn build_sms_envelope(
    from: String,
    to: String,
    text: String,
    message_sid: String,
    num_media: u32,
) -> ChannelMessageEnvelope {
    let env = EnvId::try_from("default").expect("env id");
    let tenant = TenantId::try_from("default").expect("tenant id");
    let mut metadata = MessageMetadata::new();
    metadata.insert("channel_id".to_string(), "sms".to_string());
    metadata.insert("from".to_string(), from.clone());
    metadata.insert("to_number".to_string(), to.clone());
    if num_media > 0 {
        metadata.insert("media_dropped".to_string(), num_media.to_string());
    }
    let sender = Actor {
        id: from,
        kind: Some("user".into()),
    };
    let destination = Destination {
        id: to,
        kind: Some("phone".into()),
    };
    ChannelMessageEnvelope {
        id: format!("sms-{message_sid}"),
        tenant: TenantCtx::new(env, tenant),
        channel: "sms".to_string(),
        session_id: "sms".to_string(),
        reply_scope: None,
        from: Some(sender),
        to: vec![destination],
        correlation_id: Some(message_sid),
        text: Some(text),
        attachments: Vec::new(),
        metadata,
        extensions: Default::default(),
    }
}

/// Decode a Twilio `application/x-www-form-urlencoded` body into key/value pairs.
/// Never fails: unparsable pairs (no `=`) are skipped, decode errors fall back to
/// the (space-normalized) raw text — callers reject on missing required keys instead.
fn parse_form_urlencoded(body: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        map.insert(decode_form_component(key), decode_form_component(value));
    }
    map
}

fn decode_form_component(raw: &str) -> String {
    let plus_decoded = raw.replace('+', " ");
    urlencoding::decode(&plus_decoded)
        .map(|cow| cow.into_owned())
        .unwrap_or(plus_decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn http_in_with_body(body: &str) -> Vec<u8> {
        let http_in = json!({
            "method": "POST",
            "path": "/v1/messaging/ingress/sms/default",
            "body_b64": general_purpose::STANDARD.encode(body.as_bytes()),
            "headers": [],
            "query": null,
        });
        serde_json::to_vec(&http_in).expect("serialize http_in")
    }

    fn decode_out(bytes: &[u8]) -> HttpOutV1 {
        serde_json::from_slice(bytes).expect("valid HttpOutV1")
    }

    #[test]
    fn parses_twilio_inbound_form_into_channel_message() {
        let body = "From=%2B15551230001&To=%2B15559990000&Body=hello+agent&MessageSid=SM123&NumMedia=0&AccountSid=AC1";
        let out = decode_out(&ingest_http(&http_in_with_body(body)));
        assert_eq!(out.status, 200);
        assert_eq!(out.events.len(), 1);
        let env = &out.events[0];
        assert_eq!(env.channel, "sms");
        assert_eq!(env.text.as_deref(), Some("hello agent"));
        assert_eq!(env.correlation_id.as_deref(), Some("SM123"));
        assert!(env.from.as_ref().is_some_and(|a| a.id == "+15551230001"));
        assert_eq!(env.to.len(), 1);
        assert_eq!(env.to[0].id, "+15559990000");
        assert!(env.attachments.is_empty());
    }

    #[test]
    fn malformed_body_returns_400_no_events() {
        let out = decode_out(&ingest_http(&http_in_with_body("%%%not-a-form")));
        assert_eq!(out.status, 400);
        assert!(out.events.is_empty());
    }

    #[test]
    fn mms_drops_media_keeps_text_with_note() {
        let body = "From=%2B15551230001&To=%2B15559990000&Body=pic&MessageSid=SM9&NumMedia=1&MediaUrl0=https%3A%2F%2Fx";
        let out = decode_out(&ingest_http(&http_in_with_body(body)));
        assert_eq!(out.status, 200);
        assert_eq!(out.events.len(), 1);
        let env = &out.events[0];
        assert_eq!(env.text.as_deref(), Some("pic"));
        assert!(env.attachments.is_empty(), "text-only v1");
        assert_eq!(
            env.metadata.get("media_dropped").map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn get_request_returns_200_with_no_events() {
        let http_in = json!({
            "method": "GET",
            "path": "/v1/messaging/ingress/sms/default",
            "body_b64": "",
            "headers": [],
            "query": null,
        });
        let input = serde_json::to_vec(&http_in).unwrap();
        let out = decode_out(&ingest_http(&input));
        assert_eq!(out.status, 200);
        assert!(out.events.is_empty());
    }
}
