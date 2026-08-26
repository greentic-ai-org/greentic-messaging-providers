# messaging-3aigent-gui

The WebChat GUI shipped as the 3AIgent product: the `3aigent` skin by default and
OAuth login enabled by default.

It is an **alternative** to `messaging-webchat-gui`, never an addition. Both packs
claim `/v1/web/webchat/{tenant}` and `/v1/messaging/webchat/{tenant}`, so a bundle
carrying both fails static-route validation — which is the intended behavior.

## What differs from messaging-webchat-gui

| | messaging-webchat-gui | messaging-3aigent-gui |
| --- | --- | --- |
| provider type | `messaging.webchat-gui` | `messaging.3aigent-gui` |
| default skin | `default` | `3aigent` |
| `oauth_enabled` default | `false` | `true` |

Nothing else. The component source-includes `gui_core.rs` from
`messaging-provider-webchat-gui`; the SPA assets are mirrored by
`tools/prepare_pack_assets.sh`.

## Assets

`assets/` is generated and mirrored from `packs/messaging-webchat-gui/assets/webchat-gui/`
by `tools/prepare_pack_assets.sh`, with one exception: `config/tenants/default.json`
is owned by this pack and is excluded from the mirror. Build through the normal pack
build (`tools/build_packs.sh` / `ci/steps/11_build_packs.sh`), never by hand from a
fresh clone.

## SSO

`oauth_enabled` defaults to `true`, but individual providers stay disabled until an
administrator supplies credentials in the setup wizard. Until then the SPA shows
"No sign-in provider configured" rather than an empty login screen.
