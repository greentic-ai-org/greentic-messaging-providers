# 3AIgent GUI provider + skin authoring contract — design

Date: 2026-08-26
Status: approved for planning

## Summary

Two deliverables, independent enough to land separately:

**A.** A new provider pack `messaging-3aigent-gui` exposing provider type
`messaging.3aigent-gui` — the WebChat GUI experience with the `3aigent`
(3Point) skin and OAuth login on by default. It reuses the existing
`messaging-provider-webchat-gui` implementation via source inclusion; no
backend logic is duplicated.

**B.** A skin authoring contract for `messaging-webchat-gui`: a `_template`
skin, a JSON Schema, a CI validator, a generator script, and a change that
makes skins location-independent. Shipping skins as separate `.gtpack`
artifacts is explicitly deferred; this design removes the obstacle that would
have blocked it.

## Background

### What a skin actually is

A skin is a directory of static files plus a manifest. The SPA fetches
`${guiBase}skins/${tenant}/skin.json`; a fetch interceptor in
`packs/messaging-webchat-gui/assets/webchat-gui/runtime-bootstrap.js:1408`
rewrites the folder name using the tenant config's `skin` field and falls back
to `skins/default/` on a miss. Every other skin asset (styleOptions,
hostconfig, hooks, fullpage HTML/CSS, images) is referenced by path from inside
`skin.json`. Nothing about a skin is compiled.

### webchat-gui is already a thin wrapper

`components/messaging-provider-webchat-gui/src/lib.rs:23-29` source-includes
the backend from the plain webchat provider:

```rust
#[path = "../../messaging-provider-webchat/src/describe.rs"]
mod describe;
#[path = "../../messaging-provider-webchat/src/ops/mod.rs"]
mod ops;
```

It owns only `config.rs`, `directline/host.rs`, and its own `lib.rs`. The
repo's own audit (`docs/audit/webchat_gui_provider_types.md`) recommends
exactly this shape: *"Prefer two thin wrappers over duplicated logic."*
Deliverable A extends the same pattern one level further.

### One pack, one provider type

The audit also establishes that operator inventory is effectively
single-primary-provider-per-pack. A second provider type therefore requires a
second pack, not a second entry in `greentic.provider-extension.v1`.

## Deliverable A — `messaging-3aigent-gui`

### A.1 Component: parameterize, then wrap

The provider type and the two changed defaults are the only differences
between webchat-gui and 3aigent-gui. They become crate-level constants that
the shared modules read.

**Refactor `messaging-provider-webchat-gui`** (behavior-preserving):

- Move the body of `src/lib.rs` (844 lines) into a new `src/gui_core.rs`.
- `src/lib.rs` keeps only the `bindings` module, the crate-level constants,
  and the module declarations.
- `src/config.rs:17` — `default_skin()` returns `crate::DEFAULT_SKIN.to_string()`.
- `src/config.rs` — add `default_oauth_enabled() -> Option<bool>` returning
  `Some(crate::DEFAULT_OAUTH_ENABLED)`, and apply
  `#[serde(default = "default_oauth_enabled")]` to `oauth_enabled` on both
  `ProviderConfig` and `ProviderConfigOut`.

Constants in `messaging-provider-webchat-gui/src/lib.rs`:

```rust
pub(crate) const PROVIDER_ID: &str = "messaging-provider-webchat-gui";
pub(crate) const PROVIDER_TYPE: &str = "messaging.webchat-gui";
pub(crate) const WORLD_ID: &str = "component-v0-v6-v0";
pub(crate) const DEFAULT_SKIN: &str = "default";
pub(crate) const DEFAULT_OAUTH_ENABLED: bool = false;
```

**New crate `components/messaging-provider-3aigent-gui`** (~60 lines of
`lib.rs` plus a copied `wit/` tree):

```rust
mod bindings {
    wit_bindgen::generate!({
        path: "wit/messaging-provider-3aigent-gui",
        world: "component-v0-v6-v0",
        generate_all
    });
}

pub(crate) const PROVIDER_ID: &str = "messaging-provider-3aigent-gui";
pub(crate) const PROVIDER_TYPE: &str = "messaging.3aigent-gui";
pub(crate) const WORLD_ID: &str = "component-v0-v6-v0";
pub(crate) const DEFAULT_SKIN: &str = "3aigent";
pub(crate) const DEFAULT_OAUTH_ENABLED: bool = true;

#[path = "../../messaging-provider-webchat-gui/src/config.rs"]
pub(crate) mod config;
#[path = "../../messaging-provider-webchat-gui/src/directline/mod.rs"]
pub(crate) mod directline;
#[path = "../../messaging-provider-webchat/src/describe.rs"]
#[allow(dead_code)]
mod describe;
#[path = "../../messaging-provider-webchat/src/ops/mod.rs"]
mod ops;
#[path = "../../messaging-provider-webchat-gui/src/gui_core.rs"]
mod gui_core;
```

`Cargo.toml` mirrors webchat-gui's with the new package name and
`package.metadata.component.package = "greentic:messaging-provider-3aigent-gui-core"`.
The `wit/` tree is copied to `wit/messaging-provider-3aigent-gui/` with the
same `deps/` set; `tools/sync_wit_deps_from_greentic_interfaces.sh` keeps it
current thereafter.

The workspace picks the crate up automatically (`members = ["components/*"]`).

### A.2 Pack: reuse the existing public paths

`packs/messaging-3aigent-gui/pack.yaml` declares
`provider_type: messaging.3aigent-gui` with the same op list as webchat-gui,
and reuses **identical** public paths:

- static route `/v1/web/webchat/{tenant}`
- HTTP routes `/v1/messaging/webchat/{tenant}/...`

This is deliberate. Two hardcoded assumptions make a new namespace expensive
for no gain:

- `greentic-start/src/static_handler.rs:313` strips the literal prefix
  `/v1/web/webchat/` to serve the locale picker's i18n `_manifest.json`.
- `runtime-bootstrap.js:141` builds the backend base from the literal
  `/v1/messaging/webchat/`.

Reusing the paths also produces the correct failure mode. `validate_plan`
(`greentic-start/src/static_routes.rs:661`) records a blocking failure when two
routes claim the same `public_path`, so a bundle containing both webchat-gui
and 3aigent-gui refuses to activate. They are alternatives, never additions —
the same relationship webchat-gui has to webchat — so the validator enforcing
mutual exclusivity is the desired behavior, obtained for free.

### A.3 Assets: mirrored, with one pack-owned exception

`tools/prepare_pack_assets.sh` mirrors
`packs/messaging-webchat-gui/assets/webchat-gui/` into
`packs/messaging-3aigent-gui/assets/webchat-gui/`, following the
`messaging-webchat-ui` precedent so there remains exactly one committed copy of
the SPA and the packs cannot drift.

One file is excluded from the mirror and owned by the 3aigent pack:
`config/tenants/default.json`. The rsync gains
`--exclude config/tenants/default.json`.

`packs/messaging-3aigent-gui/assets/setup.yaml` is the pack's own legacy QA
copy, identical to webchat-gui's except `skin` defaults to `3aigent` and
`oauth_enabled` defaults to `true`.

### A.4 Tenant config

Two behaviors in `greentic-start/src/static_handler.rs` make the committed
`default.json` load-bearing, and getting either wrong fails silently.

**Tenant `default` is never synthesized.** `try_serve_synthesized_tenant_config`
returns `None` when `tenant_id == "default"` and lets the pack file serve
byte-for-byte (test: `synthesize_skips_default_tenant_so_pack_file_serves_directly`).
The wizard's `skin` answer therefore never reaches the default tenant, so the
committed file must carry the skin itself.

**Only pre-declared providers can be enabled.**
`apply_envelope_tenant_overrides` (`static_handler.rs:440`) maps
`oauth_enable_<id>` from the setup envelope onto `enabled` for each provider
**already present** in the template's `auth.providers` array. webchat-gui ships
`"auth": { "providers": [] }`, so no wizard answer can ever switch a provider
on. The 3aigent pack must pre-declare each provider with `enabled: false` and
an `id` that matches the `oauth_enable_<id>` answer key exactly.

`packs/messaging-3aigent-gui/assets/webchat-gui/config/tenants/default.json`:

```json
{
  "tenant_id": "default",
  "skin": "3aigent",
  "legacy_skin": "3aigent",
  "branding": {
    "company_name": "3AIgent",
    "tagline": "3Point Training Assistant",
    "logo": "/skins/3aigent/assets/3point-jewel-round.png"
  },
  "webchat": {
    "directline": { "token_url": "/v1/messaging/webchat/default/token" },
    "locale": "en-US"
  },
  "auth": {
    "providers": [
      { "id": "google",    "label": "Google",      "type": "oidc", "enabled": false },
      { "id": "microsoft", "label": "Microsoft",   "type": "oidc", "enabled": false },
      { "id": "github",    "label": "GitHub",      "type": "oidc", "enabled": false },
      { "id": "custom",    "label": "Company SSO", "type": "oidc", "enabled": false }
    ]
  }
}
```

The `authorizationUrl` / `scope` / `responseType` fields follow the shape
already used in `config/tenants/greentic.json`; `clientId` and `redirectUri`
stay absent until the wizard supplies them.

Branding uses **literal strings**, not `{"i18n": ...}` references. The i18n
bundles live in `assets/webchat-gui/i18n/`, which
`tools/import_webchat_gui_assets.sh:18` rsyncs with `--delete` from the
upstream `greentic-webchat` repo — a `product.3aigent.*` key added here would
be erased on the next import. The SPA already accepts either form; its
branding resolver reads
`typeof v == "string" ? v : v?.i18n ? translate(v.i18n) : skin.brand.name`.
Adding real translation keys would mean a change upstream in
`greentic-webchat`, which is out of scope.

In practice most 3AIgent branding already arrives from the skin: the login and
header components prefer `skin.brand.name` over the tenant config, and
`skins/3aigent/skin.json` already sets it to `3AIgent`.

### A.5 SSO semantics

`oauth_enabled` defaults to `true`. Individual providers stay disabled until an
administrator supplies credentials through the QA wizard, which composes
`oauth_providers` via `compose_oauth_providers` (webchat-gui `lib.rs:630`).

A 3aigent GUI configured with `oauth_enabled: true` and no provider
credentials presents a login screen with no buttons. This is a real
misconfiguration and belongs in the implementation plan as an explicit task:
either `validate_config_out` rejects it, or the SPA renders an operator-facing
message. The decision is deferred to planning, but it must not be dropped.

### A.6 Release registration

- `ci/provider-matrix.json` — a `3aigent-gui` entry mirroring `webchat-gui`
  (pack, ghcr target, components, manifests, paths, e2e block).
- `specs/providers/3aigent-gui.yaml` — mirroring `specs/providers/webchat-gui.yaml`.
- `tools/build_components/messaging-provider-3aigent-gui.sh` and the new
  package name appended to `DEFAULT_PACKAGES` in `tools/build_components.sh`.
- `tools/prepare_pack_assets.sh` — the mirror step described in A.3.
- `tools/sync_packs.sh` then `python3 tools/update_packs_lock.py`.
- `packs/messaging-3aigent-gui/`: `pack.yaml`, `pack.manifest.json`,
  `component.manifest.json`, `schemas/messaging/3aigent-gui/{config,public.config}.schema.json`,
  `fixtures/`, `secret-requirements.json`, `README.md`.
- `./tools/regenerate_registry_fixtures.sh`.

`crates/greentic-messaging-renderer/src/capabilities.rs` needs **no** change:
the ops are source-included from the webchat provider and resolve capabilities
under the name `webchat`, which is why webchat-gui has no entry of its own
today either.

## Deliverable B — skin authoring contract

### B.1 Why not a skin `.gtpack`, yet

A skin pack was investigated and is technically reachable, but not worth its
cost today.

Route matching would work: `ActiveRouteTable::from_plan`
(`greentic-start/src/static_routes.rs:157`) sorts descending by
`route_segments.len()` and `match_first` takes the first hit, so longest-prefix
wins.

Route *validation* rejects it. `validate_plan`
(`greentic-start/src/static_routes.rs:661`) flags any two routes in a prefix
relationship (`paths_overlap`, `static_routes.rs:692`):

```rust
fn paths_overlap(left: &str, right: &str) -> bool {
    path_has_prefix(left, right) || path_has_prefix(right, left)
}
```

`/v1/web/webchat/{tenant}/skins/3point` strips to `/skins/3point`, which starts
with `/`, so it overlaps the GUI route and becomes a blocking failure — the
whole bundle refuses to activate, not just the skin. The test
`revision_discovery_rejects_overlapping_routes` (`static_routes.rs:1462`) locks
this in.

A sibling namespace such as `/v1/web/webchat-skins/{tenant}` passes validation
and is a viable design. It was deferred because the benefit it promises does
not materialize: `discover_from_bundle` collects static routes from packs
**inside the bundle** (`collect_runtime_pack_paths(bundle_root)`), so
installing a skin pack still means rebuilding and redeploying the bundle —
exactly what a bundle overlay costs. What a skin pack adds over an overlay is a
versioned OCI artifact; what it costs is a publish pipeline, a provider-matrix
entry, version syncing and CI, per skin. With one skin in existence — already
shipping inside the 3aigent GUI pack — that trade is not yet worth making.

### B.2 What is built now

Every item below is required by all three delivery mechanisms (in-pack,
bundle overlay, future skin pack), so none of it is wasted work.

**A `_template` skin.** `packs/messaging-webchat-gui/assets/webchat-gui/skins/_template/`
with placeholder `skin.json`, `fullpage/index.html`, `fullpage/page.css`,
`webchat/styleOptions.json`, `webchat/hostconfig.json`, `webchat/hooks.js`, and
`assets/`. Self-contained, matching how `default/` and `3aigent/` are
structured.

**A schema and a validator.** `schemas/webchat/skin.schema.json` describing
`skin.json`, plus a CI step validating every `skins/*/skin.json` in the pack.
Today nothing validates a skin; a broken one surfaces in the browser as a
silent fall back to `default`.

**Location-independent skins.** Skin asset paths become relative
(`./webchat/hooks.js`) instead of root-absolute
(`/skins/3aigent/webchat/hooks.js`). The fetch interceptor in
`runtime-bootstrap.js` already rebuilds the `skin.json` response body, so it
absolutizes each relative path against the skin's own directory — roughly five
lines, at the point where `directLine.tokenUrl` is already being rewritten.

This is the change that unblocks a future skin pack: a skin whose paths are
relative can be served from the GUI pack, from a bundle overlay, or from a
sibling-namespace route with no further modification. `runtime-bootstrap.js` is
owned by this repo — `tools/import_webchat_gui_assets.sh` rsyncs only
`assets/`, `config/`, `i18n/`, `js/` and `skins/`, never root-level files — so
the change survives upstream imports.

**A generator.** `tools/new_skin.sh <name>` copies `_template/`, substitutes the
skin name through `skin.json` and the fullpage HTML, and reports the follow-up
edits.

**An import allowlist.** `tools/import_webchat_gui_assets.sh:27` currently
prunes every skin directory except a hardcoded `default` and `3aigent`. Any
new pack-local skin is silently deleted on the next upstream import. The prune
becomes allowlist-driven, sourced from the directories already committed under
`skins/`.

**Bundle overlay documented.** `greentic-start` resolves assets from an
extracted bundle overlay before the pack
(`static_handler.rs`, `revision_bundle_root`, test
`serve_static_route_prefers_bundle_overlay_assets`), gated on the
`greentic.cap.bundle_assets.read.v1` capability that
`check_bundle_assets_capability` warns about when it is missing. Dropping a
skin at `<bundle>/assets/webchat-gui/skins/<name>/` is a supported per-tenant
delivery path today and should be written down as one.

## Testing

**The refactor.** webchat-gui's existing tests live in the `mod tests` block
inside `lib.rs` and move to `gui_core.rs` with it. They must pass unchanged —
that is the evidence the extraction preserved behavior. The places that
hardcode `"default"` as the expected skin — the assertions at `config.rs:244`
and `lib.rs:747`, and the fixture literals at `lib.rs:785` and `lib.rs:808` —
change to derive from `crate::DEFAULT_SKIN`, so the same tests run in both
crates and verify the parameterization instead of a literal.

**The new provider.** `standard_provider_tests!` for
`messaging-provider-3aigent-gui`, a fresh schema hash, and pack fixtures
(`egress.request.json`, `egress.expected.summary.json`,
`ingress.request.json`, `ingress.expected.message.json`,
`setup.input.json`, `setup.expected.plan.json`,
`requirements.expected.json`). Add targeted tests asserting
`PROVIDER_TYPE == "messaging.3aigent-gui"`, `skin` defaulting to `3aigent`,
and `oauth_enabled` defaulting to `true` when the input config omits them.

**Tenant config.** A test asserting the committed `default.json` carries
`skin: "3aigent"` and declares all four provider ids, since both properties are
silently load-bearing (A.4).

**Skin contract.** Schema-validation tests covering a well-formed skin, a skin
missing a required field, and a skin with an unresolvable asset path.

**Full gate.** `./ci/local_check.sh` before the PR.

## Out of scope

- Publishing skins as standalone `.gtpack` artifacts (B.1).
- Any change to `greentic-start` or `greentic-operator`.
- Migrating existing tenants from `messaging-webchat-gui` to
  `messaging-3aigent-gui`.
- New skins beyond the existing `3aigent` skin and the `_template`.

## Open decision for planning

A.5: how a 3aigent GUI with `oauth_enabled: true` and zero configured
providers should behave — config validation failure, or an operator-facing
message in the SPA.
