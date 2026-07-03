use base64::{Engine as _, engine::general_purpose};
use greentic_types::messaging::universal_dto::{Header, HttpInV1, HttpOutV1};
use greentic_types::{
    Actor, ChannelMessageEnvelope, Destination, EnvId, MessageMetadata, TenantCtx, TenantId,
};
use provider_common::http_compat::{
    http_out_error, http_out_v1_bytes, parse_operator_http_in_with_config,
};
use std::collections::BTreeMap;

use super::signature::valid_twilio_signature;

pub(crate) fn ingest_http(input_json: &[u8]) -> Vec<u8> {
    ingest_http_with_auth_token(input_json, resolve_auth_token().as_deref())
}

fn ingest_http_with_auth_token(input_json: &[u8], auth_token: Option<&str>) -> Vec<u8> {
    // Try native greentic-types format first, fall back to operator format
    let request = match serde_json::from_slice::<HttpInV1>(input_json) {
        Ok(req) => req,
        Err(_) => match parse_operator_http_in_with_config(input_json) {
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

    let body_bytes = match general_purpose::STANDARD.decode(&request.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => return http_out_error(400, &format!("invalid body encoding: {err}")),
    };
    let body = match String::from_utf8(body_bytes) {
        Ok(s) => s,
        Err(err) => return http_out_error(400, &format!("invalid body utf8: {err}")),
    };

    let form = parse_form_urlencoded(&body);

    if !signature_ok(&request, &form, auth_token) {
        return http_out_error(403, "invalid twilio signature");
    }

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
    // Per-sender session so distinct phone numbers texting one Twilio number
    // don't collide into a shared conversation (session key = pack:flow:session_id).
    let session_id = from.clone();
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
        session_id,
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
/// A `BTreeMap` (sorted by key) so the same map doubles as the Twilio signature's
/// sorted-param input without a second pass.
fn parse_form_urlencoded(body: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
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

/// Fail-closed signature check: requires an injected auth token, a signed URL
/// reconstructable from the request, and a matching `X-Twilio-Signature` header.
/// Any of the three being unavailable is treated as an invalid signature.
fn signature_ok(
    request: &HttpInV1,
    form: &BTreeMap<String, String>,
    auth_token: Option<&str>,
) -> bool {
    let Some(token) = auth_token else {
        return false;
    };
    let Some(header_sig) = find_header(&request.headers, "x-twilio-signature") else {
        return false;
    };
    let Some(url) = signed_url(request) else {
        return false;
    };
    valid_twilio_signature(token, &url, form, header_sig.trim())
}

fn find_header(headers: &[Header], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.clone())
}

/// Twilio signs the exact public URL it POSTed to. v1 reconstructs it from the
/// `X-Forwarded-Host`/`Host` request header (set by the operator front door) plus
/// the ingress path; if the host was proxied through something that injects a
/// `public_base_url` into the request config instead, that is used as the base.
/// If neither is present the signed URL can't be reconstructed, so the request
/// fails closed rather than skip verification (see design spec §4.1).
fn signed_url(request: &HttpInV1) -> Option<String> {
    let base = resolve_base_url(request)?;
    let mut url = format!("{base}{}", request.path);
    if let Some(query) = request.query.as_deref().filter(|q| !q.is_empty()) {
        url.push('?');
        url.push_str(query);
    }
    Some(url)
}

fn resolve_base_url(request: &HttpInV1) -> Option<String> {
    if let Some(host) = find_header(&request.headers, "x-forwarded-host")
        .or_else(|| find_header(&request.headers, "host"))
    {
        return Some(format!("https://{host}"));
    }
    request
        .config
        .as_ref()
        .and_then(|cfg| cfg.get("public_base_url"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim_end_matches('/').to_string())
}

#[cfg(not(test))]
fn resolve_auth_token() -> Option<String> {
    use crate::bindings::greentic::secrets_store::secrets_store;
    match secrets_store::get(crate::AUTH_TOKEN_KEY) {
        Ok(Some(bytes)) => String::from_utf8(bytes).ok(),
        _ => None,
    }
}

#[cfg(test)]
fn resolve_auth_token() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_AUTH_TOKEN: &str = "test-twilio-auth-token";
    const TEST_PATH: &str = "/v1/messaging/ingress/sms/default";
    const TEST_HOST: &str = "example.com";

    fn http_in_with_headers(body: &str, headers: &[(&str, &str)]) -> Vec<u8> {
        let header_values = headers
            .iter()
            .map(|(name, value)| json!({"name": name, "value": value}))
            .collect::<Vec<_>>();
        let http_in = json!({
            "method": "POST",
            "path": TEST_PATH,
            "body_b64": general_purpose::STANDARD.encode(body.as_bytes()),
            "headers": header_values,
            "query": null,
        });
        serde_json::to_vec(&http_in).expect("serialize http_in")
    }

    fn signed_signature_for(body: &str) -> String {
        let url = format!("https://{TEST_HOST}{TEST_PATH}");
        let params = parse_form_urlencoded(body);
        super::super::signature::sign_for_test(TEST_AUTH_TOKEN, &url, &params)
    }

    /// A request Twilio would actually send: valid `Host` + `X-Twilio-Signature`
    /// computed over the reconstructed URL and the body's own params.
    fn signed_http_in_with_body(body: &str) -> Vec<u8> {
        let sig = signed_signature_for(body);
        http_in_with_headers(body, &[("Host", TEST_HOST), ("X-Twilio-Signature", &sig)])
    }

    fn decode_out(bytes: &[u8]) -> HttpOutV1 {
        serde_json::from_slice(bytes).expect("valid HttpOutV1")
    }

    #[test]
    fn parses_twilio_inbound_form_into_channel_message() {
        let body = "From=%2B15551230001&To=%2B15559990000&Body=hello+agent&MessageSid=SM123&NumMedia=0&AccountSid=AC1";
        let out = decode_out(&ingest_http_with_auth_token(
            &signed_http_in_with_body(body),
            Some(TEST_AUTH_TOKEN),
        ));
        assert_eq!(out.status, 200);
        assert_eq!(out.events.len(), 1);
        let env = &out.events[0];
        assert_eq!(env.channel, "sms");
        assert_eq!(env.text.as_deref(), Some("hello agent"));
        assert_eq!(env.correlation_id.as_deref(), Some("SM123"));
        assert!(env.from.as_ref().is_some_and(|a| a.id == "+15551230001"));
        // Per-sender session isolation: distinct senders must not share a session.
        assert_eq!(env.session_id, "+15551230001");
        assert_eq!(env.to.len(), 1);
        assert_eq!(env.to[0].id, "+15559990000");
        assert!(env.attachments.is_empty());
    }

    #[test]
    fn malformed_body_returns_400_no_events() {
        let out = decode_out(&ingest_http_with_auth_token(
            &signed_http_in_with_body("%%%not-a-form"),
            Some(TEST_AUTH_TOKEN),
        ));
        assert_eq!(out.status, 400);
        assert!(out.events.is_empty());
    }

    #[test]
    fn mms_drops_media_keeps_text_with_note() {
        let body = "From=%2B15551230001&To=%2B15559990000&Body=pic&MessageSid=SM9&NumMedia=1&MediaUrl0=https%3A%2F%2Fx";
        let out = decode_out(&ingest_http_with_auth_token(
            &signed_http_in_with_body(body),
            Some(TEST_AUTH_TOKEN),
        ));
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
            "path": TEST_PATH,
            "body_b64": "",
            "headers": [],
            "query": null,
        });
        let input = serde_json::to_vec(&http_in).unwrap();
        let out = decode_out(&ingest_http(&input));
        assert_eq!(out.status, 200);
        assert!(out.events.is_empty());
    }

    #[test]
    fn tampered_signature_returns_403_no_events() {
        let body = "From=%2B15551230001&To=%2B15559990000&Body=hi&MessageSid=SM1";
        let input = http_in_with_headers(
            body,
            &[("Host", TEST_HOST), ("X-Twilio-Signature", "tampered==")],
        );
        let out = decode_out(&ingest_http_with_auth_token(&input, Some(TEST_AUTH_TOKEN)));
        assert_eq!(out.status, 403);
        assert!(out.events.is_empty());
    }

    #[test]
    fn missing_signature_header_returns_403_no_events() {
        let body = "From=%2B15551230001&To=%2B15559990000&Body=hi&MessageSid=SM1";
        let input = http_in_with_headers(body, &[("Host", TEST_HOST)]);
        let out = decode_out(&ingest_http_with_auth_token(&input, Some(TEST_AUTH_TOKEN)));
        assert_eq!(out.status, 403);
        assert!(out.events.is_empty());
    }

    #[test]
    fn missing_auth_token_fails_closed_even_with_a_correctly_signed_request() {
        let body = "From=%2B15551230001&To=%2B15559990000&Body=hi&MessageSid=SM1";
        let out = decode_out(&ingest_http_with_auth_token(
            &signed_http_in_with_body(body),
            None,
        ));
        assert_eq!(out.status, 403);
        assert!(out.events.is_empty());
    }

    #[test]
    fn missing_host_header_cannot_reconstruct_url_and_fails_closed() {
        let body = "From=%2B15551230001&To=%2B15559990000&Body=hi&MessageSid=SM1";
        let sig = signed_signature_for(body);
        let input = http_in_with_headers(body, &[("X-Twilio-Signature", &sig)]);
        let out = decode_out(&ingest_http_with_auth_token(&input, Some(TEST_AUTH_TOKEN)));
        assert_eq!(out.status, 403);
        assert!(out.events.is_empty());
    }

    #[test]
    fn public_entrypoint_fails_closed_when_no_secret_is_injected() {
        // resolve_auth_token() is stubbed to `None` under cfg(test), mirroring
        // production behavior when TWILIO_AUTH_TOKEN was never provisioned.
        let body = "From=%2B15551230001&To=%2B15559990000&Body=hi&MessageSid=SM1";
        let out = decode_out(&ingest_http(&signed_http_in_with_body(body)));
        assert_eq!(out.status, 403);
        assert!(out.events.is_empty());
    }
}
