# PR 3: Add Graph Change-Notification Ingress

## Review Status

Reviewed against the current codebase on 2026-05-27 and adapted.

This PR is valid, but the original prompt needs adjustment for the current component boundary.

Current state:

- `components/messaging-ingress-teams` already exports `provider:common/ingress` and `provider:common/subscriptions`.
- It already implements `sync_subscriptions`, including Graph token acquisition, list/create/renew subscription calls, and state output.
- It does not implement separate exported `subscription_ensure`, `subscription_renew`, or `subscription_delete` operations.
- `handle_webhook` currently echoes the raw event as `{ "ok": true, "event": parsed }`; it does not normalize Graph notifications into `ChannelMessageEnvelope`.
- The current ingress WIT exports `ingress` and `subscriptions`, but not `ingress-validation`; handling Graph `validationToken` may need host/header conventions or a WIT/export change.
- `messaging.subscriptions.v1` currently declares resources `message` and `reaction`; reaction normalization is not implemented.

## Title

Add Graph change-notification ingress for Teams provider

## Goal

Receive Teams messages via Microsoft Graph change notifications without Azure Bot Service, Bot Framework JWT validation, Bot App ID, `serviceUrl`, or Bot Connector conversations.

## Adapted Scope

Primary implementation should be in:

- `components/messaging-ingress-teams/src/lib.rs`
- `components/messaging-ingress-teams/wit/...` only if validation-token support needs a new export
- `packs/messaging-teams/pack.yaml`
- `packs/messaging-teams/pack.manifest.json`
- subscription fixtures under `packs/messaging-teams/fixtures/`
- docs touched by PR 2 if needed

Do not reimplement subscription lifecycle in `components/messaging-provider-teams` unless the pack/WIT design is intentionally changed. Prefer extending the existing `sync_subscriptions` implementation.

## Subscription Lifecycle

Extend existing `sync_subscriptions` rather than replacing it.

Inputs should support desired subscription specs for:

- channel messages:
  - `/teams/{team_id}/channels/{channel_id}/messages`
- channel replies:
  - `/teams/{team_id}/channels/{channel_id}/messages/{message_id}/replies`
- chat messages:
  - `/chats/{chat_id}/messages`

Support:

- `team_id + channel_id`
- `chat_id`
- `change_type`, default `created`
- `notification_url`, preferably derived by the caller from `public_base_url` as `/v1/messaging/ingress/messaging-teams/{tenant}/{team}`
- `client_state`, generated/stored by caller or provided in desired state
- `include_resource_data` optional
- `lifecycle_notification_url` optional

Return/store:

- `subscription_id`
- `resource`
- `expirationDateTime`
- `client_state`

The current implementation already has pieces for list/create/renew and should be evolved rather than duplicated.

## Graph Validation Handshake

Microsoft Graph validates subscriptions with a `validationToken` query parameter and expects a plain text response.

Current `handle-webhook(headers_json, body_json)` does not receive a query parameter separately. Before implementing, inspect the host contract used by `greentic-messaging-tester` and `gtc start`:

- If query parameters are included in `headers_json`, parse `validationToken` there.
- If not, add/enable `provider:common/ingress-validation` export or adjust the host bridge in a separate small compatibility change.

Acceptance for this PR requires the validation-token path to be testable.

## Notification Normalization

`handle_webhook` should parse Graph notification payloads:

- Validate `clientState` when present and expected.
- Parse `value[]`.
- Extract:
  - `subscriptionId`
  - `changeType`
  - `resource`
  - `resourceData.id`
  - `tenantId`
- If full resource data is present, normalize from it.
- If resource data is absent, fetch the message from Graph using the resource path when token/config is available.

Normalize to the existing message envelope shape:

- source/provider: Teams
- `provider_message_id: teams:{message_id}`
- text from `body.content`, stripped/sanitized enough for existing tests
- metadata preserves:
  - `graph_resource`
  - `change_type`
  - `subscription_id`
  - `team_id`
  - `channel_id`
  - `chat_id`
  - `webUrl`
  - `replyToId`
  - original body content and content type where useful
- destination:
  - channel destination id `team_id:channel_id`
  - chat destination id `chat_id`

Keep Bot Framework activity normalization only as a legacy path if it is cheap and clearly separated. It must not be required by the Graph default path.

## Reaction Capability

Current `messaging.subscriptions.v1` advertises:

```text
message
reaction
```

Adaptation: narrow to `message` unless this PR implements and tests reaction normalization. Do not leave misleading manifest capabilities.

## Security

- Do not use Bot Framework JWT validation for Graph notifications.
- Validate Graph `clientState`.
- Do not log access or refresh tokens.
- Document that Graph webhooks require public HTTPS.

## Tests

Add unit tests for:

- validationToken response path.
- notification parsing.
- channel message normalization.
- channel reply normalization.
- chat message normalization.
- resource string building:
  - `/teams/{team_id}/channels/{channel_id}/messages`
  - `/teams/{team_id}/channels/{channel_id}/messages/{message_id}/replies`
  - `/chats/{chat_id}/messages`
- clientState mismatch rejection or warning behavior.

If mocked HTTP support is practical:

- POST `/subscriptions` body.
- PATCH `/subscriptions/{id}` renewal.
- DELETE `/subscriptions/{id}` only if delete support is added.
- fetch-message path when notification does not include resource data.

## Acceptance Criteria

- Teams ingress works via Graph notifications.
- Graph validation-token handshake works.
- No Azure Bot Service setup is needed for ingress.
- `messaging.subscriptions.v1` accurately declares only implemented resources.
- `cargo test -p messaging-ingress-teams` passes.
- Pack validation passes after metadata updates.

## Out Of Scope

- Graph egress rewrite. That belongs in PR 1.
- Setup UX/schema/doc overhaul except subscription docs. That belongs in PR 2.
- Interactive tester. That belongs in PR 4.
