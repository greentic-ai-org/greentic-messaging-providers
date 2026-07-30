use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use urlencoding::{decode, encode};
use uuid::Uuid;

use greentic_types::messaging::universal_dto::{Header, HttpInV1, HttpOutV1};

use super::jwt::{DirectLineContext, TTL_SECONDS, TokenIdentity, issue_token, verify_token};
use super::state::{ConversationState, StoredActivity, conversation_key, sanitize_team};
use super::store::{RateLimitState, SecretStore, StateStore};

const DIRECTLINE_PREFIX: &str = "/v3/directline";
const JSON_CONTENT_TYPE: &str = "application/json";
const TOKEN_SECRET_KEY: &str = "jwt_signing_key";
const RATE_LIMIT_WINDOW_SECONDS_DEFAULT: i64 = 60;
const RATE_LIMIT_REQUESTS_DEFAULT: u32 = 60;
const MAX_ATTACHMENT_BYTES: usize = 512 * 1024;
const ALLOWED_ATTACHMENT_TYPES: &[&str] = &[
    "text/plain",
    "application/json",
    "image/png",
    "image/jpeg",
    "image/gif",
    "application/vnd.microsoft.card.adaptive",
    "application/vnd.microsoft.card.hero",
    "application/vnd.microsoft.card.thumbnail",
];

pub fn handle_directline_request<S, SE>(
    request: &HttpInV1,
    state_store: &mut S,
    secrets: &SE,
) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
{
    if !request.path.starts_with(DIRECTLINE_PREFIX) {
        return respond_not_found("missing directline prefix");
    }

    // Handle CORS preflight requests
    if method_is(request, "OPTIONS") {
        return respond_cors_preflight();
    }

    let segments = request
        .path
        .trim_start_matches('/')
        .split('/')
        .collect::<Vec<_>>();

    match segments.as_slice() {
        ["v3", "directline", "tokens", "generate"] if method_is(request, "POST") => {
            handle_tokens(request, state_store, secrets)
        }
        ["v3", "directline", "tokens", "generate"] => method_not_allowed(),
        ["v3", "directline", "tokens", "refresh"] if method_is(request, "POST") => {
            handle_refresh_token(request, state_store, secrets)
        }
        ["v3", "directline", "tokens", "refresh"] => method_not_allowed(),
        ["v3", "directline", "conversations"] if method_is(request, "POST") => {
            handle_conversations(request, state_store, secrets)
        }
        ["v3", "directline", "conversations"] => method_not_allowed(),
        ["v3", "directline", "conversations", conv_id, "activities"] => {
            match request.method.as_str() {
                m if m.eq_ignore_ascii_case("POST") => {
                    handle_post_activities(request, state_store, secrets, conv_id)
                }
                m if m.eq_ignore_ascii_case("GET") => {
                    handle_get_activities(request, state_store, secrets, conv_id)
                }
                _ => method_not_allowed(),
            }
        }
        ["v3", "directline", "conversations", conv_id] if method_is(request, "GET") => {
            handle_reconnect_conversation(request, state_store, secrets, conv_id)
        }
        ["v3", "directline", "conversations", _conv_id] => method_not_allowed(),
        ["v3", "directline", "conversations", _conv_id, "stream"] => respond_not_implemented(),
        _ => respond_not_found("unknown directline endpoint"),
    }
}

fn handle_tokens<S, SE>(request: &HttpInV1, state_store: &mut S, secrets: &SE) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
{
    let ctx = parse_context(request.query.as_deref());
    let body = match decode_json_body(request) {
        Ok(payload) => payload,
        Err(resp) => return resp,
    };
    let subject = determine_rate_limit_subject(request, &body);
    let now = Utc::now().timestamp();
    let cfg = RateLimitConfig::from_request(request);
    let rate_key = rate_limit_key(&ctx, &subject);
    if let Err(resp) = enforce_rate_limit(state_store, &rate_key, now, &cfg) {
        return resp;
    }

    let signing_key = match load_signing_key(request, secrets) {
        Ok(key) => key,
        Err(resp) => return resp,
    };

    let identity = TokenIdentity {
        sub: subject.token_subject().to_string(),
        email: None,
        idp: None,
        verified: false,
    };
    match issue_token(&signing_key, ctx.clone(), &identity, None) {
        Ok((token, _exp)) => respond_json(
            200,
            json!({
                "token": token,
                "expires_in": TTL_SECONDS,
            }),
        ),
        Err(err) => respond_error(
            500,
            "token_issue_failed",
            format!("failed to mint token: {err:?}"),
        ),
    }
}

fn handle_conversations<S, SE>(request: &HttpInV1, state_store: &mut S, secrets: &SE) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
{
    let authorization = match extract_bearer(request.headers.as_slice()) {
        Some(header) => header,
        None => return respond_unauthorized("missing Authorization header"),
    };
    let signing_key = match load_signing_key(request, secrets) {
        Ok(key) => key,
        Err(resp) => return resp,
    };
    let claims = match verify_token(&signing_key, &authorization) {
        Ok(claims) => claims,
        Err(err) => return respond_unauthorized(&format!("invalid token: {err:?}")),
    };

    if claims.conv.is_some() {
        return respond_forbidden("token already bound to a conversation");
    }

    let ctx = claims.ctx.clone();
    let conversation_id = Uuid::new_v4().to_string();
    let key = conversation_key(&ctx, &conversation_id);
    let conversation = ConversationState::new(ctx.clone());

    if let Err(resp) = write_conversation_state(state_store, &key, &conversation) {
        return resp;
    }

    let identity = TokenIdentity {
        sub: claims.sub.clone(),
        email: claims.email.clone(),
        idp: claims.idp.clone(),
        verified: claims.verified,
    };
    let (token, _exp) = match issue_token(
        &signing_key,
        ctx.clone(),
        &identity,
        Some(conversation_id.clone()),
    ) {
        Ok(pair) => pair,
        Err(err) => {
            return respond_error(
                500,
                "token_issue_failed",
                format!("failed to mint conversation token: {err:?}"),
            );
        }
    };

    // Include context in headers so ingest_http can extract env/tenant for autoStart envelope
    let mut headers = json_headers();
    headers.push(Header {
        name: "X-Greentic-Env".to_string(),
        value: ctx.env.clone(),
    });
    headers.push(Header {
        name: "X-Greentic-Tenant".to_string(),
        value: ctx.tenant.clone(),
    });
    headers.push(Header {
        name: "X-Greentic-User".to_string(),
        value: claims.sub.clone(),
    });
    headers.push(Header {
        name: "X-Greentic-ConversationId".to_string(),
        value: conversation_id.clone(),
    });

    let stream_url = build_stream_url(&ctx.tenant, &conversation_id, &token);
    respond_json_with_headers(
        201,
        json!({
            "conversationId": conversation_id,
            "token": token,
            "expires_in": TTL_SECONDS,
            "streamUrl": stream_url,
        }),
        headers,
    )
}

fn handle_refresh_token<S, SE>(request: &HttpInV1, state_store: &mut S, secrets: &SE) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
{
    let authorization = match extract_bearer(request.headers.as_slice()) {
        Some(header) => header,
        None => return respond_unauthorized("missing Authorization header"),
    };
    let signing_key = match load_signing_key(request, secrets) {
        Ok(key) => key,
        Err(resp) => return resp,
    };
    let claims = match verify_token(&signing_key, &authorization) {
        Ok(claims) => claims,
        Err(err) => return respond_unauthorized(&format!("invalid token: {err:?}")),
    };

    if let Some(conversation_id) = claims.conv.as_deref() {
        let conv_key = conversation_key(&claims.ctx, conversation_id);
        let conversation = match load_conversation_state(state_store, &conv_key) {
            Ok(state) => state,
            Err(resp) => return resp,
        };

        if conversation.ctx != claims.ctx {
            return respond_forbidden("token context mismatch");
        }
    }

    let identity = TokenIdentity {
        sub: claims.sub.clone(),
        email: claims.email.clone(),
        idp: claims.idp.clone(),
        verified: claims.verified,
    };
    let (token, _exp) = match issue_token(
        &signing_key,
        claims.ctx.clone(),
        &identity,
        claims.conv.clone(),
    ) {
        Ok(pair) => pair,
        Err(err) => {
            return respond_error(
                500,
                "token_issue_failed",
                format!("failed to mint refresh token: {err:?}"),
            );
        }
    };

    respond_json(
        200,
        json!({
            "conversationId": claims.conv,
            "token": token,
            "expires_in": TTL_SECONDS,
        }),
    )
}

/// Handle GET /v3/directline/conversations/{conversationId} - reconnect to existing conversation.
/// Returns conversation info with a refreshed token if the conversation exists and token is valid.
fn handle_reconnect_conversation<S, SE>(
    request: &HttpInV1,
    state_store: &mut S,
    secrets: &SE,
    conversation_id: &str,
) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
{
    let authorization = match extract_bearer(request.headers.as_slice()) {
        Some(header) => header,
        None => return respond_unauthorized("missing Authorization header"),
    };
    let signing_key = match load_signing_key(request, secrets) {
        Ok(key) => key,
        Err(resp) => return resp,
    };
    let claims = match verify_token(&signing_key, &authorization) {
        Ok(claims) => claims,
        Err(err) => return respond_unauthorized(&format!("invalid token: {err:?}")),
    };

    // Token must be bound to this conversation (or unbound for first reconnect)
    if let Some(ref bound_conv) = claims.conv
        && bound_conv != conversation_id
    {
        return respond_forbidden("token bound to different conversation");
    }

    let ctx = claims.ctx.clone();
    let conv_key = conversation_key(&ctx, conversation_id);

    // Verify conversation exists
    match load_conversation_state(state_store, &conv_key) {
        Ok(conversation) => {
            if conversation.ctx != ctx {
                return respond_forbidden("token context mismatch");
            }
        }
        Err(resp) => return resp,
    };

    // Issue a new token bound to this conversation. Clone ctx because we still
    // need its tenant after the move into `issue_token` to build the streamUrl.
    let tenant_for_stream = ctx.tenant.clone();
    let identity = TokenIdentity {
        sub: claims.sub.clone(),
        email: claims.email.clone(),
        idp: claims.idp.clone(),
        verified: claims.verified,
    };
    let (token, _exp) = match issue_token(
        &signing_key,
        ctx,
        &identity,
        Some(conversation_id.to_string()),
    ) {
        Ok(pair) => pair,
        Err(err) => {
            return respond_error(
                500,
                "token_issue_failed",
                format!("failed to mint reconnect token: {err:?}"),
            );
        }
    };

    let stream_url = build_stream_url(&tenant_for_stream, conversation_id, &token);
    respond_json(
        200,
        json!({
            "conversationId": conversation_id,
            "token": token,
            "expires_in": TTL_SECONDS,
            "streamUrl": stream_url,
        }),
    )
}

fn handle_post_activities<S, SE>(
    request: &HttpInV1,
    state_store: &mut S,
    secrets: &SE,
    conversation_id: &str,
) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
{
    let authorization = match extract_bearer(request.headers.as_slice()) {
        Some(token) => token,
        None => return respond_unauthorized("missing Authorization header"),
    };
    let signing_key = match load_signing_key(request, secrets) {
        Ok(key) => key,
        Err(resp) => return resp,
    };
    let claims = match verify_token(&signing_key, &authorization) {
        Ok(claims) => claims,
        Err(err) => return respond_unauthorized(&format!("invalid token: {err:?}")),
    };

    if claims.conv.as_deref() != Some(conversation_id) {
        return respond_forbidden("token bound to different conversation");
    }

    let conv_key = conversation_key(&claims.ctx, conversation_id);
    let mut conversation = match load_conversation_state(state_store, &conv_key) {
        Ok(state) => state,
        Err(resp) => return resp,
    };

    if conversation.ctx != claims.ctx {
        return respond_forbidden("token context mismatch");
    }

    let watermark = conversation.bump_watermark();
    let body = match decode_json_body(request) {
        Ok(value) => value,
        Err(resp) => return resp,
    };

    if let Err(resp) = validate_attachments(&body) {
        return resp;
    }

    let activity = StoredActivity {
        id: Uuid::new_v4().to_string(),
        type_: body
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("message")
            .to_string(),
        text: body
            .get("text")
            .and_then(|value| value.as_str())
            .map(|s| s.to_string()),
        from: body
            .get("from")
            .and_then(|from| from.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        timestamp: Utc::now().timestamp_millis(),
        watermark,
        raw: body.clone(),
    };

    conversation.activities.push(activity.clone());

    if let Err(resp) = write_conversation_state(state_store, &conv_key, &conversation) {
        return resp;
    }

    // Include context in headers so ingest_http can extract env/tenant for envelope routing
    let mut headers = json_headers();
    headers.push(Header {
        name: "X-Greentic-Env".to_string(),
        value: claims.ctx.env.clone(),
    });
    headers.push(Header {
        name: "X-Greentic-Tenant".to_string(),
        value: claims.ctx.tenant.clone(),
    });

    // Emit `_greentic` metadata so the host can fire WebSocket push notifications
    // for live activity streams. Underscore-prefixed keys are ignored by DirectLine
    // clients, making this safe to add to the response body.
    let body_value = json!({
        "id": activity.id,
        "_greentic": {
            "watermark_bumped": watermark,
            "conversation_id": conversation_id,
            "tenant": claims.ctx.tenant,
        },
    });
    respond_json_with_headers(201, body_value, headers)
}

fn handle_get_activities<S, SE>(
    request: &HttpInV1,
    state_store: &mut S,
    secrets: &SE,
    conversation_id: &str,
) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
{
    let authorization = match extract_bearer(request.headers.as_slice()) {
        Some(token) => token,
        None => return respond_unauthorized("missing Authorization header"),
    };
    let signing_key = match load_signing_key(request, secrets) {
        Ok(key) => key,
        Err(resp) => return resp,
    };
    let claims = match verify_token(&signing_key, &authorization) {
        Ok(claims) => claims,
        Err(err) => return respond_unauthorized(&format!("invalid token: {err:?}")),
    };

    if claims.conv.as_deref() != Some(conversation_id) {
        return respond_forbidden("token bound to different conversation");
    }

    let conv_key = conversation_key(&claims.ctx, conversation_id);
    let conversation = match load_conversation_state(state_store, &conv_key) {
        Ok(state) => state,
        Err(resp) => return resp,
    };

    if conversation.ctx != claims.ctx {
        return respond_forbidden("token context mismatch");
    }

    let watermark = match parse_watermark(request.query.as_deref()) {
        Ok(value) => value,
        Err(resp) => return resp,
    };

    let activities = conversation
        .activities
        .iter()
        .filter(|activity| match watermark {
            Some(watermark) => activity.watermark >= watermark,
            None => true,
        })
        .map(activity_to_value)
        .collect::<Vec<_>>();

    respond_json(
        200,
        json!({
            "activities": activities,
            "watermark": conversation.next_watermark.to_string(),
        }),
    )
}

fn enforce_rate_limit<S: StateStore>(
    store: &mut S,
    key: &str,
    now: i64,
    cfg: &RateLimitConfig,
) -> Result<(), HttpOutV1> {
    let mut state = match read_rate_limit_state(store, key) {
        Ok(Some(state)) => state,
        Ok(None) => RateLimitState::new(now),
        Err(resp) => return Err(resp),
    };

    if state.bump(now, cfg.window_seconds, cfg.requests).is_err() {
        let retry_after = (cfg.window_seconds - (now - state.window_start)).max(1);
        return Err(respond_rate_limited(retry_after));
    }

    let bytes = match serde_json::to_vec(&state) {
        Ok(bytes) => bytes,
        Err(err) => return Err(respond_error(500, "state_serialize", err.to_string())),
    };

    store
        .write(key, &bytes)
        .map_err(|err| respond_error(500, "state_write", err))
}

fn read_rate_limit_state<S: StateStore>(
    store: &mut S,
    key: &str,
) -> Result<Option<RateLimitState>, HttpOutV1> {
    match store.read(key) {
        Ok(Some(bytes)) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|err| respond_error(500, "state_parse", err.to_string())),
        Ok(None) => Ok(None),
        Err(err) => Err(respond_error(500, "state_read", err)),
    }
}

fn write_conversation_state<S: StateStore>(
    store: &mut S,
    key: &str,
    state: &ConversationState,
) -> Result<(), HttpOutV1> {
    let bytes = serde_json::to_vec(state)
        .map_err(|err| respond_error(500, "state_serialize", err.to_string()))?;
    store
        .write(key, &bytes)
        .map_err(|err| respond_error(500, "state_write", err))
}

fn load_conversation_state<S: StateStore>(
    store: &mut S,
    key: &str,
) -> Result<ConversationState, HttpOutV1> {
    match store.read(key) {
        Ok(Some(bytes)) => serde_json::from_slice(&bytes)
            .map_err(|err| respond_error(500, "state_parse", err.to_string())),
        Ok(None) => Err(respond_not_found("conversation not found")),
        Err(err) => Err(respond_error(500, "state_read", err)),
    }
}

fn activity_to_value(activity: &StoredActivity) -> Value {
    let mut map = match activity.raw.clone() {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("data".to_string(), other);
            map
        }
    };
    map.insert("id".to_string(), Value::String(activity.id.clone()));
    map.insert("type".to_string(), Value::String(activity.type_.clone()));
    // Format timestamp as ISO 8601 (WebChat SDK expects this, not raw millis).
    let ts_iso = chrono::DateTime::from_timestamp_millis(activity.timestamp)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| activity.timestamp.to_string());
    map.insert("timestamp".to_string(), Value::String(ts_iso));
    map.insert(
        "watermark".to_string(),
        Value::String(activity.watermark.to_string()),
    );
    if let Some(text) = &activity.text {
        map.insert("text".to_string(), Value::String(text.clone()));
    }
    if !map.contains_key("from")
        && let Some(from) = &activity.from
    {
        let mut from_map = Map::new();
        from_map.insert("id".to_string(), Value::String(from.clone()));
        map.insert("from".to_string(), Value::Object(from_map));
    }
    Value::Object(map)
}

fn validate_attachments(body: &Value) -> Result<(), HttpOutV1> {
    let attachments = match body.get("attachments") {
        Some(Value::Array(items)) => items,
        _ => return Ok(()),
    };

    for attachment in attachments {
        let content_type = attachment
            .get("contentType")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if !ALLOWED_ATTACHMENT_TYPES.contains(&content_type) {
            return Err(respond_bad_request(&format!(
                "unsupported content type: {content_type}"
            )));
        }

        if let Some(content) = attachment.get("content")
            && let Some(text) = content.as_str()
            && text.len() > MAX_ATTACHMENT_BYTES
        {
            return Err(respond_bad_request("attachment too large"));
        }
    }

    Ok(())
}

fn parse_watermark(query: Option<&str>) -> Result<Option<u64>, HttpOutV1> {
    let params = parse_query(query);
    if let Some(value) = params.get("watermark") {
        if value.is_empty() {
            return Ok(None);
        }
        value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| respond_bad_request("watermark must be a number"))
    } else {
        Ok(None)
    }
}

fn rate_limit_key(ctx: &DirectLineContext, subject: &RateLimitSubject) -> String {
    format!(
        "webchat:rate:tokens:{}:{}:{}:{}",
        ctx.env,
        ctx.tenant,
        sanitize_team(ctx.team.as_deref()),
        subject.bucket_key()
    )
}

/// Per-route rate limit configuration. Operators can override defaults via
/// the provider config envelope: `rate_limit_requests` (per-window cap) and
/// `rate_limit_window_seconds` (window length). Both fall back to sensible
/// defaults that target ~60 token mints / minute / subject — enough for
/// normal page reload + retry burst, while still bounding abuse.
#[derive(Clone, Copy, Debug)]
struct RateLimitConfig {
    window_seconds: i64,
    requests: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            window_seconds: RATE_LIMIT_WINDOW_SECONDS_DEFAULT,
            requests: RATE_LIMIT_REQUESTS_DEFAULT,
        }
    }
}

impl RateLimitConfig {
    fn from_request(request: &HttpInV1) -> Self {
        let mut cfg = Self::default();
        let Some(config) = request.config.as_ref() else {
            return cfg;
        };
        if let Some(req) = config.get("rate_limit_requests").and_then(Value::as_u64)
            && req > 0
            && req <= u32::MAX as u64
        {
            cfg.requests = req as u32;
        }
        if let Some(win) = config
            .get("rate_limit_window_seconds")
            .and_then(Value::as_i64)
            && win > 0
        {
            cfg.window_seconds = win;
        }
        cfg
    }
}

/// Bucket subject for rate limiting.
///
/// Authenticated users get their own per-user bucket. Anonymous requests
/// are bucketed by hashed client IP (read from forwarding headers) so that
/// 1000 unauthenticated visitors don't all share the single `anonymous`
/// bucket. If neither id nor IP is available, a true anonymous bucket is
/// used as last resort — operators behind misconfigured reverse proxies
/// will see the legacy shared-bucket behaviour and should fix proxy
/// `X-Forwarded-For` propagation.
#[derive(Clone, Debug, PartialEq)]
enum RateLimitSubject {
    User(String),
    Ip(String),
    Anonymous,
}

impl RateLimitSubject {
    fn bucket_key(&self) -> String {
        match self {
            Self::User(id) => format!("u:{id}"),
            Self::Ip(hash) => format!("ip:{hash}"),
            Self::Anonymous => "anonymous".to_string(),
        }
    }

    fn token_subject(&self) -> &str {
        match self {
            Self::User(id) => id.as_str(),
            Self::Ip(hash) => hash.as_str(),
            Self::Anonymous => "anonymous",
        }
    }
}

fn determine_rate_limit_subject(request: &HttpInV1, body: &Value) -> RateLimitSubject {
    if let Some(id) = body
        .get("user")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return RateLimitSubject::User(id.to_string());
    }
    if let Some(ip) = extract_client_ip(&request.headers) {
        return RateLimitSubject::Ip(hash_client_id(&ip));
    }
    RateLimitSubject::Anonymous
}

/// Extract the originating client IP from common forwarding headers.
///
/// Priority: `True-Client-IP` (Cloudflare/Akamai) > `X-Real-IP` (nginx) >
/// first hop of `X-Forwarded-For` (RFC 7239 / generic). Returns the raw
/// string; callers should hash before persisting or logging.
fn extract_client_ip(headers: &[Header]) -> Option<String> {
    for header_name in ["true-client-ip", "x-real-ip", "x-forwarded-for"] {
        for header in headers {
            if !header.name.eq_ignore_ascii_case(header_name) {
                continue;
            }
            let first_hop = header.value.split(',').next().unwrap_or("").trim();
            if !first_hop.is_empty() {
                return Some(first_hop.to_string());
            }
        }
    }
    None
}

/// Hash a client identifier (typically an IP) to a short stable token used
/// for rate-limit bucketing. We never persist the raw IP — only the hash.
fn hash_client_id(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let hash = hasher.finalize();
    let mut out = String::with_capacity(16);
    for byte in &hash[..8] {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Load the JWT signing key from either injected config or secrets store.
/// Injected config takes priority (for provider_core_only mode where host pre-fetches secrets).
fn load_signing_key<SE: SecretStore>(
    request: &HttpInV1,
    secrets: &SE,
) -> Result<Vec<u8>, HttpOutV1> {
    // First, check for host-injected secret in config (for provider_core_only mode)
    if let Some(config) = &request.config {
        let config_key = format!("{TOKEN_SECRET_KEY}_b64");
        if let Some(b64_value) = config.get(&config_key).and_then(|v| v.as_str()) {
            return general_purpose::STANDARD
                .decode(b64_value)
                .map_err(|err| {
                    respond_error(
                        500,
                        "config_decode_error",
                        format!("failed to decode {config_key} from config: {err}"),
                    )
                })
                .and_then(|bytes| {
                    if bytes.is_empty() {
                        Err(respond_error(500, "invalid_secret", "signing key is empty"))
                    } else {
                        Ok(bytes)
                    }
                });
        }
    }

    // Fall back to secrets store
    match secrets.get(TOKEN_SECRET_KEY) {
        Ok(Some(bytes)) if !bytes.is_empty() => Ok(bytes),
        Ok(Some(_)) => Err(respond_error(500, "invalid_secret", "signing key is empty")),
        Ok(None) => Err(respond_error(
            500,
            "missing_secret",
            format!("secret {TOKEN_SECRET_KEY} not found"),
        )),
        Err(err) => Err(respond_error(500, "secret_error", err)),
    }
}

fn parse_context(query: Option<&str>) -> DirectLineContext {
    let params = parse_query(query);
    let env = params
        .get("env")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let tenant = params
        .get("tenant")
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let team = params.get("team").and_then(|team| {
        let trimmed = team.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    DirectLineContext { env, tenant, team }
}

fn parse_query(query: Option<&str>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(query) = query {
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let mut split = pair.splitn(2, '=');
            let key = split.next().unwrap_or_default();
            let value = split.next().unwrap_or_default();
            if let (Ok(key), Ok(value)) = (decode(key), decode(value)) {
                map.insert(key.into_owned(), value.into_owned());
            }
        }
    }
    map
}

fn decode_json_body(request: &HttpInV1) -> Result<Value, HttpOutV1> {
    if request.body_b64.trim().is_empty() {
        return Ok(Value::Null);
    }
    let bytes = match general_purpose::STANDARD.decode(&request.body_b64) {
        Ok(bytes) => bytes,
        Err(err) => {
            return Err(respond_bad_request(&format!(
                "invalid body encoding: {err}"
            )));
        }
    };
    serde_json::from_slice(&bytes)
        .map_err(|err| respond_bad_request(&format!("invalid json payload: {err}")))
}

fn extract_bearer(headers: &[Header]) -> Option<String> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("Authorization"))
        .and_then(|header| {
            let value = header.value.trim();
            let mut parts = value.splitn(2, ' ');
            let scheme = parts.next().unwrap_or_default();
            if scheme.eq_ignore_ascii_case("bearer") {
                Some(parts.next().unwrap_or_default().trim().to_string())
            } else {
                None
            }
        })
}

fn method_is(request: &HttpInV1, method: &str) -> bool {
    request.method.eq_ignore_ascii_case(method)
}

fn json_headers() -> Vec<Header> {
    vec![
        Header {
            name: "Content-Type".to_string(),
            value: JSON_CONTENT_TYPE.to_string(),
        },
        Header {
            name: "Access-Control-Allow-Origin".to_string(),
            value: "*".to_string(),
        },
        Header {
            name: "Access-Control-Allow-Headers".to_string(),
            value: "Authorization, Content-Type".to_string(),
        },
        Header {
            name: "Access-Control-Allow-Methods".to_string(),
            value: "GET, POST, OPTIONS".to_string(),
        },
    ]
}

fn respond_json(status: u16, payload: Value) -> HttpOutV1 {
    respond_json_with_headers(status, payload, json_headers())
}

fn respond_json_with_headers(status: u16, payload: Value, headers: Vec<Header>) -> HttpOutV1 {
    let body = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
    HttpOutV1 {
        status,
        headers,
        body_b64: general_purpose::STANDARD.encode(&body),
        events: Vec::new(),
    }
}

fn respond_error(status: u16, error: &str, message: impl Into<String>) -> HttpOutV1 {
    respond_json(
        status,
        json!({
            "error": error,
            "message": message.into(),
        }),
    )
}

fn respond_rate_limited(retry_after_seconds: i64) -> HttpOutV1 {
    let mut headers = json_headers();
    headers.push(Header {
        name: "Retry-After".to_string(),
        value: retry_after_seconds.to_string(),
    });
    respond_json_with_headers(
        429,
        json!({
            "error": "rate_limited",
            "message": "token rate limit exceeded",
            "retry_after": retry_after_seconds,
        }),
        headers,
    )
}

fn respond_bad_request(message: &str) -> HttpOutV1 {
    respond_error(400, "bad_request", message)
}

fn respond_not_found(message: &str) -> HttpOutV1 {
    respond_error(404, "not_found", message)
}

fn method_not_allowed() -> HttpOutV1 {
    respond_error(
        405,
        "method_not_allowed",
        "method not allowed on this endpoint",
    )
}

fn respond_not_implemented() -> HttpOutV1 {
    respond_error(501, "not_implemented", "streaming not supported")
}

fn respond_unauthorized(message: &str) -> HttpOutV1 {
    respond_error(401, "unauthorized", message)
}

fn respond_forbidden(message: &str) -> HttpOutV1 {
    respond_error(403, "forbidden", message)
}

fn respond_cors_preflight() -> HttpOutV1 {
    HttpOutV1 {
        status: 204,
        headers: json_headers(),
        body_b64: String::new(),
        events: Vec::new(),
    }
}

/// Build the WebSocket `streamUrl` advertised to BotFramework-WebChat clients.
///
/// The URL must match the host's WS upgrade route, which is registered under
/// the tenant-scoped prefix `/v1/messaging/webchat/{tenant}/v3/directline/...`.
/// Token rides in the `t` query param because browser WebSocket APIs cannot
/// set custom headers.
///
/// `watermark=-1` instructs the server to replay all activities since the
/// conversation began (the host parses negative or missing values as 0).
fn build_stream_url(tenant: &str, conversation_id: &str, token: &str) -> String {
    format!(
        "{base}/v1/messaging/webchat/{tenant}/v3/directline/conversations/{conv_id}/stream?watermark=-1&t={token}",
        base = public_base_url_or_relative(),
        tenant = encode(tenant),
        conv_id = conversation_id,
        token = encode(token),
    )
}

/// Resolve the public base URL for the streamUrl.
///
/// WASM components cannot read host environment variables directly; supplying
/// an absolute URL would require a host import that does not yet exist. For
/// the MVP we return an empty string so the resulting `streamUrl` is a
/// site-relative path. Browsers (and the BotFramework WebChat SDK) resolve
/// such paths against the page origin and automatically upgrade `http(s)://`
/// to `ws(s)://` when opening the WebSocket.
fn public_base_url_or_relative() -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose;
    use serde_json::json;
    use std::collections::HashMap;

    struct InMemoryStateStore {
        data: HashMap<String, Vec<u8>>,
    }

    impl InMemoryStateStore {
        fn new() -> Self {
            Self {
                data: HashMap::new(),
            }
        }
    }

    impl StateStore for InMemoryStateStore {
        fn read(&mut self, key: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.data.get(key).cloned())
        }

        fn write(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
            self.data.insert(key.to_string(), value.to_vec());
            Ok(())
        }
    }

    struct TestSecretStore {
        data: HashMap<String, Vec<u8>>,
    }

    impl TestSecretStore {
        fn new() -> Self {
            Self {
                data: HashMap::new(),
            }
        }

        fn insert(&mut self, key: &str, value: &[u8]) {
            self.data.insert(key.to_string(), value.to_vec());
        }
    }

    impl SecretStore for TestSecretStore {
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.data.get(key).cloned())
        }
    }

    fn build_request(
        method: &str,
        path: &str,
        query: Option<&str>,
        body: Option<&Value>,
        headers: Vec<Header>,
    ) -> Result<HttpInV1, String> {
        let body_b64 = match body {
            Some(payload) => general_purpose::STANDARD
                .encode(serde_json::to_vec(payload).map_err(|err| err.to_string())?),
            None => String::new(),
        };
        Ok(HttpInV1 {
            method: method.to_string(),
            path: path.to_string(),
            query: query.map(|value| value.to_string()),
            headers,
            body_b64,
            route_hint: None,
            binding_id: None,
            config: None,
        })
    }

    fn decode_body(response: &HttpOutV1) -> Result<Value, String> {
        let bytes = general_purpose::STANDARD
            .decode(&response.body_b64)
            .map_err(|err| err.to_string())?;
        serde_json::from_slice(&bytes).map_err(|err| err.to_string())
    }

    #[test]
    fn directline_polling_flow() -> Result<(), String> {
        let mut state = InMemoryStateStore::new();
        let mut secrets = TestSecretStore::new();
        secrets.insert(TOKEN_SECRET_KEY, b"test-secret");

        let token_request = build_request(
            "POST",
            "/v3/directline/tokens/generate",
            Some("env=default&tenant=default"),
            Some(&json!({"user": {"id": "alice"}})),
            vec![],
        )?;
        let token_response = handle_directline_request(&token_request, &mut state, &secrets);
        assert_eq!(token_response.status, 200);
        let token_body = decode_body(&token_response)?;
        let user_token = token_body["token"].as_str().ok_or("token returned")?;

        let conversation_request = build_request(
            "POST",
            "/v3/directline/conversations",
            None,
            None,
            vec![Header {
                name: "Authorization".into(),
                value: format!("Bearer {user_token}"),
            }],
        )?;
        let conversation_response =
            handle_directline_request(&conversation_request, &mut state, &secrets);
        assert_eq!(conversation_response.status, 201);
        let conversation_body = decode_body(&conversation_response)?;
        let conversation_id = conversation_body["conversationId"]
            .as_str()
            .ok_or("conversation id")?;
        let conv_token = conversation_body["token"]
            .as_str()
            .ok_or("conversation token")?;

        let reuse_response = handle_directline_request(
            &build_request(
                "POST",
                "/v3/directline/conversations",
                None,
                None,
                vec![Header {
                    name: "Authorization".into(),
                    value: format!("Bearer {conv_token}"),
                }],
            )?,
            &mut state,
            &secrets,
        );
        assert_eq!(reuse_response.status, 403);

        let activity = json!({
            "type": "message",
            "text": "hello",
            "from": {"id": "alice"},
        });
        let post_activity_response = handle_directline_request(
            &build_request(
                "POST",
                &format!("/v3/directline/conversations/{conversation_id}/activities"),
                None,
                Some(&activity),
                vec![Header {
                    name: "Authorization".into(),
                    value: format!("Bearer {conv_token}"),
                }],
            )?,
            &mut state,
            &secrets,
        );
        assert_eq!(post_activity_response.status, 201);
        let posted = decode_body(&post_activity_response)?;
        assert!(posted.get("id").is_some());

        let get_response = handle_directline_request(
            &build_request(
                "GET",
                &format!("/v3/directline/conversations/{conversation_id}/activities"),
                None,
                None,
                vec![Header {
                    name: "Authorization".into(),
                    value: format!("Bearer {conv_token}"),
                }],
            )?,
            &mut state,
            &secrets,
        );
        assert_eq!(get_response.status, 200);
        let get_body = decode_body(&get_response)?;
        let activities = get_body["activities"]
            .as_array()
            .ok_or("activities returned")?;
        assert_eq!(activities.len(), 1);
        assert_eq!(get_body["watermark"], Value::String("1".to_string()));

        let empty_response = handle_directline_request(
            &build_request(
                "GET",
                &format!("/v3/directline/conversations/{conversation_id}/activities"),
                Some("watermark=1"),
                None,
                vec![Header {
                    name: "Authorization".into(),
                    value: format!("Bearer {conv_token}"),
                }],
            )?,
            &mut state,
            &secrets,
        );
        assert_eq!(empty_response.status, 200);
        let empty_body = decode_body(&empty_response)?;
        assert!(
            empty_body["activities"]
                .as_array()
                .ok_or("activities returned")?
                .is_empty()
        );
        assert_eq!(empty_body["watermark"], Value::String("1".to_string()));

        let refresh_response = handle_directline_request(
            &build_request(
                "POST",
                "/v3/directline/tokens/refresh",
                None,
                None,
                vec![Header {
                    name: "Authorization".into(),
                    value: format!("Bearer {conv_token}"),
                }],
            )?,
            &mut state,
            &secrets,
        );
        assert_eq!(refresh_response.status, 200);
        let refresh_body = decode_body(&refresh_response)?;
        assert_eq!(
            refresh_body["conversationId"],
            Value::String(conversation_id.to_string())
        );
        assert!(refresh_body["token"].as_str().is_some());

        let wrong_conv_response = handle_directline_request(
            &build_request(
                "POST",
                "/v3/directline/conversations/other/activities",
                None,
                Some(&activity),
                vec![Header {
                    name: "Authorization".into(),
                    value: format!("Bearer {conv_token}"),
                }],
            )?,
            &mut state,
            &secrets,
        );
        assert_eq!(wrong_conv_response.status, 403);
        Ok(())
    }

    fn token_request_with_ip(client_ip: &str) -> Result<HttpInV1, String> {
        build_request(
            "POST",
            "/v3/directline/tokens/generate",
            Some("env=default&tenant=default"),
            None,
            vec![Header {
                name: "X-Forwarded-For".into(),
                value: client_ip.to_string(),
            }],
        )
    }

    fn header_value<'a>(response: &'a HttpOutV1, name: &str) -> Option<&'a str> {
        response
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }

    #[test]
    fn anonymous_visitors_with_distinct_ips_get_independent_buckets() -> Result<(), String> {
        let mut state = InMemoryStateStore::new();
        let mut secrets = TestSecretStore::new();
        secrets.insert(TOKEN_SECRET_KEY, b"test-secret");

        // Send the default per-window cap from one IP — every call should
        // succeed because the bucket only fills for that IP.
        for _ in 0..RATE_LIMIT_REQUESTS_DEFAULT {
            let resp = handle_directline_request(
                &token_request_with_ip("203.0.113.10")?,
                &mut state,
                &secrets,
            );
            assert_eq!(resp.status, 200, "first IP should not be rate-limited");
        }

        // A different IP arriving immediately afterwards must still get a
        // fresh bucket — pre-fix this would have been blocked because
        // both anonymous visitors collapsed onto the same `anonymous`
        // bucket key.
        let other_ip_resp = handle_directline_request(
            &token_request_with_ip("198.51.100.20")?,
            &mut state,
            &secrets,
        );
        assert_eq!(
            other_ip_resp.status, 200,
            "distinct anonymous IP must not share rate-limit bucket"
        );
        Ok(())
    }

    #[test]
    fn xff_chain_uses_first_hop_for_bucket() -> Result<(), String> {
        let mut state = InMemoryStateStore::new();
        let mut secrets = TestSecretStore::new();
        secrets.insert(TOKEN_SECRET_KEY, b"test-secret");

        for _ in 0..RATE_LIMIT_REQUESTS_DEFAULT {
            assert_eq!(
                handle_directline_request(
                    &token_request_with_ip("203.0.113.10, 10.0.0.1")?,
                    &mut state,
                    &secrets,
                )
                .status,
                200
            );
        }

        // Same first hop, different proxy chain — must be the *same*
        // bucket because identity is decided by the leftmost hop only.
        let blocked = handle_directline_request(
            &token_request_with_ip("203.0.113.10, 10.0.0.99")?,
            &mut state,
            &secrets,
        );
        assert_eq!(blocked.status, 429);
        Ok(())
    }

    #[test]
    fn rate_limit_response_includes_retry_after_header() -> Result<(), String> {
        let mut state = InMemoryStateStore::new();
        let mut secrets = TestSecretStore::new();
        secrets.insert(TOKEN_SECRET_KEY, b"test-secret");

        for _ in 0..RATE_LIMIT_REQUESTS_DEFAULT {
            assert_eq!(
                handle_directline_request(
                    &token_request_with_ip("203.0.113.55")?,
                    &mut state,
                    &secrets,
                )
                .status,
                200
            );
        }

        let blocked = handle_directline_request(
            &token_request_with_ip("203.0.113.55")?,
            &mut state,
            &secrets,
        );
        assert_eq!(blocked.status, 429);
        let retry_after = header_value(&blocked, "Retry-After")
            .ok_or("Retry-After header must be present on 429")?;
        let secs: i64 = retry_after
            .parse()
            .map_err(|err| format!("Retry-After must be integer seconds: {err}"))?;
        assert!(
            (1..=RATE_LIMIT_WINDOW_SECONDS_DEFAULT).contains(&secs),
            "Retry-After should be within current window, got {secs}"
        );

        let body = decode_body(&blocked)?;
        assert_eq!(body["error"], "rate_limited");
        assert!(body.get("retry_after").is_some());
        Ok(())
    }

    #[test]
    fn rate_limit_config_override_raises_cap() {
        let mut state = InMemoryStateStore::new();
        let mut secrets = TestSecretStore::new();
        secrets.insert(TOKEN_SECRET_KEY, b"test-secret");

        // Config raises the per-window cap to 200; first 200 requests pass.
        let make = || HttpInV1 {
            method: "POST".into(),
            path: "/v3/directline/tokens/generate".into(),
            query: Some("env=default&tenant=default".into()),
            headers: vec![Header {
                name: "X-Forwarded-For".into(),
                value: "203.0.113.99".into(),
            }],
            body_b64: String::new(),
            route_hint: None,
            binding_id: None,
            config: Some(json!({ "rate_limit_requests": 200 })),
        };

        for _ in 0..200 {
            assert_eq!(
                handle_directline_request(&make(), &mut state, &secrets).status,
                200
            );
        }
        let blocked = handle_directline_request(&make(), &mut state, &secrets);
        assert_eq!(blocked.status, 429);
    }

    #[test]
    fn determine_subject_prefers_user_id_over_ip() -> Result<(), String> {
        let request = build_request(
            "POST",
            "/v3/directline/tokens/generate",
            None,
            Some(&json!({ "user": { "id": "guest-abc" } })),
            vec![Header {
                name: "X-Forwarded-For".into(),
                value: "203.0.113.10".into(),
            }],
        )?;
        let body = decode_json_body(&request).map_err(|out| format!("status {}", out.status))?;
        let subject = determine_rate_limit_subject(&request, &body);
        assert_eq!(subject, RateLimitSubject::User("guest-abc".to_string()));
        Ok(())
    }

    #[test]
    fn determine_subject_falls_back_to_anonymous_without_ip() -> Result<(), String> {
        let request = build_request("POST", "/v3/directline/tokens/generate", None, None, vec![])?;
        let body = decode_json_body(&request).map_err(|out| format!("status {}", out.status))?;
        let subject = determine_rate_limit_subject(&request, &body);
        assert_eq!(subject, RateLimitSubject::Anonymous);
        Ok(())
    }

    #[test]
    fn extract_client_ip_priority_order() {
        let headers = vec![
            Header {
                name: "X-Forwarded-For".into(),
                value: "1.1.1.1".into(),
            },
            Header {
                name: "X-Real-IP".into(),
                value: "2.2.2.2".into(),
            },
            Header {
                name: "True-Client-IP".into(),
                value: "3.3.3.3".into(),
            },
        ];
        // True-Client-IP wins (highest priority — Cloudflare/Akamai trust
        // boundary).
        assert_eq!(extract_client_ip(&headers).as_deref(), Some("3.3.3.3"));
    }

    #[test]
    fn empty_user_id_falls_through_to_ip_bucket() -> Result<(), String> {
        // Pre-fix bug: a body with `"user":{"id":""}` would be accepted as
        // the literal user id "" → `webchat:rate:tokens:..::` shared
        // bucket. Now we trim and treat empty as missing.
        let request = build_request(
            "POST",
            "/v3/directline/tokens/generate",
            None,
            Some(&json!({ "user": { "id": "   " } })),
            vec![Header {
                name: "X-Real-IP".into(),
                value: "203.0.113.77".into(),
            }],
        )?;
        let body = decode_json_body(&request).map_err(|out| format!("status {}", out.status))?;
        match determine_rate_limit_subject(&request, &body) {
            RateLimitSubject::Ip(_) => {}
            other => panic!("expected Ip bucket, got {other:?}"),
        }
        Ok(())
    }
}
