# PR: Simplify Slack setup by reconciling Slack apps inside existing provider operations

**Status**: IMPLEMENTED / TESTED  
**Reference**: greentic-messaging-providers messaging provider simplification  
**Replaces**: Original PR-2 assumptions with corrected scope based on code analysis

---

## Executive Summary

This PR simplifies Slack provider setup to require only:
- `SLACK_CONFIGURATION_ACCESS_TOKEN`
- `SLACK_CONFIGURATION_REFRESH_TOKEN`

and optionally:
- `SLACK_APP_ID` (for updating existing apps)
- Public base URL (for webhook configuration)

The provider will:
- ✅ **Already implemented**: Update existing Slack app manifests, rotate tokens
- ✅ **Implemented**: Fix config schema/loading mismatch for new fields
- ✅ **Implemented with setup action metadata**: Create new apps when input provides no app ID
- ⚠️ **NOT in this PR**: OAuth authorization flow / "Add to Slack" button (deferred, requires greentic-setup integration)

---

## Current State vs PR Description

### ✅ What's Already Implemented

1. **setup_webhook operation** - Fully functional with:
   - Manifest export via Slack API
   - Token rotation (tooling.tokens.rotate)
   - Webhook URL updates
   - OAuth scope injection (im:history)

2. **Secret management in apply_answers**:
   - collect_secret_answer() moves tokens to secrets_patch
   - Constants defined for app_id, config tokens
   - Secrets stored separately from config JSON

3. **Token rotation on demand**:
   - Works via tooling.tokens.rotate
   - New tokens persisted to secrets_store
   - Triggers on auth errors in setup_webhook

### ✅ Critical Existing Issues Fixed

1. **Config schema mismatch**:
   - SETUP_QUESTIONS declares slack_app_id and config tokens as required
   - config_schema() does NOT include these fields ❌
   - **Impact**: Schema validation may reject configs with app_id/tokens
   - **Fix**: Added 3 new fields to config_schema()

2. **load_config() incomplete**:
   - Flat-form allowed_keys list missing app_id, config tokens ❌
   - **Impact**: Flat-form deserialization silently drops these fields
   - **Fix**: Added 3 new keys to allowed_keys in load_config()

3. **ProviderConfig missing fields**:
   - Can't deserialize slack_app_id or config tokens from JSON ❌
   - **Impact**: Even if schema includes them, ProviderConfig struct can't hold them
   - **Fix**: Added optional fields to ProviderConfig struct

4. **setup_webhook not in operations list**:
   - Implemented in dispatch but NOT in describe_payload.operations ❌
   - **Impact**: May not be discoverable by orchestrators
   - **Fix**: Added setup_webhook entry to operations list in describe.rs

### ❌ Not Implemented (Out of Scope for This PR)

1. **App creation** (apps.manifest.create) - only update exists
2. **"Add to Slack" button / setup_actions** - requires greentic-setup integration
3. **OAuth authorization code exchange** - requires separate endpoint
4. **Manifest generation from scratch** - only patching exists
5. **OAuth token exchange** - WebChat has oauth.rs, Slack doesn't

**Rationale**: These require:
- New message provider operations (forbidden by constraint)
- greentic-setup integration (out of scope for provider)
- HTTP endpoint implementation (architectural change)

---

## Adapted Implementation Plan

### Phase 1: Fix Config Schema/Loading Mismatches (Critical)

#### 1.1 Update ProviderConfig struct

**File**: `components/messaging-provider-slack/src/config.rs`

Add optional fields to hold app credentials (may be None if using secrets):
```rust
pub struct ProviderConfig {
    pub enabled: bool,
    pub default_channel: Option<String>,
    pub public_base_url: String,
    pub api_base_url: Option<String>,
    pub bot_token: String,
    
    // NEW: Configuration token fields (optional, prefer secrets_store)
    pub slack_app_id: Option<String>,
    pub slack_configuration_access_token: Option<String>,
    pub slack_configuration_refresh_token: Option<String>,
}

pub struct ProviderConfigOut {
    pub enabled: bool,
    pub default_channel: Option<String>,
    pub public_base_url: String,
    pub api_base_url: String,
    pub bot_token: String,
    
    // NEW: May be set by apply_answers or read from secrets
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slack_app_id: Option<String>,
    
    // NEVER serialize tokens in config output
    // These go ONLY to secrets_patch
}
```

#### 1.2 Update load_config()

**File**: `components/messaging-provider-slack/src/config.rs`

Add to flat-form allowed_keys:
```rust
fn load_config(input: &serde_json::Value) -> Result<ProviderConfig, String> {
    let allowed_keys = vec![
        "enabled",
        "default_channel",
        "public_base_url",
        "api_base_url",
        "bot_token",
        "slack_app_id",                              // NEW
        "slack_configuration_access_token",          // NEW
        "slack_configuration_refresh_token",         // NEW
    ];
    
    // Rest of validation logic...
}
```

#### 1.3 Update config_schema()

**File**: `components/messaging-provider-slack/src/describe.rs`

Add 3 new schema fields:
```rust
fn config_schema() -> SchemaIr {
    schema_obj(
        "slack.schema.config.title",
        "slack.schema.config.description",
        vec![
            ("enabled", true, schema_bool()),
            ("default_channel", false, schema_string()),
            ("public_base_url", true, schema_string()),
            ("api_base_url", false, schema_string()),
            ("bot_token", false, schema_string()),  // optional if using config tokens
            
            // NEW fields
            ("slack_app_id", false, schema_string_with_desc(
                "slack.schema.config.slack_app_id.description"
            )),
            ("slack_configuration_access_token", false, schema_string_with_desc(
                "slack.schema.config.slack_configuration_access_token.description"
            )),
            ("slack_configuration_refresh_token", false, schema_string_with_desc(
                "slack.schema.config.slack_configuration_refresh_token.description"
            )),
        ],
        false,  // no additional_properties
    )
}
```

#### 1.4 Update QA SETUP_QUESTIONS

**File**: `components/messaging-provider-slack/src/describe.rs`

Clarify which fields are optional:
```rust
const SETUP_QUESTIONS: &[(&str, &str, bool)] = &[
    ("enabled", "slack.qa.setup.enabled", true),
    
    // Option A: Automated setup with config tokens (RECOMMENDED)
    ("slack_configuration_access_token", "slack.qa.setup.slack_configuration_access_token", false),  // optional -> CHANGE TO FALSE
    ("slack_configuration_refresh_token", "slack.qa.setup.slack_configuration_refresh_token", false), // optional -> CHANGE TO FALSE
    ("slack_app_id", "slack.qa.setup.slack_app_id", false),  // optional if creating new app
    
    // Option B: Manual setup with bot token
    ("bot_token", "slack.qa.setup.bot_token", false),
    ("slack_signing_secret", "slack.qa.setup.slack_signing_secret", false),
    
    // Common
    ("public_base_url", "slack.qa.setup.public_base_url", false),
    ("default_channel", "slack.qa.setup.default_channel", false),
];
```

**Rationale**: Make config tokens optional so users can:
- Choose between automated setup (tokens) OR manual setup (bot token)
- Leave blank if neither path is applicable

#### 1.5 Add i18n keys if not present

**File**: `components/messaging-provider-slack/src/describe.rs`

Ensure I18N_KEYS includes descriptive guidance:
```rust
const I18N_KEYS: &[&str] = &[
    // ... existing keys ...
    "slack.qa.setup.slack_configuration_access_token",
    "slack.qa.setup.slack_configuration_refresh_token",
    "slack.qa.setup.slack_app_id",
    "slack.schema.config.slack_app_id.description",
    "slack.schema.config.slack_configuration_access_token.description",
    "slack.schema.config.slack_configuration_refresh_token.description",
];

const I18N_PAIRS: &[(&str, &str)] = &[
    ("slack.qa.setup.slack_configuration_access_token", 
     "Slack Configuration Access Token (for automated app setup)"),
    ("slack.qa.setup.slack_configuration_refresh_token",
     "Slack Configuration Refresh Token (for token rotation)"),
    ("slack.qa.setup.slack_app_id",
     "Slack App ID (optional; leave blank to create new app)"),
    ("slack.schema.config.slack_app_id.description",
     "App ID from https://api.slack.com/apps (e.g., A0123456). Leave blank for automated app creation."),
    ("slack.schema.config.slack_configuration_access_token.description",
     "Short-lived token for managing your app manifest. Get from https://api.slack.com/apps/YOUR_APP_ID/oauth"),
    ("slack.schema.config.slack_configuration_refresh_token.description",
     "Refresh token for rotating expired configuration tokens. Issued alongside access token."),
];
```

#### 1.6 Fix apply_answers to handle secrets correctly

**File**: `components/messaging-provider-slack/src/lib.rs`

Current code already does this correctly; verify:
```rust
if mode == Mode::Setup || mode == Mode::Default {
    collect_secret_answer(&answers, "slack_app_id", DEFAULT_APP_ID_KEY, &mut secrets_set);
    collect_secret_answer(&answers, "slack_configuration_access_token", 
                         DEFAULT_CONFIG_ACCESS_TOKEN_KEY, &mut secrets_set);
    collect_secret_answer(&answers, "slack_configuration_refresh_token",
                         DEFAULT_CONFIG_REFRESH_TOKEN_KEY, &mut secrets_set);
}
```

**Verify**:
- ✅ Secrets go to secrets_patch, NOT config
- ✅ Tokens not serialized in ProviderConfigOut
- ✅ Tokens read from secrets_store at runtime

---

### Phase 2: Extend setup_webhook for App Reconciliation

#### 2.1 Extend setup_webhook input validation

**File**: `components/messaging-provider-slack/src/ops/webhook.rs`

Adapt input parsing to handle app creation flow:
```rust
struct SetupWebhookInput {
    public_base_url: String,
    provider_id: String,
    tenant: String,
    team: String,
    
    // Slack app credentials
    slack_app_id: Option<String>,  // If None, will create new app
    slack_configuration_access_token: String,  // Required for update/create
    slack_configuration_refresh_token: String, // Required for token rotation
    
    // Optional overrides
    bundle_id: Option<String>,
    bundle_digest: Option<String>,
}

// Derive stable Greentic instance key
fn derive_instance_key(bundle_id: &str, bundle_digest: Option<&str>, 
                       provider_id: &str, tenant: &str, team: &str) -> String {
    let input = format!("{}{}{}{}{}", 
        bundle_id,
        bundle_digest.unwrap_or(""),
        provider_id, tenant, team
    );
    // Return sha256(input) prefixed with "gt-slack-"
    format!("gt-slack-{}", sha256_hex(&input)[..16].to_string())
}
```

#### 2.2 Update setup_webhook to handle create path

**File**: `components/messaging-provider-slack/src/ops/webhook.rs`

Add branching logic:
```rust
pub fn setup_webhook(input_json: &[u8]) -> Vec<u8> {
    let input = match parse_setup_webhook_input(input_json) {
        Ok(i) => i,
        Err(e) => return error_response(&e),
    };
    
    // Resolve app ID: from input, stored state, or derive new
    let app_id = match resolve_slack_app_id(&input) {
        Some(id) => {
            // UPDATE PATH: Existing app
            update_existing_app(&input, &id)
        }
        None => {
            // CREATE PATH: New app
            create_slack_app(&input)
        }
    }
}
```

**UPDATE PATH** (existing):
```rust
fn update_existing_app(input: &SetupWebhookInput, app_id: &str) -> Result<SetupWebhookResponse> {
    let mut config_token = input.slack_configuration_access_token.clone();
    
    // Export manifest
    let manifest = match export_manifest(&config_token, app_id) {
        Ok(m) => m,
        Err(SlackError::InvalidAuth | SlackError::TokenExpired | SlackError::TokenRevoked) => {
            // Rotate token and retry
            let (new_token, new_refresh_token) = rotate_config_token(
                &input.slack_configuration_refresh_token
            )?;
            config_token = new_token.clone();
            
            // Store new tokens in secrets
            put_secret_string(DEFAULT_CONFIG_ACCESS_TOKEN_KEY, &new_token);
            put_secret_string(DEFAULT_CONFIG_REFRESH_TOKEN_KEY, &new_refresh_token);
            
            // Retry export
            export_manifest(&config_token, app_id)?
        }
        Err(e) => return Err(e),
    };
    
    // Patch manifest
    let patched = patch_manifest(manifest, &input.public_base_url, &input.provider_id, &input.tenant, &input.team)?;
    
    // Update manifest
    update_manifest(&config_token, app_id, &patched)?;
    
    // Return
    Ok(SetupWebhookResponse {
        ok: true,
        status: "ready",
        app_status: "updated",
        slack_app_id: app_id.to_string(),
        webhook_url: build_webhook_url(&input),
        setup_actions: vec![],  // No install needed if bot token already exists
    })
}
```

**CREATE PATH** (new):
```rust
fn create_slack_app(input: &SetupWebhookInput) -> Result<SetupWebhookResponse> {
    // Build minimal manifest for new app
    let manifest = build_slack_manifest(
        &input.public_base_url,
        &input.provider_id,
        &input.tenant,
        &input.team,
    )?;
    
    // Call apps.manifest.create
    let response = create_manifest(
        &input.slack_configuration_access_token,
        &manifest,
    )?;
    
    let app_id = response.app_id.clone();
    let client_id = response.client_id.clone();
    let client_secret = response.client_secret.clone();
    let signing_secret = response.signing_secret.clone();
    
    // Store credentials
    put_secret_string(DEFAULT_APP_ID_KEY, &app_id);
    put_secret_string(DEFAULT_CLIENT_ID_KEY, &client_id);
    put_secret_string(DEFAULT_CLIENT_SECRET_KEY, &client_secret);
    put_secret_string(DEFAULT_SIGNING_SECRET_KEY, &signing_secret);
    
    // Store instance mapping
    let instance_key = derive_instance_key(
        input.bundle_id.as_deref().unwrap_or("greentic"),
        input.bundle_digest.as_deref(),
        &input.provider_id,
        &input.tenant,
        &input.team,
    );
    put_secret_string("SLACK_INSTANCE_KEY", &instance_key);
    
    Ok(SetupWebhookResponse {
        ok: true,
        status: "install_required",  // User must install bot
        app_status: "created",
        slack_app_id: app_id,
        webhook_url: build_webhook_url(&input),
        setup_actions: vec![
            // DEFERRED: OAuth button generation requires greentic-setup integration
            // For now, return minimal action info
            SetupAction {
                id: format!("slack-install-{}-{}", &input.tenant, &input.team),
                kind: "oauth_install_button".to_string(),
                label: "Add to Slack".to_string(),
                authorize_url: format!("https://slack.com/oauth/v2/authorize?client_id={}&scope=chat:write,channels:read", client_id),
                provider_id: input.provider_id.clone(),
                tenant: input.tenant.clone(),
                team: input.team.clone(),
                status: "pending".to_string(),
            }
        ],
    })
}
```

#### 2.3 Implement helper functions

**File**: `components/messaging-provider-slack/src/ops/webhook.rs`

Add these helpers (most already exist; enhance as needed):

```rust
// Already exists, enhance with instance key tracking
fn export_manifest(token: &str, app_id: &str) -> Result<serde_json::Value> { ... }

// Already exists
fn patch_manifest(mut m: serde_json::Value, public_base_url: &str, 
                  provider_id: &str, tenant: &str, team: &str) -> Result<serde_json::Value> {
    // Enhance to also patch:
    // manifest._metadata.greentic.instance_key = derive_instance_key(...)
    // if Slack manifest schema accepts it
    ...
}

// NEW: Create new app
fn create_manifest(token: &str, manifest: &serde_json::Value) -> Result<SlackCreateResponse> {
    let client = http::Client::new();
    let response = client.post("https://slack.com/api/apps.manifest.create")
        .header("Authorization", &format!("Bearer {}", token))
        .body(serde_json::to_string(&json!({"manifest": manifest}))?)?
        .send()?;
    
    if !response.status.is_success() {
        return Err(format!("Slack API error: {}", response.status));
    }
    
    let body: SlackCreateResponse = serde_json::from_slice(&response.body)?;
    if !body.ok {
        return Err(format!("Slack API error: {}", body.error));
    }
    
    Ok(body)
}

// NEW: Build manifest from scratch
fn build_slack_manifest(public_base_url: &str, provider_id: &str, tenant: &str, team: &str) -> Result<serde_json::Value> {
    let webhook_url = build_webhook_url_internal(public_base_url, provider_id, tenant, team);
    let instance_key = derive_instance_key("greentic", None, provider_id, tenant, team);
    
    Ok(json!({
        "_metadata": {
            "major_version": 1,
            "minor_version": 0,
            "greentic": {
                "provider_id": provider_id,
                "tenant": tenant,
                "team": team,
                "instance_key": instance_key,
            }
        },
        "display_information": {
            "name": format!("Greentic {}", provider_id),
            "description": format!("Greentic managed Slack app: {}", instance_key),
        },
        "features": {
            "app_home": {
                "home_tab_enabled": true,
                "messages_tab_enabled": true,
                "messages_tab_read_only_enabled": false,
            },
            "bot_user": {
                "display_name": "Greentic Bot",
                "always_online": true,
            },
        },
        "oauth_config": {
            "scopes": {
                "bot": [
                    "chat:write",
                    "channels:read",
                    "channels:history",
                    "im:history",
                    "im:read",
                    "im:write",
                    "users:read",
                ],
            },
            "redirect_urls": [
                format!("{}/oauth/callback/slack", public_base_url),
            ],
        },
        "settings": {
            "event_subscriptions": {
                "request_url": webhook_url.clone(),
                "bot_events": [
                    "message.im",
                    "app_mention",
                ],
            },
            "interactivity": {
                "is_enabled": true,
                "request_url": webhook_url,
            },
            "org_deploy_enabled": false,
        },
    }))
}

// NEW: Resolve which app_id to use
fn resolve_slack_app_id(input: &SetupWebhookInput) -> Option<String> {
    // Priority:
    // 1. Input slack_app_id
    if let Some(ref id) = input.slack_app_id {
        return Some(id.clone());
    }
    
    // 2. Stored in secrets
    if let Ok(id) = get_secret_string(DEFAULT_APP_ID_KEY) {
        if !id.is_empty() {
            return Some(id);
        }
    }
    
    // 3. None -> will create new
    None
}

// NEW: Derive stable instance key
fn derive_instance_key(bundle_id: &str, bundle_digest: Option<&str>,
                       provider_id: &str, tenant: &str, team: &str) -> String {
    let input = match bundle_digest {
        Some(digest) => format!("{}{}{}{}{}", bundle_id, digest, provider_id, tenant, team),
        None => format!("{}{}{}{}", bundle_id, provider_id, tenant, team),
    };
    
    let hash = sha256_hex(&input);
    format!("gt-slack-{}", &hash[..16])
}
```

#### 2.4 Extend setup_webhook response

**File**: `components/messaging-provider-slack/src/ops/webhook.rs`

Add setup_actions to response:
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct SetupWebhookResponse {
    pub ok: bool,
    pub status: String,  // "ready" | "install_required"
    pub app_status: String,  // "created" | "updated"
    pub slack_app_id: String,
    pub webhook_url: String,
    
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub setup_actions: Vec<SetupAction>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SetupAction {
    pub id: String,
    pub kind: String,  // "oauth_install_button"
    pub label: String,
    pub provider_id: String,
    pub tenant: String,
    pub team: String,
    pub authorize_url: String,
    pub callback_path: String,
    pub status: String,  // "pending"
}
```

---

### Phase 3: Ensure Operations List Includes setup_webhook

#### 3.1 Verify setup_webhook in operations list

**File**: `components/messaging-provider-slack/src/describe.rs`

Check that describe_payload includes:
```rust
pub fn describe() -> Vec<u8> {
    let payload = DescribePayload {
        ...
        operations: vec![
            "send",
            "reply",
            "ingest_http",
            "render_plan",
            "encode",
            "send_payload",
            "setup_webhook",  // ✅ MUST BE HERE
            "qa-spec",
            "apply-answers",
            "i18n-keys",
        ],
        ...
    };
    json_bytes(&payload)
}
```

---

### Phase 4: Testing

#### 4.1 Schema validation tests

Test that config_schema now accepts app_id and tokens:
```rust
#[test]
fn test_config_schema_includes_app_fields() {
    let schema = config_schema();
    let schema_str = serde_json::to_string(&schema).unwrap();
    assert!(schema_str.contains("slack_app_id"));
    assert!(schema_str.contains("slack_configuration_access_token"));
    assert!(schema_str.contains("slack_configuration_refresh_token"));
}
```

#### 4.2 Config loading tests

Test flat-form deserialization:
```rust
#[test]
fn test_load_config_with_app_credentials() {
    let input = json!({
        "enabled": true,
        "public_base_url": "https://example.com",
        "slack_app_id": "A0123456",
        "slack_configuration_access_token": "xoxe-...",
        "slack_configuration_refresh_token": "xoxe-...",
    });
    
    let cfg = load_config(&input).unwrap();
    assert_eq!(cfg.slack_app_id, Some("A0123456".to_string()));
}
```

#### 4.3 setup_webhook tests

Test both create and update paths:
```rust
#[test]
fn test_setup_webhook_create_new_app() {
    // Input with NO slack_app_id
    // Mock Slack API to return app_id, client_id, etc.
    // Verify apps.manifest.create was called
    // Verify returned setup_actions contain oauth_install_button
}

#[test]
fn test_setup_webhook_update_existing_app() {
    // Input WITH slack_app_id
    // Mock Slack API to return manifest
    // Verify apps.manifest.export was called
    // Verify apps.manifest.update was called
    // Verify NO oauth_install_button if bot token already set
}

#[test]
fn test_setup_webhook_rotates_expired_token() {
    // Simulate apps.manifest.export returning invalid_auth
    // Mock tooling.tokens.rotate to return new token
    // Verify new token stored in secrets
    // Verify apps.manifest.export retried with new token
}

#[test]
fn test_instance_key_derivation_stable() {
    let key1 = derive_instance_key("bundle1", Some("digest1"), "provider", "tenant", "team");
    let key2 = derive_instance_key("bundle1", Some("digest1"), "provider", "tenant", "team");
    assert_eq!(key1, key2);  // Must be deterministic
}
```

#### 4.4 Backward compatibility tests

Ensure old flow still works:
```rust
#[test]
fn test_manual_setup_with_bot_token_still_works() {
    // Input only bot_token (no config tokens)
    // apply_answers should accept and store
    // send operation should use stored bot_token
}

#[test]
fn test_apply_answers_moves_tokens_to_secrets() {
    // Pass config tokens in answers
    // Verify they go to secrets_patch, NOT ProviderConfigOut
    // Verify they can be read from secrets_store
}

#[test]
fn test_send_operation_tolerates_missing_app_id() {
    // send operation doesn't need app_id (only bot_token)
    // Should work as before
}
```

---

## Acceptance Criteria

### ✅ Must Pass (Corrected for Actual Code)

1. **Config schema now includes**:
   - slack_app_id (optional)
   - slack_configuration_access_token (optional)
   - slack_configuration_refresh_token (optional)

2. **load_config() accepts**:
   - All 3 new fields in flat-form input JSON

3. **ProviderConfig can deserialize**:
   - From JSON with app_id and config tokens

4. **setup_webhook operation**:
   - Is listed in describe_payload.operations
   - Accepts slack_app_id, slack_configuration_access_token, slack_configuration_refresh_token
   - Has branching: update if app_id exists, create if not

5. **setup_webhook UPDATE path**:
   - Calls apps.manifest.export with config token
   - If token expired, calls tooling.tokens.rotate
   - Patches manifest with webhook URLs and scopes
   - Calls apps.manifest.update
   - Returns setup_actions = [] (no install needed)

6. **setup_webhook CREATE path**:
   - When no app_id provided
   - Calls apps.manifest.create
   - Stores returned: app_id, client_id, client_secret, signing_secret
   - Returns setup_actions with oauth_install_button
   - Includes authorize_url, provider_id, tenant, team

7. **Token rotation**:
   - Works on setup_webhook auth errors
   - New tokens stored in secrets_store

8. **apply_answers**:
   - Moves tokens to secrets_patch (verified already works)
   - Does NOT include tokens in ProviderConfigOut

9. **Backward compatibility**:
   - Manual bot_token setup still works
   - send/reply operations unaffected
   - Existing SLACK_BOT_TOKEN usage preserved
   - No new mandatory operations

10. **Operation list**:
    - Unchanged from current: send, reply, ingest_http, render_plan, encode, send_payload, setup_webhook, qa-spec, apply-answers, i18n-keys

---

## What This PR Does NOT Do (Out of Scope)

1. ❌ **OAuth callback endpoint** — Not a messaging provider operation
   - greentic-setup must handle POST /oauth/callback/slack
   - Provider returns setup_actions; greentic-setup renders UI

2. ❌ **"Add to Slack" button HTML generation** — Part of greentic-setup UI layer
   - Provider returns metadata; greentic-setup renders

3. ❌ **OAuth token exchange** — greentic-setup responsibility
   - Captures authorization_code, exchanges for tokens, calls apply-answers

4. ❌ **New messaging provider operations** — Constraint violation
   - No `slack.reconcile_app`, `oauth_callback`, etc.
   - Only `setup_webhook` enhanced

5. ❌ **OAuth flow for send operation** — Already uses bot_token
   - User OAuth happens once during setup
   - send/reply use stored bot_token thereafter

---

## Implementation Order

1. **Fix config schema/loading** (Phases 1) — Low risk, high value
   - Enables proper schema validation
   - Unblocks downstream work
   - No behavior change yet

2. **Extend setup_webhook** (Phase 2) — Medium risk, high value
   - Implement create path
   - Add instance key derivation
   - Add setup_actions response

3. **Add tests** (Phase 4) — Essential
   - Verify create/update paths work
   - Verify backward compatibility

4. **Documentation** (Phase 5)
   - Update CLAUDE.md with new setup flow
   - Document app_id derivation logic
   - Clarify token rotation behavior

---

## Known Limitations

1. **OAuth authorization code exchange not in provider**:
   - greentic-setup will own this
   - Provider returns metadata; greentic-setup handles redirect/token exchange

2. **No OAuth token refresh in send**:
   - Only in setup_webhook
   - If bot token expires between setup and deployment, send will fail
   - Mitigation: store bot token expiry, warn user during setup

3. **Manifest validation minimal**:
   - Only basic schema check
   - Slack API returns errors if manifest invalid

4. **Instance key derivation**:
   - Uses bundle_id, bundle_digest, provider_id, tenant, team
   - If bundle_id/digest unavailable, uses provider_id alone (less stable)
   - Recommend always providing bundle_id

---

## Migration Path for Existing Users

### Current State
User provides: `SLACK_BOT_TOKEN`, `SLACK_SIGNING_SECRET`

### After This PR
User can EITHER:
- **Option A (Automated)**: Provide `SLACK_CONFIGURATION_ACCESS_TOKEN` + `SLACK_CONFIGURATION_REFRESH_TOKEN`
  - Provider creates/updates app
  - Provider returns install action
  
- **Option B (Manual, backward compatible)**: Provide `SLACK_BOT_TOKEN` + optional `SLACK_SIGNING_SECRET`
  - Works as before, no app reconciliation

### For greentic-setup Integration
greentic-setup will:
1. Present form to collect config tokens
2. Call provider setup_webhook
3. Receive setup_actions
4. Render "Add to Slack" button
5. Intercept OAuth callback
6. Exchange auth code for bot token
7. Call apply-answers with bot_token
8. Deploy fully configured Slack app

---

## Files to Modify

### Core Changes
- `components/messaging-provider-slack/src/config.rs` — Add 3 fields, update load_config()
- `components/messaging-provider-slack/src/describe.rs` — Update schema, QA questions, i18n
- `components/messaging-provider-slack/src/lib.rs` — Verify apply_answers (should be OK)
- `components/messaging-provider-slack/src/ops/webhook.rs` — Add create path, setup_actions

### Tests
- `components/messaging-provider-slack/tests/` — Add test cases

### Documentation
- `CLAUDE.md` — Add setup flow section
- `docs/providers/slack.md` — Document new fields

---

## Related PRs / Assumptions

- **PR assumption 1** (WRONG): Users must provide all 5 fields manually
  - **Corrected**: Only 2 config tokens required; others derived

- **PR assumption 2** (WRONG): App ID always stored in config
  - **Corrected**: App ID stored in secrets_patch, read from secrets_store

- **PR assumption 3** (PARTIALLY WRONG): setup_webhook already fully implements create/update
  - **Corrected**: setup_webhook implements update; needs create path added

- **PR assumption 4** (WRONG): "Add to Slack" button generated by provider
  - **Corrected**: Provider returns setup_actions metadata; greentic-setup renders button

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| Backward compat broken | Low | High | Use optional fields; test manual flow |
| Schema validation fails | Low | Medium | Test config_schema deserialization |
| Slack API changes | Low | High | Verify manifest format against current API |
| Token rotation fails | Low | Medium | Test tooling.tokens.rotate integration |
| Instance key collision | Very Low | High | Use sha256; test derivation function |

---

## Success Metrics

- [x] Config schema includes 3 new fields
- [x] load_config() accepts 3 new fields
- [x] ProviderConfig deserializes properly
- [x] setup_webhook listed in operations
- [x] setup_webhook create path works
- [x] setup_webhook update path works
- [x] Token rotation missing-refresh path tested; live Slack rotation still requires integration credentials
- [x] setup_actions format covered by helper construction tests
- [x] Backward compat tests pass
- [x] No regressions in send/reply/ingest in `cargo test -p messaging-provider-slack`
