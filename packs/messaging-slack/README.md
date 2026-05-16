# Messaging Slack Pack

Slack messaging provider — Bot API with Events API ingress.

## Pack ID
- `messaging-slack`

## Providers
- `messaging.slack.api` (capabilities: messaging; ops: send, reply, qa-spec, apply-answers, i18n-keys)

## Components
- `messaging-provider-slack` — core provider WASM (secrets-store + http-client)
- `messaging-ingress-slack` — Events API webhook ingress WASM

## Secrets
- `SLACK_BOT_TOKEN` — Slack bot token (xoxb-...)
- `SLACK_SIGNING_SECRET` — Slack signing secret (optional, webhook verification)
- `SLACK_APP_ID` — Slack app ID for manifest webhook registration
- `SLACK_CONFIGURATION_ACCESS_TOKEN` — short-lived Slack configuration access token
- `SLACK_CONFIGURATION_REFRESH_TOKEN` — refresh token used to rotate expired configuration access tokens
- `SLACK_CONFIGURATION_TOKEN` — legacy configuration token key accepted for existing installs

## Flows
- `setup_default` — configures provider via `messaging.configure` op
- `requirements` — validates provider configuration

## Setup
Inputs:
- Config required: public_base_url
- Config optional: default_channel, team_id
- Secrets required: SLACK_BOT_TOKEN
- Secrets optional: SLACK_SIGNING_SECRET, SLACK_APP_ID, SLACK_CONFIGURATION_ACCESS_TOKEN, SLACK_CONFIGURATION_REFRESH_TOKEN

Webhooks:
- public_base_url + /webhooks/slack

## Extensions
- `greentic.ext.capabilities.v1` — capability offer `messaging-slack-v1`
- `greentic.provider-extension.v1` — provider type, ops, runtime binding
- `messaging.oauth.v1` — OAuth 2.0 configuration for Slack
- `messaging.provider_ingress.v1` — webhook ingress (supports_webhook_validation: true)
