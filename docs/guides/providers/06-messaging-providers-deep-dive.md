# Messaging Providers Deep Dive

## Repo Structure: greentic-messaging-providers

```
greentic-messaging-providers/
├── components/                      # WASM component source code
│   ├── messaging-provider-telegram/
│   ├── messaging-ingress-telegram/
│   ├── messaging-provider-webex/
│   ├── messaging-provider-webchat/
│   │   └── src/directline/         # Direct Line v3 protocol
│   ├── messaging-provider-slack/
│   ├── messaging-ingress-slack/
│   ├── messaging-provider-teams-graph/
│   ├── messaging-ingress-teams/
│   ├── messaging-provider-whatsapp/
│   ├── messaging-ingress-whatsapp/
│   ├── messaging-provider-email/
│   ├── messaging-provider-dummy/
│   └── provision/                   # Shared provision component
├── crates/
│   ├── component_questions/         # Legacy questions WASM component (to be replaced by greentic-qa)
│   ├── questions-cli/               # Native CLI for interactive setup questions
│   ├── messaging-core/              # Shared Message struct (minimal)
│   ├── messaging-cardkit/           # Card rendering library
│   ├── messaging-cardkit-bin/       # Cardkit CLI + HTTP server
│   ├── messaging-universal-dto/     # JSON DTOs for operator-provider protocol
│   └── greentic-messaging-packgen/  # Pack generator from specs
├── packs/                           # Built provider packs
│   ├── messaging-telegram/
│   ├── messaging-webex/
│   ├── messaging-webchat/
│   ├── messaging-slack/
│   ├── messaging-teams-graph/
│   ├── messaging-whatsapp/
│   ├── messaging-email/
│   └── messaging-dummy/
├── specs/providers/                 # Pack generation specs
│   ├── telegram.yaml
│   ├── webex.yaml
│   ├── webchat.yaml
│   ├── slack.yaml
│   ├── teams.yaml
│   ├── whatsapp.yaml
│   ├── email.yaml
│   └── dummy.yaml
├── scripts/
│   └── build-provider-wasms.sh
├── tools/
│   ├── sync_packs.sh
│   ├── build_packs_only.sh
│   └── publish_packs_oci.sh
└── wit/                             # Shared WIT interfaces
```

---

## Telegram Provider

### Component: messaging-provider-telegram

**File:** `components/messaging-provider-telegram/src/lib.rs` (~694 lines)

**WIT World:**
```wit
world messaging-provider-telegram {
    import http-client;      # outbound HTTP to Telegram Bot API
    import secrets-store;    # TELEGRAM_BOT_TOKEN
    export schema-core-api;  # provider interface
}
```

**Ops dispatch:**
```rust
fn invoke(op: &str, input: &[u8]) -> Vec<u8> {
    match op {
        "send"         => handle_send(input),
        "reply"        => handle_reply(input),
        "ingest_http"  => handle_ingest_http(input),
        "render_plan"  => handle_render_plan(input),
        "encode"       => handle_encode(input),
        "send_payload" => handle_send_payload(input),
        _ => error_response("unknown op"),
    }
}
```

**Send flow:**
1. Parse `ChannelMessageEnvelope` from input
2. Get `TELEGRAM_BOT_TOKEN` from secrets store
3. Build Telegram API payload: `{ chat_id, text, parse_mode, reply_markup }`
4. POST to `https://api.telegram.org/bot{token}/sendMessage`
5. Return message_id from response

**Render plan:** Returns TierD (basic text + open-URL only)

### Component: messaging-ingress-telegram

**File:** `components/messaging-ingress-telegram/src/lib.rs`

Separate WASM that handles Telegram webhook payloads:
- Parses Update JSON → extracts message/callback_query
- Returns `HttpOutV1` with normalized events

### Pack: messaging-telegram

**Key files in `packs/messaging-telegram/`:**
- `pack.yaml` - manifest with extensions
- `setup.yaml` - questions: public_base_url, default_chat_id, bot_token (secret)
- `flows/setup_default.ygtc` - emit_questions → collect → validate → apply → summary
- `schemas/messaging/telegram/public.config.schema.json`
- `fixtures/` - test fixture JSONs

---

## Webex Provider

### Component: messaging-provider-webex

**File:** `components/messaging-provider-webex/src/lib.rs`

**WIT World:**
```wit
world messaging-provider-webex {
    import http-client;      # outbound HTTP to Webex API
    import secrets-store;    # WEBEX_BOT_TOKEN
    export schema-core-api;
}
```

**Send flow:**
1. Parse envelope, get `WEBEX_BOT_TOKEN`
2. Determine destination: `roomId` OR `toPersonId` OR `toPersonEmail`
3. Build body: `{ roomId/toPersonId/toPersonEmail, text/markdown }`
4. POST to `https://webexapis.com/v1/messages`

**Adaptive Card support in send_payload:**
```rust
// If metadata contains "adaptive_card" key:
let body = json!({
    destination_key: destination_value,
    "attachments": [{
        "contentType": "application/vnd.microsoft.card.adaptive",
        "content": adaptive_card_json
    }]
});
```

**Ingest (ingest_http):**
1. Parse Webex webhook event
2. If `resource == "messages" && event == "created"`:
   - Make GET `/messages/{id}` to fetch full message (Webex webhooks don't include message body)
   - Requires bot token + http-client capability
3. Build ChannelMessageEnvelope from fetched message

**Render plan:** Returns TierC (text + images + facts + postbacks, no inputs)

### Pack: messaging-webex

- `setup.yaml` questions: `public_base_url`, `bot_token` (secret)
- No separate ingress component (mode: `default`)

---

## Webchat Provider

### Component: messaging-provider-webchat

**File:** `components/messaging-provider-webchat/src/lib.rs` + `src/directline/`

**WIT World:**
```wit
world messaging-provider-webchat {
    import state-store;      # conversation state persistence
    import secrets-store;    # jwt_signing_key
    export schema-core-api;
}
```

**NOTE: No `http-client` import.** All communication is via state store.

### Direct Line v3 Protocol (`src/directline/`)

Full REST API implementation for BotFramework-compatible webchat:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v3/directline/tokens/generate` | POST | Issue HMAC-SHA256 JWT |
| `/v3/directline/conversations` | POST | Create conversation |
| `/v3/directline/conversations/{id}/activities` | POST | Send message |
| `/v3/directline/conversations/{id}/activities` | GET | Poll for messages (watermark) |
| `/v3/directline/conversations/{id}/stream` | GET | 501 Not Implemented |

**JWT structure:**
```json
{
    "sub": "webchat-user",
    "conv_id": "<conversation_id>",
    "env": "<env>",
    "tenant": "<tenant>",
    "team": "<team>",
    "user": "<user>",
    "iss": "greentic-webchat",
    "exp": <now + 30min>,
    "iat": <now>
}
```

**State store keys:**
- `webchat:conv:{env}:{tenant}:{team}:{conv_id}` - conversation metadata
- `webchat:activities:{env}:{tenant}:{team}:{conv_id}` - message history

**Send path:** `send_payload` writes to state store → browser polls via GET activities

**Render plan:** Returns TierD (basic text only)

---

## Provision Component (Shared)

**File:** `components/provision/src/lib.rs`

Shared across all providers. Executes setup "plan actions":

```rust
// Action types
match action.action_type.as_str() {
    "config.set"  => state_store::write(&format!("config/{scope}/{key}"), value),
    "secrets.put" => state_store::write(&format!("secrets/{scope}/{key}"), value),
    _ => skip
}
```

Supports `dry_run` mode. Returns `ApplyResult` with per-action status.

---

## Questions Component (Legacy - to be replaced by greentic-qa)

**File:** `crates/component_questions/src/lib.rs`

Operations:
- `emit(id, spec_ref)` → read `setup.yaml` from WASM filesystem → return `QuestionsSpec` JSON
- `validate(spec_json, answers_json)` → validate answers against spec
- `example-answers(spec_json)` → generate example answers

**SetupSpec format (setup.yaml):**
```yaml
provider_id: telegram
version: 1
title: Telegram provider setup
questions:
  - name: public_base_url
    title: Public base URL
    kind: string        # string | bool | number | choice
    required: true
    secret: false
    validate:
      regex: "^https://"
  - name: bot_token
    title: Telegram bot token
    kind: string
    required: true
    secret: true        # hidden input, stored as secret
```

---

## Packgen Tool

**File:** `crates/greentic-messaging-packgen/src/main.rs`

Generates provider packs from spec YAML files:

```bash
greentic-messaging-packgen generate --spec specs/providers/telegram.yaml --out packs/messaging-telegram
greentic-messaging-packgen generate-all --spec-dir specs/providers --out packs
```

**Validation enforces:**
- Allowed flow names: `setup_default`, `diagnostics`, `requirements`, `verify_webhooks`, `sync_subscriptions`, `rotate_credentials`
- Allowed ops: `send`, `reply`, `ingest`, `subscription_ensure`, `subscription_renew`, `subscription_delete`
- `setup_default` and `requirements` are mandatory
- `write_to` fields must be `config:` or `secrets:` prefixed

**All 8 current specs use `source.pack_dir`** - they copy from existing pack directories rather than generating from scratch.

---

## Card Rendering (messaging-cardkit)

**File:** `crates/messaging-cardkit/src/lib.rs`

Self-contained MessageCard rendering library:
- `CardKit<P: ProfileSource>` wraps `MessageCardEngine` + platform profiles
- Profiles: `StaticProfiles` (hardcoded) or `PackProfiles` (from pack extension data)
- Renderers per platform: Slack blocks, Teams AC, Telegram HTML, Webex markdown, WhatsApp text, Webchat JSON

**CLI:** `crates/messaging-cardkit-bin/`
- `render` command: render a card for a specific provider
- `serve` command: HTTP preview server
