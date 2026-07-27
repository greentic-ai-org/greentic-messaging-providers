# messaging-webchat-ui

The WebChat SPA **and nothing else** — no components, no provider, no flows.

`messaging-webchat-gui` ships the SPA *and* a webchat provider. That makes it an alternative to
`messaging-webchat`, never an addition: two webchat providers in one bundle both claim the same
endpoints. So a bundle already on the plain `messaging-webchat` pack has a working DirectLine
backend and no way to gain a browser tier without swapping providers — and swapping is not
equivalent, since the plain pack carries `diagnostics__*`, `verify_webhooks__*`,
`sync_subscriptions__*` and `ingress_default__*` flows the GUI pack does not.

This pack exists to close that gap. It contributes exactly one thing:

```yaml
greentic.static-routes.v1:
  routes:
  - id: webchat-gui
    public_path: /v1/web/webchat/{tenant}
    source_root: assets/webchat-gui
```

Declaring no components, no `messaging.provider_ingress.v1` and no flows is what makes it safe to
add to a bundle that already has a provider:

- **no provider** → no endpoint collision with the pack already serving DirectLine;
- **no flows** → `greentic-start`'s `extract_flows` reads the manifest's top-level `flows` array,
  which is empty here, so the bundle's flow index is untouched. It gains no phantom flow id, and
  its default flow is not tombstoned by a second pack claiming one.

The route it declares is stamped with the *consuming revision's* scope at discovery time, so the
SPA is served under the real bundle's URL and talks to that bundle's DirectLine endpoint.

## Assets are mirrored, not duplicated

`assets/` is generated and git-ignored. `tools/prepare_pack_assets.sh` mirrors
`packs/messaging-webchat-gui/assets/webchat-gui/` into this pack immediately after the upstream
import, so there is exactly one committed copy of the SPA in this repo and the two packs cannot
drift. Build this pack through the normal pack build (`scripts/build_providers.sh` /
`ci/steps/11_build_packs.sh`), never by hand from a fresh clone with an empty `assets/`.

A symlink would be simpler but does not work: `greentic-pack` does not walk a symlinked
`source_root`, and packages only the explicitly declared `assets:` entries — 5 files instead of the
full SPA.
