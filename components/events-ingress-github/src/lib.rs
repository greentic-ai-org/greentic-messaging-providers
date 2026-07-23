mod bindings {
    wit_bindgen::generate!({
        path: "wit/events-ingress-github",
        world: "events-ingress-github",
        generate_all
    });
}

use bindings::exports::provider::common::ingress::Guest;
use bindings::greentic::secrets_store::secrets_store;
use serde_json::{Map, Value, json};

mod github_signature;

use github_signature::{GithubWebhookHeaders, verify_github_signature};

const WEBHOOK_SECRET_KEY: &str = "GITHUB_WEBHOOK_SECRET";
const PROVIDER_ID: &str = "events-ingress-github";
const EVENT_TYPE_PUSH: &str = "github.push";

struct Component;

impl Guest for Component {
    fn handle_webhook(headers_json: String, body_json: String) -> Result<String, String> {
        let headers: Map<String, Value> = serde_json::from_str(&headers_json)
            .map_err(|_| "validation error: invalid headers".to_string())?;

        let secret = match get_optional_secret(WEBHOOK_SECRET_KEY) {
            Some(result) => Some(result.map_err(|e| format!("transport error: {e}"))?),
            None => None,
        };

        process_webhook(&headers, &body_json, secret.as_deref())
    }
}

bindings::exports::provider::common::ingress::__export_provider_common_ingress_0_0_2_cabi!(
    Component with_types_in bindings::exports::provider::common::ingress
);

fn get_optional_secret(key: &str) -> Option<Result<String, String>> {
    match secrets_store::get(key) {
        Ok(Some(bytes)) => {
            Some(String::from_utf8(bytes).map_err(|_| "secret not valid utf-8".into()))
        }
        Ok(None) => None,
        Err(e) => Some(Err(format!("secret store error: {e:?}"))),
    }
}

/// Core webhook handler, isolated from the WASI secret store so it is unit
/// testable on the native target. When `secret` is `None` the HMAC check is
/// skipped (no webhook secret configured for this deployment).
fn process_webhook(
    headers: &Map<String, Value>,
    body_json: &str,
    secret: Option<&str>,
) -> Result<String, String> {
    let webhook_headers = GithubWebhookHeaders::from_map(headers);

    // A push carries the signature over the raw request body bytes.
    if let Some(secret) = secret {
        verify_github_signature(
            secret.as_bytes(),
            body_json.as_bytes(),
            &webhook_headers.signature_256,
        )?;
    }

    // Only `push` deliveries become `github.push` events. `ping` (sent when a
    // webhook is first registered) and every other event type are acknowledged
    // with an empty event list so GitHub still receives a 200.
    if webhook_headers.event != "push" {
        let ignored = json!({
            "ok": true,
            "status": 200,
            "events": [],
            "ignored_event": webhook_headers.event,
        });
        return serde_json::to_string(&ignored)
            .map_err(|_| "other error: serialization failed".to_string());
    }

    let payload: Value = serde_json::from_str(body_json)
        .map_err(|_| "validation error: invalid push payload json".to_string())?;

    let event = build_push_event(&payload, &webhook_headers);

    let normalized = json!({
        "ok": true,
        "status": 200,
        "events": [event],
    });
    serde_json::to_string(&normalized).map_err(|_| "other error: serialization failed".to_string())
}

/// Build a single `EventEnvelopeV1`-shaped event from a GitHub push payload.
///
/// Shape mirrors `greentic-start` `src/ingress_types.rs::EventEnvelopeV1`, the
/// events-domain ingress contract consumed by `parse_events` in
/// `ingress_dispatch.rs`. `scope.tenant` defaults to `"default"` because the
/// ingress extension ABI only forwards headers+body; the effective tenant/team
/// is resolved from the HTTP route by greentic-start's `event_router`.
fn build_push_event(payload: &Value, headers: &GithubWebhookHeaders) -> Value {
    let repo = payload
        .get("repository")
        .and_then(|repo| repo.get("full_name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let git_ref = payload
        .get("ref")
        .and_then(Value::as_str)
        .map(str::to_string);
    let commits = payload
        .get("commits")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let head_commit = payload.get("head_commit").cloned().unwrap_or(Value::Null);

    let occurred_at = head_commit
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();

    let delivery = if headers.delivery.trim().is_empty() {
        head_commit_id(&head_commit).unwrap_or_else(|| "unknown".to_string())
    } else {
        headers.delivery.clone()
    };

    let event_id = format!("github:{delivery}");

    json!({
        "event_id": event_id,
        "event_type": EVENT_TYPE_PUSH,
        "occurred_at": occurred_at,
        "source": {
            "domain": "events",
            "provider": PROVIDER_ID,
            "handler_id": "push",
        },
        "scope": {
            "tenant": "default",
        },
        "correlation_id": delivery,
        "payload": {
            "repo": repo,
            "ref": git_ref,
            "commits": commits,
            "head_commit": head_commit,
        },
    })
}

fn head_commit_id(head_commit: &Value) -> Option<String> {
    head_commit
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use github_signature::hmac_sha256_hex;
    use serde::Deserialize;

    // Local mirror of greentic-start `src/ingress_types.rs::EventEnvelopeV1`,
    // used to prove the emitted JSON deserializes into the exact ingress
    // contract that `parse_events` consumes.
    #[derive(Debug, Deserialize)]
    struct EventSourceV1 {
        domain: String,
        provider: String,
        #[allow(dead_code)]
        handler_id: Option<String>,
    }
    #[derive(Debug, Deserialize)]
    struct EventScopeV1 {
        tenant: String,
        #[allow(dead_code)]
        team: Option<String>,
    }
    #[derive(Debug, Deserialize)]
    struct EventEnvelopeV1 {
        event_id: String,
        event_type: String,
        #[allow(dead_code)]
        occurred_at: String,
        source: EventSourceV1,
        scope: EventScopeV1,
        correlation_id: Option<String>,
        payload: Value,
        #[allow(dead_code)]
        http: Option<Value>,
        #[allow(dead_code)]
        raw: Option<String>,
    }

    fn header_map(pairs: &[(&str, &str)]) -> Map<String, Value> {
        let mut headers = Map::new();
        for (key, value) in pairs {
            headers.insert((*key).to_string(), Value::String((*value).to_string()));
        }
        headers
    }

    fn sample_push_body() -> &'static str {
        r#"{
            "ref": "refs/heads/main",
            "repository": {"full_name": "octo/hello"},
            "head_commit": {"id": "abc123", "timestamp": "2026-07-23T10:00:00Z", "message": "init"},
            "commits": [
                {"id": "abc123", "message": "init"}
            ]
        }"#
    }

    #[test]
    fn signature_verification_accepts_correct_and_rejects_wrong() {
        // Known key + body. Expected signature is the value produced by:
        //   printf '%s' "<body>" | openssl dgst -sha256 -hmac "<key>"
        let key = b"It's a Secret to Everybody";
        let body = b"Hello, World!";
        // openssl dgst -sha256 -hmac "It's a Secret to Everybody" over "Hello, World!"
        let expected = "757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
        let signature = format!("sha256={expected}");

        // Cross-check our HMAC helper against the known vector.
        assert_eq!(hmac_sha256_hex(key, body), expected);

        verify_github_signature(key, body, &signature).expect("valid signature accepted");

        let wrong = format!("sha256={}", "0".repeat(64));
        let err = verify_github_signature(key, body, &wrong).expect_err("wrong signature rejected");
        assert!(err.contains("signature"), "{err}");

        let malformed = verify_github_signature(key, body, "deadbeef")
            .expect_err("missing sha256= prefix rejected");
        assert!(malformed.contains("sha256="), "{malformed}");
    }

    #[test]
    fn push_payload_parses_into_github_push_event() {
        let body = sample_push_body();
        // Signature computed over the exact body with a known key.
        let key = "test-secret";
        let signature = format!(
            "sha256={}",
            hmac_sha256_hex(key.as_bytes(), body.as_bytes())
        );
        let headers = header_map(&[
            ("X-GitHub-Event", "push"),
            ("X-GitHub-Delivery", "delivery-1"),
            ("X-Hub-Signature-256", &signature),
        ]);

        let out = process_webhook(&headers, body, Some(key)).expect("processed");
        let parsed: Value = serde_json::from_str(&out).expect("json");

        assert_eq!(parsed["ok"], true);
        let events = parsed["events"].as_array().expect("events array");
        assert_eq!(events.len(), 1);

        // Deserialize into the mirrored ingress contract.
        let event: EventEnvelopeV1 =
            serde_json::from_value(events[0].clone()).expect("event envelope contract");
        assert_eq!(event.event_type, "github.push");
        assert_eq!(event.event_id, "github:delivery-1");
        assert_eq!(event.correlation_id.as_deref(), Some("delivery-1"));
        assert_eq!(event.source.domain, "events");
        assert_eq!(event.source.provider, "events-ingress-github");
        assert_eq!(event.scope.tenant, "default");
        assert_eq!(event.payload["repo"], "octo/hello");
        assert_eq!(event.payload["ref"], "refs/heads/main");
        assert_eq!(event.payload["head_commit"]["id"], "abc123");
        assert_eq!(
            event.payload["commits"].as_array().expect("commits").len(),
            1
        );
    }

    #[test]
    fn wrong_signature_is_rejected_by_handler() {
        let body = sample_push_body();
        let headers = header_map(&[
            ("X-GitHub-Event", "push"),
            ("X-GitHub-Delivery", "delivery-1"),
            ("X-Hub-Signature-256", &format!("sha256={}", "0".repeat(64))),
        ]);

        let err = process_webhook(&headers, body, Some("test-secret"))
            .expect_err("bad signature should fail");
        assert!(err.contains("signature"), "{err}");
    }

    #[test]
    fn non_push_event_is_ignored_cleanly() {
        let headers = header_map(&[
            ("X-GitHub-Event", "ping"),
            ("X-GitHub-Delivery", "delivery-ping"),
        ]);
        // No secret configured -> signature check skipped; ping is acknowledged.
        let out = process_webhook(&headers, r#"{"zen":"Keep it simple"}"#, None).expect("ignored");
        let parsed: Value = serde_json::from_str(&out).expect("json");

        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["status"], 200);
        assert_eq!(parsed["events"].as_array().expect("events").len(), 0);
        assert_eq!(parsed["ignored_event"], "ping");
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let headers = header_map(&[("x-github-event", "push")]);
        let parsed = GithubWebhookHeaders::from_map(&headers);
        assert_eq!(parsed.event, "push");
    }
}
