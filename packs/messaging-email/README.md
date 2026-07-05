# Messaging Email Pack

Legacy SMTP email messaging provider. Also carries the optional Gmail
inbound backend (`kind: gmail`) for the shared `messaging-provider-email`
component — off by default; Graph/SMTP tenants are unaffected.

## Pack ID
- `messaging-email`

## Providers
- `messaging.email.smtp` (capabilities: messaging; ops: send, reply, ingest_http, qa-spec, apply-answers, i18n-keys)

## Components
- `messaging-provider-email` — core provider WASM (secrets-store + http-client)

## Secrets
- `EMAIL_PASSWORD` — SMTP password
- `GMAIL_CLIENT_SECRET` — Gmail OAuth client secret (kind: gmail only)
- `GMAIL_REFRESH_TOKEN` — Gmail OAuth refresh token (kind: gmail only)
- `GMAIL_PUBSUB_VERIFICATION_TOKEN` — shared token verifying inbound Gmail Pub/Sub pushes (kind: gmail only)

## Flows
- `setup_default` — configures provider via `messaging.configure` op
- `requirements` — validates provider configuration

## Setup
Inputs:
- Config required: host, port, username, from_address, tls_mode
- Secrets required: EMAIL_PASSWORD
- Gmail backend (optional): set `kind: gmail` + `gmail_user`/`gmail_client_id` config,
  plus GMAIL_CLIENT_SECRET/GMAIL_REFRESH_TOKEN/GMAIL_PUBSUB_VERIFICATION_TOKEN secrets

## Extensions
- `greentic.ext.capabilities.v1` — capability offer `messaging-email-v1`
- `greentic.provider-extension.v1` — provider type, ops, setup_contract (Gmail secrets), runtime binding
