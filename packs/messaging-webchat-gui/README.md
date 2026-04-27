# Messaging WebChat GUI Pack

Hosted WebChat GUI pack with packaged `greentic-webchat` assets, depending on `messaging-webchat` for the Direct Line backend.

## Pack ID
- `messaging-webchat-gui`

## Providers
- `messaging.webchat-gui`

## Components
- `messaging-provider-webchat-gui` — GUI setup/config runtime for the hosted shell

## Hosted routes
- GUI base: `/v1/web/webchat/{tenant}`

## Backend dependency
- `messaging-webchat` owns `/v1/messaging/webchat/{tenant}/...`
- `messaging-webchat` owns `jwt_signing_key` and token issuance

## Assets
- `assets/webchat-gui/...` — packaged static frontend
