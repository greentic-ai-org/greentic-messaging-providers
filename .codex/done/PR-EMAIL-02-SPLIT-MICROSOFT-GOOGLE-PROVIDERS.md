# PR: Split Generic Email Into Microsoft and Google Email Providers

## Problem

The current `messaging-email` provider mixes incompatible concepts:

- Pack/setup copy says Microsoft Graph.
- Component QA setup asks for SMTP fields.
- `apply-answers` ignores Microsoft Graph setup answers.
- `send_payload` is Graph-backed, but `send`/`reply` return synthetic SMTP-style IDs.
- Graph subscription and webhook code exists, but the pack does not declare a startup subscription contract.
- Docs still describe SMTP as the primary path.

Trying to make one generic email provider support Microsoft Graph, Gmail, and SMTP will keep producing ambiguous contracts. Microsoft Graph and Gmail have different OAuth scopes, webhook/subscription lifecycles, message identifiers, attachment APIs, threading models, rate limits, and ingress validation behavior.

## Decision

Do not redesign `messaging-email` as one email provider for all vendors.

Create vendor-specific email providers:

- `messaging-microsoft-email`
  - Provider type: `messaging.email.microsoft_graph`
  - Owns Microsoft Graph OAuth, send, reply, subscriptions, webhook ingress, and setup contracts.

- `messaging-google-email`
  - Provider type: `messaging.email.gmail`
  - Owns Gmail OAuth, send, reply, watch/Pub/Sub setup, webhook ingress, and setup contracts.

Keep `messaging-email` only as a legacy/generic SMTP provider if SMTP remains needed. It should not advertise Microsoft Graph behavior.

## Microsoft Provider Contract

`messaging-microsoft-email` should be built from an answer document per `PR-EMAIL-01`.

Setup answers:

- `from_address`
- `graph_tenant_id`
- `ms_graph_client_id`
- `ms_graph_refresh_token`
- `ms_graph_client_secret`
- `public_base_url` when ingress/subscriptions are enabled
- optional Graph endpoint overrides such as `graph_base_url`, `graph_authority`, `graph_token_endpoint`, `graph_scope`

Runtime config:

- non-secret config only: `from_address`, `public_base_url`, Graph endpoint overrides, optional default recipient
- no client secret or refresh token in config

Runtime secrets:

- `FROM_ADDRESS`
- `GRAPH_TENANT_ID`
- `MS_GRAPH_CLIENT_ID`
- `MS_GRAPH_REFRESH_TOKEN`
- `MS_GRAPH_CLIENT_SECRET`

Operations:

- `send`
- `reply`
- `send_payload`
- `render_plan`
- `encode`
- `ingest_http`
- `subscription_ensure`
- `subscription_renew`
- `subscription_delete`
- `qa-spec`
- `apply-answers`
- `i18n-keys`

Subscription metadata:

- If startup is expected to manage subscriptions generically, the pack must declare a provider-owned `messaging.subscriptions.v1` contract.
- Component config allowlist must include only auth/client fields accepted by the subscription component.
- Desired state templates may use mailbox/user setup answers, but those fields must not be passed as unknown top-level component config.

## Google Provider Contract

`messaging-google-email` should be a separate PR after Microsoft is corrected.

Setup answers should be Gmail-specific:

- sender mailbox
- OAuth client id/secret
- refresh token or installed OAuth setup output
- Google Cloud project id
- Pub/Sub topic/subscription or provider-owned desired-state metadata
- public webhook URL when push notifications are enabled

Runtime secrets should use Gmail-owned names, not Microsoft or generic SMTP names.

The provider must model Gmail watch expiration and Pub/Sub verification explicitly. It should not reuse Microsoft Graph subscription metadata.

## Legacy SMTP Provider

If `messaging-email` remains:

- Rename docs and setup copy to SMTP/generic email.
- Keep SMTP fields: `host`, `port`, `username`, `password`, `from_address`, `tls_mode`.
- Do not include Graph setup questions.
- Do not advertise Graph subscriptions or Graph webhook ingress.
- Do not route Graph code from generic SMTP setup.

## Implementation PR Sequence

1. Add answer-driven provider build support from `PR-EMAIL-01`.
2. Create `messaging-microsoft-email` from the existing Graph code path.
3. Strip Microsoft Graph claims from legacy `messaging-email`, or mark it deprecated if SMTP is not needed.
4. Add Microsoft subscription metadata only when the component export and startup contract are aligned.
5. Add `messaging-google-email` in a later PR with Gmail-specific setup, OAuth, send, and ingress semantics.

## Tests

- Microsoft setup answers produce Microsoft runtime config and `secrets_patch`.
- Microsoft config validation does not require SMTP fields.
- Microsoft send/reply use the Graph send path and do not return synthetic SMTP IDs.
- Microsoft pack metadata advertises only operations actually supported by the component.
- Microsoft subscription metadata separates component config from desired-state fields.
- Legacy `messaging-email` setup does not ask for Microsoft Graph fields.
- Existing configs without new vendor-specific labels fail with a clear migration/deprecation message or continue only through the legacy SMTP path.

## Non-Goals

- No live Microsoft or Google API e2e.
- No host-specific email branching.
- No Gmail implementation in the Microsoft provider PR.

## Status

Done for the Microsoft split.

Implemented `messaging-microsoft-email` as a Microsoft Graph-specific pack using the existing Graph-capable email component, moved Microsoft setup and secret ownership into that pack, and narrowed `messaging-email` back to legacy SMTP setup/docs. Gmail remains a separate provider implementation because this PR explicitly scoped it as a later provider with different OAuth and Pub/Sub semantics.
