# Messaging Email Pack

Legacy SMTP email messaging provider.

## Pack ID
- `messaging-email`

## Providers
- `messaging.email.smtp` (capabilities: messaging; ops: send, reply, qa-spec, apply-answers, i18n-keys)

## Components
- `messaging-provider-email` — core provider WASM (secrets-store + http-client)

## Secrets
- `EMAIL_PASSWORD` — SMTP password

## Flows
- `setup_default` — configures provider via `messaging.configure` op
- `requirements` — validates provider configuration

## Setup
Inputs:
- Config required: host, port, username, from_address, tls_mode
- Secrets required: EMAIL_PASSWORD

## Extensions
- `greentic.ext.capabilities.v1` — capability offer `messaging-email-v1`
- `greentic.provider-extension.v1` — provider type, ops, runtime binding
