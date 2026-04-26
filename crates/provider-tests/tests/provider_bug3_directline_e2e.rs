//! End-to-end integration test for TASK-082 Bug 3 (envelope.extensions preservation).
//!
//! Loads the real `messaging-provider-webchat` WASM artifact and exercises the full
//! Direct Line POST `/conversations` → POST `/activities` flow through wasmtime,
//! sharing one `Store<HostState>` so the in-memory state-store persists across
//! invocations. JWT is forged inline using the same HS256 algorithm the provider
//! uses internally (`directline::jwt::issue_token`) so the second call passes
//! authentication.
//!
//! Asserts that the activity envelope emitted from the second call carries
//! `extensions.attachments` (Adaptive Card), `extensions.channel_data.rag.citations`,
//! and `extensions.entities` — the exact fields Bug 3 dropped before v0.4.78.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use base64::{
    Engine as _, engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD,
};
use greentic_interfaces_wasmtime::host_helpers::v1::{
    HostFns, add_all_v1_to_linker, secrets_store, state_store,
};
use hmac::{Hmac, Mac};
use provider_common::component_v0_6::{canonical_cbor_bytes, decode_cbor};
use serde_json::{Value, json};
use sha2::Sha256;
use wasmtime::component::{
    Component, ComponentExportIndex, HasSelf, Linker, ResourceTable, TypedFunc,
};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

mod bindings {
    wasmtime::component::bindgen!({
        path: "../../components/messaging-provider-webchat/wit/messaging-provider-webchat",
        world: "component-v0-v6-v0",
    });
}

const JWT_SECRET: &str = "dummy-jwt-secret-bug3-e2e";
const TENANT: &str = "demo";
const ENV: &str = "demo";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn webchat_wasm_path() -> PathBuf {
    workspace_root().join("target/components/messaging-provider-webchat.wasm")
}

#[derive(Default)]
struct HostState {
    table: ResourceTable,
    wasi_ctx: WasiCtx,
    secrets: HashMap<String, Vec<u8>>,
    state: HashMap<String, Vec<u8>>,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.table,
        }
    }
}

impl HostState {
    fn new(secrets: HashMap<String, Vec<u8>>) -> Self {
        Self {
            table: ResourceTable::new(),
            wasi_ctx: WasiCtxBuilder::new().inherit_stdio().build(),
            secrets,
            state: HashMap::new(),
        }
    }
}

impl secrets_store::SecretsStoreHostV1_1 for HostState {
    fn get(&mut self, key: String) -> Result<Option<Vec<u8>>, secrets_store::SecretsErrorV1_1> {
        Ok(self.secrets.get(&key).cloned())
    }

    fn put(&mut self, key: String, value: Vec<u8>) {
        self.secrets.insert(key, value);
    }
}

impl bindings::greentic::http::http_client::Host for HostState {
    fn send(
        &mut self,
        _req: bindings::greentic::http::http_client::Request,
        _options: Option<bindings::greentic::http::http_client::RequestOptions>,
        _ctx: Option<bindings::greentic::interfaces_types::types::TenantCtx>,
    ) -> Result<
        bindings::greentic::http::http_client::Response,
        bindings::greentic::http::http_client::HostError,
    > {
        Ok(bindings::greentic::http::http_client::Response {
            status: 200,
            headers: vec![],
            body: Some(b"{}".to_vec()),
        })
    }
}

impl state_store::StateStoreHost for HostState {
    fn read(
        &mut self,
        key: state_store::StateKey,
        _ctx: Option<state_store::TenantCtx>,
    ) -> Result<Vec<u8>, state_store::StateStoreError> {
        self.state
            .get(&key)
            .cloned()
            .ok_or_else(|| state_store::StateStoreError {
                code: "not_found".into(),
                message: format!("state key not found: {key}"),
            })
    }

    fn write(
        &mut self,
        key: state_store::StateKey,
        bytes: Vec<u8>,
        _ctx: Option<state_store::TenantCtx>,
    ) -> Result<state_store::OpAck, state_store::StateStoreError> {
        self.state.insert(key, bytes);
        Ok(state_store::OpAck::Ok)
    }

    fn delete(
        &mut self,
        key: state_store::StateKey,
        _ctx: Option<state_store::TenantCtx>,
    ) -> Result<state_store::OpAck, state_store::StateStoreError> {
        self.state.remove(&key);
        Ok(state_store::OpAck::Ok)
    }
}

fn add_wasi_to_linker(linker: &mut Linker<HostState>) {
    wasmtime_wasi::p2::add_to_linker_sync(linker).expect("add wasi");
}

fn add_greentic_hosts(linker: &mut Linker<HostState>) {
    bindings::greentic::http::http_client::add_to_linker::<HostState, HasSelf<HostState>>(
        linker,
        |state: &mut HostState| state,
    )
    .expect("link http-client");
    add_all_v1_to_linker(
        linker,
        HostFns {
            secrets_store_v1_1: Some(|state| state as &mut dyn secrets_store::SecretsStoreHostV1_1),
            state_store: Some(|state| state as &mut dyn state_store::StateStoreHost),
            ..Default::default()
        },
    )
    .expect("add greentic hosts");
}

fn new_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.cache(None);
    Engine::new(&config).expect("engine")
}

/// Build an HS256 JWT matching the provider's `directline::jwt::issue_token` format.
fn forge_directline_jwt(secret: &[u8], conv_id: Option<&str>) -> String {
    let now = chrono_now();
    let header = json!({"alg": "HS256", "typ": "JWT"});
    let claims = json!({
        "iss": "greentic.webchat",
        "aud": "directline",
        "sub": "test-user",
        "iat": now,
        "nbf": now,
        "exp": now + 1800,
        "ctx": {
            "env": ENV,
            "tenant": TENANT,
            "team": null,
        },
        "conv": conv_id,
    });
    let header_enc = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
    let payload_enc = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret).expect("hmac key");
    mac.update(header_enc.as_bytes());
    mac.update(b".");
    mac.update(payload_enc.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    format!("{header_enc}.{payload_enc}.{sig}")
}

fn chrono_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_secs() as i64
}

/// Decode response bytes — try JSON first, fall back to CBOR.
fn decode_response(bytes: &[u8]) -> Result<Value> {
    if let Ok(v) = serde_json::from_slice::<Value>(bytes) {
        return Ok(v);
    }
    decode_cbor::<Value>(bytes).map_err(|e| anyhow::anyhow!("not JSON nor CBOR: {e}"))
}

/// Invoke the WASM `runtime.invoke` export with op + raw input bytes, return raw output.
fn invoke_op(
    store: &mut Store<HostState>,
    invoke: &TypedFunc<(String, Vec<u8>), (Vec<u8>,)>,
    op: &str,
    input: Vec<u8>,
) -> Result<Vec<u8>> {
    let (resp,) = invoke
        .call(store, (op.to_string(), input))
        .map_err(|err| anyhow::anyhow!("invoke {op}: {err}"))?;
    Ok(resp)
}

/// Build a CBOR-encoded `HttpInV1` payload (webchat `runtime::Guest::invoke` decodes input as CBOR first).
fn build_http_in(
    method: &str,
    path: &str,
    query: Option<&str>,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Vec<u8> {
    let body_b64 = STANDARD.encode(body);
    let headers_json: Vec<Value> = headers
        .iter()
        .map(|(name, value)| json!({"name": name, "value": value}))
        .collect();
    let payload = json!({
        "method": method,
        "path": path,
        "query": query,
        "headers": headers_json,
        "body_b64": body_b64,
        "route_hint": null,
        "binding_id": null,
        "config": null,
    });
    canonical_cbor_bytes(&payload)
}

#[test]
fn bug3_directline_attachments_channel_data_entities_round_trip() -> Result<()> {
    // Skip gracefully if the WASM artifact has not been built. The test is meant
    // to be run after `./tools/build_components.sh`.
    let component_path = webchat_wasm_path();
    if !component_path.exists() {
        eprintln!(
            "SKIP: webchat WASM not built at {}. Run ./tools/build_components.sh first.",
            component_path.display()
        );
        return Ok(());
    }

    let engine = new_engine();
    let component = Component::from_file(&engine, &component_path)
        .map_err(|err| anyhow::anyhow!("load component: {err}"))?;

    let mut linker = Linker::new(&engine);
    add_wasi_to_linker(&mut linker);
    add_greentic_hosts(&mut linker);

    // Single Store, used for ALL invocations so the in-memory state-store persists.
    let mut secrets = HashMap::new();
    secrets.insert(
        "jwt_signing_key".to_string(),
        JWT_SECRET.as_bytes().to_vec(),
    );
    let mut store = Store::new(&engine, HostState::new(secrets));
    let instance = linker
        .instantiate(&mut store, &component)
        .map_err(|err| anyhow::anyhow!("instantiate: {err}"))?;

    let api_index: ComponentExportIndex = instance
        .get_export_index(&mut store, None, "greentic:component/runtime@0.6.0")
        .context("get runtime export index")?;
    let invoke_index = instance
        .get_export_index(&mut store, Some(&api_index), "invoke")
        .context("get invoke export index")?;
    let invoke: TypedFunc<(String, Vec<u8>), (Vec<u8>,)> = instance
        .get_typed_func(&mut store, invoke_index)
        .map_err(|err| anyhow::anyhow!("get invoke func: {err}"))?;

    // -------------------------------------------------------------------------
    // Step 1 — Forge an initial JWT (no conv) so POST /conversations can mint a
    // conversation-bound token, mirroring the real /tokens/generate flow.
    // -------------------------------------------------------------------------
    let initial_token = forge_directline_jwt(JWT_SECRET.as_bytes(), None);

    let create_in = build_http_in(
        "POST",
        "/v3/directline/conversations",
        Some(&format!("tenant={TENANT}")),
        &[
            ("content-type", "application/json"),
            ("origin", "https://example.com"),
            ("authorization", &format!("Bearer {initial_token}")),
        ],
        b"{}",
    );
    let create_out_bytes = invoke_op(&mut store, &invoke, "ingest_http", create_in)?;
    eprintln!(
        "CREATE raw bytes ({}): {:02x?}",
        create_out_bytes.len(),
        &create_out_bytes[..create_out_bytes.len().min(80)]
    );
    let create_out: Value = decode_response(&create_out_bytes)
        .with_context(|| format!("decode create response: {} bytes", create_out_bytes.len()))?;

    eprintln!(
        "CREATE response: {}",
        serde_json::to_string_pretty(&create_out).unwrap_or_default()
    );

    let status = create_out
        .get("status")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        (200..300).contains(&(status as u16)),
        "POST /conversations should succeed, got status {status}: {create_out}"
    );

    // Decode body to get conversationId.
    let body_b64 = create_out
        .get("body_b64")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let body_bytes = STANDARD.decode(body_b64).unwrap_or_default();
    let conv_body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
    let conv_id = conv_body
        .get("conversationId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            // Some responses put conversationId in headers.
            create_out
                .get("headers")?
                .as_array()?
                .iter()
                .find(|h| {
                    h.get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.eq_ignore_ascii_case("X-Greentic-ConversationId"))
                        .unwrap_or(false)
                })?
                .get("value")?
                .as_str()
                .map(|s| s.to_string())
        })
        .expect("conversationId in response body or headers");
    eprintln!("CREATE conversationId={conv_id}");

    // -------------------------------------------------------------------------
    // Step 2 — Use the conversation-bound token returned by /conversations
    // (mirrors what a real client does — the response body contains the new token).
    // -------------------------------------------------------------------------
    let token = conv_body
        .get("token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| forge_directline_jwt(JWT_SECRET.as_bytes(), Some(&conv_id)));

    // -------------------------------------------------------------------------
    // Step 3 — POST /v3/directline/conversations/{id}/activities with a body
    // containing attachments + channelData.rag.citations + entities.
    // This is the EXACT scenario Bug 3 dropped before v0.4.78.
    // -------------------------------------------------------------------------
    let activity_body = json!({
        "type": "message",
        "from": {"id": "user-1"},
        "text": "What does the documentation say?",
        "attachments": [{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": {
                "type": "AdaptiveCard",
                "version": "1.5",
                "body": [{"type": "TextBlock", "text": "User question"}],
            }
        }],
        "channelData": {
            "rag": {
                "citations": [
                    {"id": "c1", "source": "docs/x.md", "snippet": "..."},
                    {"id": "c2", "source": "docs/y.md", "snippet": "..."}
                ]
            },
            "clientActivityID": "abc123"
        },
        "entities": [{"type": "mention", "text": "@bot"}]
    });
    let activity_body_bytes = serde_json::to_vec(&activity_body).unwrap();

    let post_activity_in = build_http_in(
        "POST",
        &format!("/v3/directline/conversations/{conv_id}/activities"),
        Some(&format!("tenant={TENANT}")),
        &[
            ("content-type", "application/json"),
            ("authorization", &format!("Bearer {token}")),
        ],
        &activity_body_bytes,
    );
    let activity_out_bytes = invoke_op(&mut store, &invoke, "ingest_http", post_activity_in)?;
    let activity_out: Value = decode_response(&activity_out_bytes).with_context(|| {
        format!(
            "decode activity response: {} bytes",
            activity_out_bytes.len()
        )
    })?;

    eprintln!(
        "ACTIVITY response: {}",
        serde_json::to_string_pretty(&activity_out).unwrap_or_default()
    );

    let activity_status = activity_out
        .get("status")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        (200..300).contains(&(activity_status as u16)),
        "POST /activities should succeed, got status {activity_status}: {activity_out}"
    );

    // -------------------------------------------------------------------------
    // Step 4 — Inspect emitted envelope events and assert Bug 3 fields preserved.
    // -------------------------------------------------------------------------
    let events = activity_out
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !events.is_empty(),
        "expected at least one envelope event from /activities POST"
    );

    let envelope = events
        .iter()
        .find(|e| e.get("extensions").is_some())
        .expect("envelope with extensions field expected (Bug 3 fix)");

    let extensions = envelope
        .get("extensions")
        .expect("envelope.extensions must exist");

    // attachments preserved as-is (DirectLine camelCase content kept).
    let attachments = extensions
        .get("attachments")
        .and_then(|v| v.as_array())
        .expect("extensions.attachments must be an array");
    assert_eq!(attachments.len(), 1, "1 AC attachment expected");
    assert_eq!(
        attachments[0].get("contentType").and_then(|v| v.as_str()),
        Some("application/vnd.microsoft.card.adaptive")
    );

    // channelData → channel_data with snake_case mapping; RAG citations preserved.
    let citations = extensions
        .pointer("/channel_data/rag/citations")
        .and_then(|v| v.as_array())
        .expect("RAG citations must round-trip via extensions.channel_data.rag.citations");
    assert_eq!(citations.len(), 2);
    assert_eq!(citations[0].get("id").and_then(|v| v.as_str()), Some("c1"));
    assert_eq!(citations[1].get("id").and_then(|v| v.as_str()), Some("c2"));

    // entities preserved.
    let entities = extensions
        .get("entities")
        .and_then(|v| v.as_array())
        .expect("extensions.entities must be an array");
    assert_eq!(entities.len(), 1);
    assert_eq!(
        entities[0].get("type").and_then(|v| v.as_str()),
        Some("mention")
    );

    eprintln!(
        "✅ Bug 3 E2E PASS — attachments, channel_data.rag.citations, entities all preserved through full DirectLine HTTP flow."
    );
    Ok(())
}
