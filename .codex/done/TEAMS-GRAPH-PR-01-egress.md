# PR 1: Redesign Teams Provider As Graph-First Egress

## Review Status

Reviewed against the current codebase on 2026-05-27 and adapted.

This PR is valid and should be first. The current repository has partial Graph-era artifacts, but runtime egress is still Bot Framework:

- `components/messaging-provider-teams/src/lib.rs` sets `PROVIDER_TYPE` to `messaging.teams.bot`.
- `components/messaging-provider-teams/src/config.rs` is explicitly Bot Service-oriented and requires `ms_bot_app_id` plus `public_base_url`.
- `components/messaging-provider-teams/src/auth.rs` acquires Bot Framework tokens from the botframework scope and validates Bot Framework JWTs.
- `components/messaging-provider-teams/src/ops/send.rs` posts to Bot Connector `/v3/conversations/.../activities` URLs and calls `acquire_bot_token()`.
- `packs/messaging-teams/pack.yaml` and `pack.manifest.json` still advertise `provider_type: messaging.teams.bot`.

There is also an inconsistency to handle carefully: root `schemas/messaging/teams/*.schema.json` are already Graph-shaped, while `packs/messaging-teams/schemas/messaging/teams/config.schema.json` and the component QA/describe path are Bot-shaped.

## Title

Redesign Teams provider as Graph-first messaging provider

## Goal

Make the canonical Teams provider use Microsoft Graph for outbound messages and stop requiring Azure Bot Service, Bot Framework credentials, Bot Connector `serviceUrl`, or Bot Framework conversation IDs in the default egress path.

## Adapted Scope

Implement this in the provider component only:

- `components/messaging-provider-teams/src/lib.rs`
- `components/messaging-provider-teams/src/config.rs`
- `components/messaging-provider-teams/src/auth.rs`, or a new `graph_auth.rs`
- `components/messaging-provider-teams/src/ops/send.rs`
- focused tests in those files

Do not do broad setup/docs/schema cleanup here except for code-local expectations needed for tests. Pack metadata and setup UX belong in PR 2.

## Provider Type

Canonical provider type:

```text
messaging.teams.graph
```

Backward compatibility:

- Accept `messaging.teams.graph`.
- Accept `messaging.teams.bot` only as a low-cost legacy input alias during transition.
- Return `messaging.teams.graph` in new successful results.
- Reject non-Teams provider types in `send_payload`.

## Config Design

Replace the runtime config struct with:

```text
enabled: bool = true
public_base_url: Option<String>
tenant_id: String
client_id: String
refresh_token: Option<String>
client_secret: Option<String>
graph_base_url: String = "https://graph.microsoft.com/v1.0"
auth_base_url: String = "https://login.microsoftonline.com"
token_scope: String = "https://graph.microsoft.com/.default"
team_id: Option<String>
channel_id: Option<String>
chat_id: Option<String>
user_id: Option<String>
```

Runtime validation should require:

- `tenant_id`
- `client_id`
- one of `refresh_token`, `MS_GRAPH_REFRESH_TOKEN`, `MS_GRAPH_ACCESS_TOKEN`

Do not require `public_base_url` for egress-only sends. Keep it optional for later Graph subscription ingress.

## Secret Lookup

Use:

- `MS_GRAPH_TENANT_ID`
- `MS_GRAPH_CLIENT_ID`
- `MS_GRAPH_REFRESH_TOKEN`
- `MS_GRAPH_CLIENT_SECRET` optional
- `MS_GRAPH_ACCESS_TOKEN` optional test/dev override

Do not require:

- `MS_BOT_APP_ID`
- `MS_BOT_APP_PASSWORD`

## Auth Design

Replace outbound Bot Framework token acquisition with `acquire_graph_token(cfg)`:

1. Use `MS_GRAPH_ACCESS_TOKEN` or config access-token override if present.
2. Otherwise use refresh-token grant:
   - `POST {auth_base_url}/{tenant_id}/oauth2/v2.0/token`
   - `grant_type=refresh_token`
   - `client_id={client_id}`
   - `refresh_token={refresh_token}`
   - `scope={token_scope}`
   - include `client_secret` only when configured or present in secrets.
3. Parse `access_token`.

Do not call the Bot Framework token endpoint in the Graph egress path.

## Egress Design

Preserve the existing `render_plan -> encode -> send_payload` shape where practical, but rewrite actual sending to Microsoft Graph `chatMessage` endpoints.

Supported destinations:

- Channel:
  - `kind = "channel"`
  - destination id format `team_id:channel_id`
  - or explicit `team_id` and `channel_id`
  - URL: `{graph_base_url}/teams/{team_id}/channels/{channel_id}/messages`
- Channel reply:
  - `kind = "reply"` or `reply_to_id` / `thread_id`
  - requires `team_id` and `channel_id`
  - URL: `{graph_base_url}/teams/{team_id}/channels/{channel_id}/messages/{message_id}/replies`
- Chat:
  - `kind = "chat"`
  - destination id or explicit `chat_id`
  - URL: `{graph_base_url}/chats/{chat_id}/messages`

Graph body:

```json
{
  "body": {
    "contentType": "text",
    "content": "..."
  }
}
```

Allow `html` content type when the input explicitly asks for it or render fallback produces safe HTML. For Adaptive Cards, do not emit Bot Framework attachments. Convert to safe text or HTML fallback for this PR.

## Helper Functions

Add focused helpers, preferably in `ops/send.rs` until reuse is real:

- `resolve_destination(parsed, cfg, envelope)`
- `graph_message_body(text, content_type, optional_html)`
- `graph_post(url, token, body)`
- `classify_graph_error(status, body)`

## Error Mapping

Map Graph failures clearly:

- `401`: expired or invalid Graph token
- `403`: missing Graph permission or admin consent
- `404`: invalid team, channel, chat, or message id
- `429`: retryable if the response shape can carry retryable status

`send_payload` should return retryable for `429` if supported by `provider_common::helpers::send_payload_error`.

## Tests

Update or add unit tests for:

- config validates `tenant_id`, `client_id`, and refresh-token/access-token availability.
- Bot config fields are not required in the Graph path.
- destination `team_id:channel_id` parsing.
- explicit `team_id` / `channel_id`.
- chat destination.
- reply URL building.
- refresh-token form omits `client_secret` unless configured.
- `send_payload` rejects non-Teams provider types.
- mocked HTTP, if available:
  - channel send posts to `/teams/{team}/channels/{channel}/messages`
  - reply posts to `/teams/{team}/channels/{channel}/messages/{message}/replies`
  - chat send posts to `/chats/{chat}/messages`

## Acceptance Criteria

- No default egress path requires Bot Framework secrets or IDs.
- Teams egress uses Microsoft Graph endpoints.
- Provider result reports `messaging.teams.graph`.
- `scripts/build_providers.sh teams` succeeds.
- `cargo test -p messaging-provider-teams` succeeds.

## Out Of Scope

- Full setup UX rewrite.
- Pack manifest/schema/docs overhaul.
- Graph change-notification normalization.
- Teams app catalog install flow.
