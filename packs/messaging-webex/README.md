# Messaging Webex Pack

Webex messaging provider — Bot API with Adaptive Cards.

## Pack ID
- `messaging-webex`

## Providers
- `messaging.webex.bot` (capabilities: messaging; ops: send, reply, setup_webhook, qa-spec, apply-answers, i18n-keys)

## Components
- `messaging-provider-webex` — core provider WASM (secrets-store + http-client)

## Secrets
- `WEBEX_BOT_TOKEN` — Webex bot access token
- `WEBEX_WEBHOOK_SECRET` — shared secret for Webex webhook signatures

## Flows
- `setup_default` — configures provider via `messaging.configure` op
- `requirements` — validates provider configuration

## Setup
Inputs:
- Config optional: public_base_url, default_room_id, default_to_person_email, api_base_url
- Secrets required at runtime: WEBEX_BOT_TOKEN, WEBEX_WEBHOOK_SECRET
- Setup asks only for WEBEX_BOT_TOKEN. The provider generates WEBEX_WEBHOOK_SECRET.
- The pack declares WEBEX_WEBHOOK_SECRET as a generated tenant-wide runtime
  secret so hosts can seed it generically for existing bundles.
- default_room_id/default_to_person_email are legacy proactive-send fallbacks. Direct bot conversations do not need them; replies should use the room/person metadata captured from the inbound Webex webhook.

Webhooks:
- `messages.created` with `mentionedPeople=me` or `roomId=<default_room_id>`
- `attachmentActions.created` for Adaptive Card submissions
- `messages.created` events authored by Webex bot accounts are acknowledged but ignored so outbound bot replies are not reprocessed as inbound user messages.

## Extensions
- `greentic.ext.capabilities.v1` — capability offer `messaging-webex-v1`
- `greentic.provider-extension.v1` — provider type, ops, runtime binding
