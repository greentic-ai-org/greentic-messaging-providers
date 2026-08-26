# Authoring a WebChat skin

A skin is a directory of static files plus a `skin.json` manifest. Nothing about
a skin is compiled. The SPA fetches `<gui-base>/skins/<name>/skin.json`;
`runtime-bootstrap.js` rewrites the folder name from the tenant config's `skin`
field and falls back to `skins/default/` if the fetch misses.

## Create one

```bash
./tools/new_skin.sh acme
```

This copies `skins/_template/` to `skins/acme/` and sets `tenant` to `acme`.
Then replace the artwork in `skins/acme/assets/`, set `brand.name` and
`brand.primary`, and adjust `webchat/styleOptions.json` and
`webchat/hostconfig.json`.

Validate before committing:

```bash
python3 tools/validate_skins.py
```

The validator runs as `ci/steps/05a_validate_skins.sh`, one of the steps
`./ci/local_check.sh` runs locally before a PR — no GitHub Actions workflow
invokes it, so it is not enforced on the PR itself. It checks the manifest
against `schemas/webchat/skin.schema.json`, that `tenant` matches the
directory name, and that every referenced file exists.

`tools/validate_skins.py` requires the `jsonschema` Python package, which no
requirements file or workflow installs: `pip install jsonschema`.

## Paths must be relative

Write `"./webchat/hooks.js"`, not `"/skins/acme/webchat/hooks.js"`. This is
enforced, not just a convention: `tools/validate_skins.py` rejects any
manifest path that starts with `/skins/`.

`runtime-bootstrap.js` absolutizes relative refs against the URL the manifest
was actually fetched from, so a skin with relative paths is servable from any
mount point. That absolutization also runs on the fallback-to-default path —
if a tenant has no skin folder of its own, the borrowed `default` manifest's
relative paths resolve against `default`'s own URL, not the missing tenant's,
which is what makes a tenant without a skin folder work at all. Root-absolute
paths still work (the fetch interceptor passes them through unchanged) but pin
the skin to one mount point; the validator rejects them for new skins.

## Delivery

Three ways to get a skin in front of users, in increasing order of ceremony:

**Inside the pack.** Commit it under
`packs/messaging-webchat-gui/assets/webchat-gui/skins/<name>/`. It ships to every
tenant on that pack and is validated in CI. This is the right choice for skins
that are part of the product.

**Bundle overlay.** Drop the skin at `<bundle>/assets/webchat-gui/skins/<name>/`.
`greentic-start` resolves assets from an extracted bundle overlay before the pack,
gated on the `greentic.cap.bundle_assets.read.v1` capability. This is the right
choice for a single tenant's brand that does not belong in the product.

**A separate `.gtpack`.** Not implemented. It is reachable — a skin pack must
mount at a sibling namespace such as `/v1/web/webchat-skins/{tenant}`, because
`validate_plan` in `greentic-start` treats a route nested under another pack's
route as an ambiguous overlap and refuses to activate the whole bundle. It was
deferred because a skin pack still has to be added to the bundle, so it costs a
publish pipeline per skin and buys only a versioned OCI artifact over the overlay.
Relative skin paths (above) are what would make this a drop-in change later.

## Assets are imported from upstream

`tools/import_webchat_gui_assets.sh` mirrors the SPA from the `greentic-webchat`
repo. Skins are handled differently from everything else it imports: it keeps
every skin directory tracked by git and prunes anything the import brought in
that git does not track, so a committed pack-local skin survives an import. If
the tracked allowlist comes back empty — for example a sparse checkout that
excludes `packs/` — the import refuses to prune anything and aborts loudly
instead of silently deleting every skin, `default` included.

Anything under `i18n/`, `config/`, `js/` and `assets/` is rsynced with
`--delete` and will be overwritten — never add translation keys there.

`skins/` IS rsynced too, unlike root-level files such as `runtime-bootstrap.js`
which the import never touches. It runs without `--delete`, so an import
cannot remove a repo-owned skin, but it still overwrites every file upstream
ships for a skin of the same name. If upstream's `greentic-webchat` still
ships `default/skin.json` or `3aigent/skin.json` with root-absolute paths,
the next import silently reverts this repo's relativization of those
manifests, and `tools/publish_packs_oci.sh` runs the import without running
the validator, so nothing catches it automatically. Run
`python3 tools/validate_skins.py` after any import.
