# WebChat GUI Provider

## What It Does

WebChat GUI packages the browser user interface for Greentic WebChat. It can run as a full hosted page or as an embeddable Web Component inside a customer's existing website.

## Features

- Serves the hosted WebChat GUI page.
- Serves `assets/webchat-gui/embed.js` for framework-independent embedding.
- Supports `presentation_mode=standalone`.
- Supports `presentation_mode=embed_webcomponent`.
- Keeps `skin` as the visual theme folder, such as `default` or `3aigent`.
- Hides top navigation links in embedded mode because the host website owns navigation.
- Uses the same Direct Line backend concepts as WebChat.

## Setup Inputs

Common setup values include:

- Public base URL.
- Route/channel name.
- Delivery mode.
- JWT signing key secret.
- Presentation mode: `standalone` or `embed_webcomponent`.
- Skin: visual theme folder.
- Navigation links: standalone mode only.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `jwt_signing_key` | Yes | Signs and verifies Direct Line tokens. |

## Message Features

- Outbound: through the WebChat backend.
- Inbound: browser activities through Direct Line-compatible endpoints.
- Adaptive Cards: native WebChat support.
- Embedded UI: `<greentic-webchat>` custom element.
- Hosted UI: `/v1/web/webchat/{tenant}`.
- Embed script: `/v1/web/webchat/{tenant}/embed.js`.

## Owned Files

- `components/messaging-provider-webchat-gui/`
- `packs/messaging-webchat-gui/`
- `packs/messaging-webchat-gui/assets/webchat-gui/`
- `tools/import_webchat_gui_assets.sh`
- `docs/guides/webchat-gui-embed-webcomponent.md`

## Focused Checks

```bash
cargo test -p messaging-provider-webchat-gui
cargo test -p greentic-messaging-provider-common webchat_gui_config_schema_declares_presentation_mode
PACK_FILTER=messaging-webchat-gui ./ci/steps/11_build_packs.sh
```

## Agent Notes

Do not overload `skin` with behavior. `skin` is visual theme only. Use `presentation_mode` for standalone versus embedded behavior.

See [WebChat GUI Web Component guide](../guides/webchat-gui-embed-webcomponent.md).

