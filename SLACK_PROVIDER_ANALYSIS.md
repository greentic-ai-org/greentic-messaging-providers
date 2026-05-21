# Slack Provider Implementation Analysis

## Executive Summary

The `messaging-provider-slack` component has a **modern, well-architected implementation** with most foundational pieces in place for app reconciliation, BUT there are **significant gaps and mismatches** between:
- What the QA system expects (SETUP_QUESTIONS includes app_id, config tokens)
- What the config schema declares
- What ProviderConfig actually stores
- What apply_answers handles

## 1. Directory Structure

### components/messaging-provider-slack/
```
src/
  lib.rs                    # Component trait impls, apply_answers_impl, dispatch
  config.rs                 # ProviderConfig, ProviderConfigOut, secret loading
  describe.rs               # I18N_KEYS, SETUP_QUESTIONS, schema builders, QA spec
  ops/
    mod.rs                  # Operation exports, envelope builders
    render.rs               # render_plan (step 1)
    encode.rs               # encode_op (step 2)
    send.rs                 # handle_send, send_payload (step 3)
    ingest.rs               # ingest_http webhook handler
    modal.rs                # Slack modal/view submission support
    webhook.rs              # setup_webhook (MANIFEST UPDATE LOGIC)
    blockkit/               # Adaptive Card → Block Kit conversion
```

### components/slack/ (older component)
```
src/
  lib.rs                    # Legacy simpler implementation
  bindings.rs               
```

## 2. Current setup_webhook Implementation

**Status**: ✅ **FULLY IMPLEMENTED**

[Read full webhook.rs](components/messaging-provider-slack/src/ops/webhook.rs)

### The Flow

```
setup_webhook(input_json)
  ├─ Parse input JSON (app_id, config_token, refresh_token, public_base_url, etc.)
  ├─ Validate inputs (app_id and config_token required, public_base_url must be https://)
  ├─ Export manifest via apps.manifest.export
  │  ├─ If auth error → try_refresh_token()
  │  │   ├─ Call tooling.tokens.rotate with refresh_token
  │  │   ├─ Save new tokens to secrets store
  │  │   └─ Retry export with new token
  │  └─ If other error → return error
  ├─ Update manifest URLs in-place:
  │  ├─ Set event_subscriptions.request_url = webhook_url
  │  ├─ Set interactivity.request_url = webhook_url
  │  ├─ Add "im:history" to oauth_config.scopes.bot
  │  ├─ Add "message.im" to event_subscriptions.bot_events
  │  └─ Enable app_home.messages_tab and interactivity.is_enabled
  ├─ Push updated manifest via apps.manifest.update
  └─ Return success with webhook_url and Slack response
```

### Key Features

- **Token rotation**: Uses `tooling.tokens.rotate` endpoint when token expires
- **Automatic retry**: On auth error, refreshes token and retries manifest export
- **In-place manifest mutation**: Updates URLs and scopes within existing app manifest
- **Webhook URL construction**: `{public_base_url}/v1/messaging/ingress/{provider_id}/{tenant}/{team}`
- **URL encoding**: Custom minimal percent-encoder for form-urlencoded refresh token

### Constants Used

```rust
DEFAULT_APP_ID_KEY = "SLACK_APP_ID"
DEFAULT_CONFIG_ACCESS_TOKEN_KEY = "SLACK_CONFIGURATION_ACCESS_TOKEN"
DEFAULT_CONFIG_REFRESH_TOKEN_KEY = "SLACK_CONFIGURATION_REFRESH_TOKEN"
```

## 3. Current Config Handling

### ProviderConfig (input struct in config.rs)

Deserializes from JSON input:
```rust
pub struct ProviderConfig {
    pub enabled: bool,
    pub default_channel: Option<String>,
    pub public_base_url: String,
    pub api_base_url: Option<String>,
    pub bot_token: String,  // Can be inline or read from secrets
}
```

**MISSING**: `slack_app_id`, `slack_configuration_access_token`, `slack_configuration_refresh_token`

### ProviderConfigOut (output struct in config.rs)

Serialized for responses:
```rust
pub struct ProviderConfigOut {
    pub enabled: bool,
    pub default_channel: Option<String>,
    pub public_base_url: String,
    pub api_base_url: String,
    pub bot_token: String,  // skip_serializing_if empty
}
```

**MISSING SAME**: No app_id or config token fields

### Secret Resolution (config.rs)

```rust
pub fn resolve_bot_token(cfg: &ProviderConfig) -> String {
    if !cfg.bot_token.trim().is_empty() {
        return cfg.bot_token.clone();
    }
    get_secret_string(DEFAULT_BOT_TOKEN_KEY).unwrap_or_default()
}

pub fn get_secret_string(key: &str) -> Result<String, String> {
    secrets_store::get(key)  // WASI secrets_store interface
}

pub fn put_secret_string(key: &str, value: &str) {
    secrets_store::put(key, value.as_bytes());
}
```

### load_config Logic (config.rs)

Supports both nested and flat config:
```json
// Nested form
{"config": {"bot_token": "xyz", "public_base_url": "..."}}

// Flat form
{"bot_token": "xyz", "public_base_url": "...", "default_channel": "C123"}
```

**Allowed keys for flat form**:
`"enabled", "default_channel", "public_base_url", "api_base_url", "bot_token"`

**MISSING**: No handling of `slack_app_id`, `slack_configuration_access_token`, `slack_configuration_refresh_token` in the flat form allowed keys list!

## 4. Current Operations

### describe.rs — Registered Operations

```
"run"           → dispatch: handle_send(input, false)
"send"          → dispatch: handle_send(input, false)
"reply"         → dispatch: handle_send(input, true)
"ingest_http"   → dispatch: ingest_http(input)
"render_plan"   → dispatch: render_plan(input)
"encode"        → dispatch: encode_op(input)
"send_payload"  → dispatch: send_payload(input)
```

**NOT registered in dispatch**: `setup_webhook` — but it IS in describe.rs as part of SETUP_QUESTIONS!

### setup_webhook Operation

- **Implemented**: ✅ Full webhook.rs
- **Registered in dispatch**: ✅ Added to dispatch_json_invoke
- **Exposed in describe()**: ❌ NOT in describe_payload operations list

Looking at lib.rs dispatch_json_invoke():
```rust
fn dispatch_json_invoke(op: &str, input_json: &[u8]) -> Vec<u8> {
    match op {
        "run" | "send" => handle_send(input_json, false),
        "reply" => handle_send(input_json, true),
        "ingest_http" => ingest_http(input_json),
        "render_plan" => render_plan(input_json),
        "encode" => encode_op(input_json),
        "send_payload" => send_payload(input_json),
        "setup_webhook" => setup_webhook(input_json),  // ✅ HERE
        other => json_bytes(&json!({"ok": false, "error": format!("unsupported op: {other}")})),
    }
}
```

## 5. Existing App Reconciliation Logic

### Status: ❌ DOES NOT EXIST

The only app-related operation is **setup_webhook**, which:
- **Updates** an existing Slack app's manifest
- Does NOT create new apps
- Does NOT provision apps
- Does NOT handle initial OAuth/app creation

### What IS there:

- Token refresh/rotation logic in webhook.rs
- Manifest URL updates and scope injection
- Webhook URL path formatting
- Error handling for auth failures

### What's NOT there:

- App creation via Slack API (apps.manifest.create)
- Manifest generation from scratch
- Client ID/secret generation or OAuth flow
- "Add to Slack" button HTML generation
- OAuth authorization code exchange
- Provisioning helpers

## 6. QA Spec and Secret Requirements

### describe.rs — SETUP_QUESTIONS

```rust
I18N_KEYS includes:
  "slack.qa.setup.slack_app_id"
  "slack.qa.setup.slack_configuration_access_token"
  "slack.qa.setup.slack_configuration_refresh_token"

SETUP_QUESTIONS = [
    ("enabled", "slack.qa.setup.enabled", true),
    ("public_base_url", "slack.qa.setup.public_base_url", true),
    ("bot_token", "slack.qa.setup.bot_token", true),
    ("default_channel", "slack.qa.setup.default_channel", false),
    ("slack_app_id", "slack.qa.setup.slack_app_id", true),  // ✅ REQUIRED
    ("slack_configuration_access_token", "...", true),      // ✅ REQUIRED
    ("slack_configuration_refresh_token", "...", true),     // ✅ REQUIRED
]
```

### I18N Descriptions

```rust
"slack.qa.setup.slack_app_id" → "Slack App ID"
"slack.schema.config.slack_app_id.description" → 
  "App ID from api.slack.com/apps (e.g. A07XXXXXX). 
   Required for auto-configuring event subscriptions."

"slack.schema.config.slack_configuration_access_token.description" →
  "Short-lived configuration access token from api.slack.com/apps settings. 
   Used to update your app manifest automatically."

"slack.schema.config.slack_configuration_refresh_token.description" →
  "Refresh token for rotating expired configuration tokens. 
   Generated alongside the configuration token at api.slack.com/apps settings."
```

### Schema — MISSING Fields

```rust
// config_schema() in describe.rs does NOT include:
// - slack_app_id
// - slack_configuration_access_token
// - slack_configuration_refresh_token
// - bot_token

fn config_schema() -> SchemaIr {
    schema_obj(
        "slack.schema.config.title",
        "slack.schema.config.description",
        vec![
            ("enabled", true, ...),
            ("default_channel", false, ...),
            ("public_base_url", true, ...),
            ("api_base_url", true, ...),
            // MISSING: slack_app_id, slack_configuration_access_token, slack_configuration_refresh_token
        ],
        false,  // not additional_properties_allowed
    )
}
```

⚠️ **MISMATCH**: SETUP_QUESTIONS requires these fields, but config_schema() doesn't declare them!

## 7. Setup Flow — apply_answers Implementation

### lib.rs — apply_answers_impl()

**For Mode::Setup or Mode::Default**:

```rust
if mode == Mode::Setup || mode == Mode::Default {
    merged.enabled = answers.get("enabled").and_then(Value::as_bool)...;
    merged.default_channel = optional_string_from(&answers, "default_channel")...;
    merged.public_base_url = string_or_default(&answers, "public_base_url", &merged.public_base_url);
    merged.api_base_url = string_or_default(&answers, "api_base_url", &merged.api_base_url);
    
    // These are moved to secrets_patch, not config
    collect_secret_answer(&answers, "bot_token", DEFAULT_BOT_TOKEN_KEY, &mut secrets_set);
    collect_secret_answer(&answers, "slack_app_id", DEFAULT_APP_ID_KEY, &mut secrets_set);
    collect_secret_answer(&answers, "slack_configuration_access_token", 
                         DEFAULT_CONFIG_ACCESS_TOKEN_KEY, &mut secrets_set);
    collect_secret_answer(&answers, "slack_configuration_refresh_token", 
                         DEFAULT_CONFIG_REFRESH_TOKEN_KEY, &mut secrets_set);
    
    merged.bot_token.clear();  // Don't include in config output
}
```

**For Mode::Upgrade**:
- Only updates fields that have answers
- Same secret collection logic

**Token Storage**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SecretsPatch {
    set: BTreeMap<String, String>,  // Keys like "SLACK_BOT_TOKEN", "SLACK_APP_ID"
    delete: Vec<String>,
}

ApplyAnswersResult {
    ok: true,
    config: Some(merged),           // Config only (no secrets)
    secrets_patch: Some(...),       // Secrets stored here
    ...
}
```

### Key Behavior

- **Secrets are NOT stored in ProviderConfigOut** — they go in secrets_patch
- **Secrets ARE read from secrets_store** in send/webhook operations
- **collect_secret_answer()** checks if answer key exists, then puts it in secrets_set
- **bot_token field is cleared** in ProviderConfigOut even if provided in answers

## 8. Manifest Handling

### What exists:

- **update_manifest_urls()** in webhook.rs — patches existing manifest JSON:
  ```
  manifest.features.app_home.messages_tab_enabled = true
  manifest.oauth_config.scopes.bot += "im:history"
  manifest.settings.event_subscriptions.request_url = webhook_url
  manifest.settings.event_subscriptions.bot_events += "message.im"
  manifest.settings.interactivity.request_url = webhook_url
  manifest.settings.interactivity.is_enabled = true
  ```

- **export_manifest()** — calls apps.manifest.export API
- **rotate_config_token()** — calls tooling.tokens.rotate API

### What does NOT exist:

- App manifest creation from scratch
- Manifest validation or schema
- Default manifest template
- Manifest generation based on capabilities
- Scope management helpers

## 9. OAuth Flow

### Current Status: ❌ NOT IMPLEMENTED

#### No OAuth at all:
- No OAuth token exchange endpoints
- No authorization code flow
- No "Add to Slack" button
- WebChat has [oauth.rs](components/messaging-provider-webchat/src/ops/oauth.rs) but Slack doesn't

#### What would need to be added:

For OAuth to work, you'd need:
1. **Manifest generation** — create an initial manifest with OAuth scopes
2. **App creation** — call apps.manifest.create or let admin create via Slack UI
3. **OAuth app config** — set redirect_uri, scopes in manifest
4. **Authorization endpoint** — generate "Add to Workspace" button
5. **Token exchange endpoint** — exchange authorization code for bot/configuration tokens
6. **Scope requirements** — bot scopes (chat:write, users:read), app configuration scopes

## 10. Token Rotation

### What's implemented:

✅ **rotate_config_token()** in webhook.rs:
- Calls `POST https://slack.com/api/tooling.tokens.rotate`
- Sends form-urlencoded `refresh_token=<token>`
- Returns `(new_token, new_refresh_token)`
- Stores new tokens in secrets_store via put_secret_string()

### How it's triggered:

In setup_webhook, if `apps.manifest.export` returns `invalid_auth`, `token_expired`, or `token_revoked`:
- Call try_refresh_token()
- Refresh logic is integrated, not separate operation
- Retry manifest export with new token

### Missing:

- Automatic rotation on every send (only happens on setup_webhook auth error)
- Standalone rotation operation
- Rotation schedule/trigger logic
- Configuration token expiry tracking

## Key Findings Summary

### ✅ Implemented
1. setup_webhook operation with full manifest update logic
2. Token rotation via tooling.tokens.rotate
3. QA apply_answers with secret collection
4. Config schema and validation
5. ingest_http handler for webhook events
6. render/encode/send pipeline
7. Webhook URL construction and validation

### ⚠️ Mismatches/Issues
1. **Config schema mismatch**: SETUP_QUESTIONS includes app_id and config tokens, but config_schema() doesn't declare them
2. **load_config() incomplete**: Doesn't include app_id tokens in flat-form allowed keys
3. **ProviderConfig missing fields**: Can't deserialize slack_app_id or config tokens from JSON
4. **setup_webhook not in operations list**: Implemented but not declared in describe_payload.operations
5. **Secrets vs Config confusion**: App ID and tokens stored only in secrets_patch, never in ProviderConfigOut

### ❌ Not Implemented
1. App creation/provisioning
2. OAuth authorization flow
3. "Add to Slack" button generation
4. Manifest generation from scratch
5. OAuth token exchange endpoint

## Assumptions That May Be Wrong

Based on PR context, common mistakes:

1. **Assumption**: Users provide slack_app_id and config tokens in answers
   - **Reality**: These are required per SETUP_QUESTIONS, but config_schema doesn't declare them
   - **Impact**: Schema validation might reject valid configs

2. **Assumption**: App ID and tokens stored in ProviderConfigOut
   - **Reality**: They're stored only in secrets_patch
   - **Impact**: Config serialization doesn't include these; they must be fetched from secrets_store at runtime

3. **Assumption**: Token rotation happens automatically on every send
   - **Reality**: Rotation only happens in setup_webhook if auth fails
   - **Impact**: Long-lived configuration tokens might expire between setup and deployment

4. **Assumption**: setup_webhook is exposed as a normal provider operation
   - **Reality**: It's in dispatch but not in describe_payload.operations
   - **Impact**: Might not be discoverable by orchestrators

## Recommendations

1. **Fix config_schema()**: Add slack_app_id, slack_configuration_access_token, slack_configuration_refresh_token
2. **Fix load_config()**: Add new fields to the allowed_keys list
3. **Update ProviderConfig/ProviderConfigOut**: Add (optional) fields for app_id and config tokens if they should persist
4. **Fix setup_webhook dispatch**: Add to describe_payload.operations if it should be public
5. **Document token rotation**: Clarify when/how tokens are refreshed
6. **Verify QA prompt flow**: Ensure setup questions guide users through OAuth or accept pre-created tokens
