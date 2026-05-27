# Messaging Teams Pack

Microsoft Teams messaging provider - Microsoft Graph egress and Graph change notification ingress.

## Pack ID

- `messaging-teams`

## Providers

- `messaging.teams.graph` (capabilities: messaging; ops: send, reply, ingest_http, render_plan, encode, send_payload, qa-spec, apply-answers, i18n-keys)

## Components

- `messaging-provider-teams` - core Graph provider WASM (secrets-store + http-client)
- `messaging-ingress-teams` - Graph notification ingress and subscription lifecycle WASM

## Secrets

- `MS_GRAPH_TENANT_ID` - Microsoft Entra tenant ID
- `MS_GRAPH_CLIENT_ID` - Microsoft Entra app client ID
- `MS_GRAPH_REFRESH_TOKEN` - delegated OAuth refresh token
- `MS_GRAPH_ACCESS_TOKEN` - optional test-only access token override

## Setup

Default setup is Graph-first:

1. Start Microsoft device-code login with the `organizations` endpoint.
2. Show `https://microsoft.com/devicelogin` and the user code.
3. Save tenant/client IDs and refresh token after consent.
4. Choose a default Team/Channel or Chat.

Azure Bot Service, redirect OAuth callbacks, and client secrets are not required for the default path. `public_base_url` is only needed for Graph change-notification ingress.

## Extensions

- `greentic.ext.capabilities.v1` - capability offer `messaging-teams-v1`
- `greentic.provider-extension.v1` - provider type, ops, runtime binding
- `messaging.oauth.v1` - Microsoft OAuth metadata
- `messaging.oauth_device_code.v1` - Microsoft device-code setup metadata
- `messaging.provider_ingress.v1` - webhook ingress
- `messaging.subscriptions.v1` - Graph subscription sync
