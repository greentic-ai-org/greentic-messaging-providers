#![allow(unsafe_op_in_unsafe_fn)]

mod bindings {
    wit_bindgen::generate!({ path: "wit/whatsapp", world: "whatsapp", generate_all });
}

use bindings::Guest;
use bindings::greentic::http::http_client;
use bindings::greentic::secrets_store::secrets_store;
use serde_json::Value;

const WHATSAPP_API: &str = "https://graph.facebook.com/v18.0";
const WHATSAPP_TOKEN: &str = "WHATSAPP_TOKEN";
const WHATSAPP_VERIFY_TOKEN: &str = "WHATSAPP_VERIFY_TOKEN";

struct Component;

impl Guest for Component {
    fn send_message(destination_json: String, text: String) -> Result<String, String> {
        let dest = parse_destination(&destination_json)?;
        let token = get_secret(WHATSAPP_TOKEN)?;

        let url = format!("{}/{}/messages", WHATSAPP_API, dest.phone_id);
        let payload = format_message_json(&destination_json, &text);

        let req = http_client::Request {
            method: "POST".into(),
            url,
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Authorization".into(), format!("Bearer {}", token)),
            ],
            body: Some(payload.clone().into_bytes()),
        };

        let resp = http_client::send(&req, None)
            .map_err(|e| format!("transport error: {} ({})", e.message, e.code))?;

        if (200..300).contains(&resp.status) {
            Ok(payload)
        } else {
            Err(format!(
                "transport error: whatsapp returned status {}",
                resp.status
            ))
        }
    }

    fn handle_webhook(headers_json: String, body_json: String) -> Result<String, String> {
        // Parse headers for validation if needed; currently unused.
        let _headers: Value = serde_json::from_str(&headers_json)
            .map_err(|_| "validation error: invalid headers".to_string())?;

        let parsed: Value = serde_json::from_str(&body_json)
            .map_err(|_| "validation error: invalid body".to_string())?;

        if let Some(token) = parsed
            .get("hub.verify_token")
            .or_else(|| parsed.get("verify_token"))
            .and_then(Value::as_str)
        {
            let expected = get_secret(WHATSAPP_VERIFY_TOKEN)?;
            if token != expected {
                return Err("validation error: verify token mismatch".into());
            }
        }

        let normalized = serde_json::json!({ "ok": true, "event": parsed });
        serde_json::to_string(&normalized).map_err(|_| "other error: serialization failed".into())
    }

    fn refresh() -> Result<String, String> {
        Ok(r#"{"ok":true,"refresh":"not-needed"}"#.to_string())
    }

    fn format_message(destination_json: String, text: String) -> String {
        format_message_json(&destination_json, &text)
    }
}

#[derive(Debug)]
struct Destination {
    phone_id: String,
    to: String,
}

fn parse_destination(json: &str) -> Result<Destination, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|_| "validation error: invalid destination".to_string())?;
    let phone_id = value
        .get("phone_number_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "validation error: missing phone_number_id".to_string())?;
    let to = value
        .get("to")
        .and_then(Value::as_str)
        .ok_or_else(|| "validation error: missing to".to_string())?;
    Ok(Destination {
        phone_id: phone_id.to_string(),
        to: to.to_string(),
    })
}

fn get_secret(key: &str) -> Result<String, String> {
    match secrets_store::get(key) {
        Ok(Some(bytes)) => String::from_utf8(bytes).map_err(|_| "secret not valid utf-8".into()),
        Ok(None) => Err("secret not found".into()),
        Err(e) => secret_error(e),
    }
}

fn secret_error(error: secrets_store::SecretsError) -> Result<String, String> {
    Err(match error {
        secrets_store::SecretsError::NotFound => "secret not found".into(),
        secrets_store::SecretsError::Denied => "secret access denied".into(),
        secrets_store::SecretsError::InvalidKey => "secret key invalid".into(),
        secrets_store::SecretsError::Internal => "secret lookup failed".into(),
    })
}

fn format_message_json(destination_json: &str, text: &str) -> String {
    let dest = parse_destination(destination_json).ok();
    let payload = serde_json::json!({
        "messaging_product": "whatsapp",
        "to": dest.as_ref().map(|d| d.to.as_str()).unwrap_or(""),
        "type": "text",
        "text": { "body": text },
    });
    serde_json::to_string(&payload)
        .unwrap_or_else(|_| "{\"to\":\"\",\"text\":{\"body\":\"\"}}".into())
}

bindings::__export_world_whatsapp_cabi!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_destination() {
        let dest = parse_destination(r#"{"phone_number_id":"pn1","to":"+100"}"#).unwrap();
        assert_eq!(dest.phone_id, "pn1");
        assert_eq!(dest.to, "+100");
    }

    #[test]
    fn formats_payload() {
        let json = format_message_json(r#"{"phone_number_id":"pn1","to":"+100"}"#, "hi");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["messaging_product"], "whatsapp");
        assert_eq!(v["to"], "+100");
        assert_eq!(v["text"]["body"], "hi");
    }

    #[test]
    fn webhook_normalizes() {
        let res = Component::handle_webhook("{}".into(), r#"{"id":"1"}"#.into()).unwrap();
        let v: Value = serde_json::from_str(&res).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["event"]["id"], "1");
    }
}
