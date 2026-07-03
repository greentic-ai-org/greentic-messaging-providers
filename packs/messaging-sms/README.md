# Messaging SMS Pack

Twilio SMS messaging provider — conversational inbound + outbound.

## Pack ID
- `messaging-sms`

## Providers
- `messaging.sms.twilio` (capabilities: messaging; ops: ingest_http, render_plan, encode, send_payload, setup_webhook, qa-spec, apply-answers, i18n-keys)

## Components
- `messaging-provider-sms` — core provider WASM (secrets-store + http-client); handles both inbound webhook ingest and outbound egress

## Secrets
- `TWILIO_ACCOUNT_SID` — Twilio Account SID used to authenticate REST API calls
- `TWILIO_AUTH_TOKEN` — Twilio Auth Token used for REST API calls and inbound webhook signature validation
- `TWILIO_FROM_NUMBER` — Twilio phone number (E.164) used as the sender for outbound SMS

## Setup
Inputs:
- Secrets required: TWILIO_ACCOUNT_SID, TWILIO_AUTH_TOKEN, TWILIO_FROM_NUMBER

Webhooks:
- `/v1/messaging/ingress/messaging-sms/{tenant}/{team}` — Twilio inbound SMS webhook (POST; GET accepted as a probe)

## Extensions
- `greentic.ext.capabilities.v1` — capability offer `messaging-sms-v1`
- `greentic.http-routes.v1` — default `ingest_http` route, domain `messaging`
- `greentic.provider-extension.v1` — provider type, ops, setup contract, runtime binding
