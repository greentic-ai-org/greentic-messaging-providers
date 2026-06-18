# How To Publish One Provider

Use this when you want to release one messaging provider, for example
`webchat-gui` or `slack`, without rebuilding and publishing every provider.

## Provider Names

Provider names are the keys in `ci/provider-matrix.json`. The common names are:

```text
dummy
email
slack
teams
telegram
webchat
webchat-gui
webex
whatsapp
```

`teams` publishes the current Bot Framework-backed `messaging-teams` pack to
`ghcr.io/<owner>/packs/messaging/messaging-teams`. Use
`messaging-teams-graph` only when you intentionally need the legacy
Graph-backed Teams provider.

You can also ask the matrix script:

```bash
python3 ci/provider_matrix.py list-providers --format text
```

## Change One Provider Version

Use `scripts/change_provider_version.sh`. It changes the version, validates the
provider metadata, and builds that provider by default.

```bash
scripts/change_provider_version.sh webchat-gui 0.4.99
```

That command runs the equivalent of:

```bash
python3 tools/provider_versions.py set-provider webchat-gui 0.4.99
python3 tools/provider_versions.py validate --provider webchat-gui
scripts/build_providers.sh webchat-gui
```

Use `--no-build` only when you intentionally want to update metadata without
building immediately:

```bash
scripts/change_provider_version.sh --no-build webchat-gui 0.4.99
```

## Rebuild After Changes

If you change provider code, pack files, webchat GUI assets, docs included in the
pack, or anything else after the version bump, rebuild that provider again before
publishing:

```bash
scripts/build_providers.sh webchat-gui
```

This keeps `dist/packs/<pack>.gtpack`, `packs/<pack>/pack.lock.cbor`, and
`packs.lock.json` in sync with the latest local files.

## Push First

GitHub Actions can only see committed and pushed changes. Commit the version
bump, pack changes, and any source changes, then push the branch you want to
release from.

```bash
git status --short
git add <changed-files>
git commit -m "Release webchat-gui 0.4.99"
git push
```

Use `main` or `develop` according to the release branch you intend to publish.
Pushing to `main` or `develop` does not publish every provider automatically.
Publishing is a separate explicit workflow run.

## Publish One Provider

### Option A: One Local Command

The helper below validates, builds, runs targeted local checks, dispatches the
one-provider GitHub workflow on the current branch, and watches it:

```bash
scripts/publish_provider.sh webchat-gui 0.4.99
```

Useful flags:

```bash
scripts/publish_provider.sh webchat-gui 0.4.99 --dry-run
scripts/publish_provider.sh webchat-gui 0.4.99 --publish-latest
scripts/publish_provider.sh webchat-gui --skip-build
scripts/publish_provider.sh webchat-gui --skip-local-check
```

`--dry-run` dispatches the workflow with `publish=false`, so it builds and
uploads artifacts but does not push GHCR tags.

### Option B: GitHub Actions UI

Run the workflow named **Provider Build, Test, and Publish**
(`.github/workflows/provider-build-publish.yml`) on the branch you pushed.

Use these inputs for a focused publish:

```text
provider: webchat-gui
provider_version: 0.4.99        # optional; normally leave empty if the pushed metadata is correct
publish: true
publish_latest: false           # true only if this version should move latest
trigger_e2e_after_publish: false # true only when you want the live/provider e2e follow-up
```

For a validation-only run, set:

```text
publish: false
```

Do not use **Provider Release Orchestrator** for a normal one-provider release
unless you specifically need orchestration. If you do use it, set `providers` to
the single provider name, not `all`.

## WebChat GUI Local Testing

`scripts/test_webchat_gui.sh` builds and extracts the current `webchat-gui` pack,
serves a local test app, and opens it in the browser. It uses a mocked Direct
Line backend, so web developers and coding agents can validate the GUI without a
live deployment.

Default skin:

```bash
scripts/test_webchat_gui.sh
```

Specific skin:

```bash
scripts/test_webchat_gui.sh 3aigent
```

Specific skin, forced to the signed-out login page:

```bash
scripts/test_webchat_gui.sh 3aigent --login
```

Skip rebuilding when you already built the pack:

```bash
scripts/test_webchat_gui.sh 3aigent --no-build
```

Use a different port or avoid opening a browser:

```bash
scripts/test_webchat_gui.sh 3aigent --port 8790 --no-open
```

Test embedded mode:

```bash
scripts/test_webchat_gui.sh 3aigent --embedded
scripts/test_webchat_gui.sh --embedded
```

`--embedded` serves a host-style demo page at `test.html`. The page loads the
pack's `embed.js` and shows the important website integration shapes together:

- `<greentic-webchat mode="inline" render="iframe">`
- `<greentic-webchat mode="inline" render="native">`
- `<greentic-webchat mode="popup" render="iframe">`
- direct full-page WebChat without a wrapper iframe

`--login` serves `login-required.html`, clears the local test auth session, and
then redirects to the real full-page WebChat app for that skin. Use it when the
login screen itself needs visual validation.

Use iframe rendering for safe drop-in isolation. Use native rendering when the
host website should style the chat directly.

Add the built-in demo top-bar links:

```bash
scripts/test_webchat_gui.sh 3aigent --demo-links
```

Add custom top-bar links:

```bash
scripts/test_webchat_gui.sh 3aigent --nav-link 'Docs|https://docs.greentic.ai'
scripts/test_webchat_gui.sh 3aigent --nav-link 'M1|Playground|https://example.com'
```

Or pass JSON directly or from a file:

```bash
scripts/test_webchat_gui.sh 3aigent --nav-links-json '[{"label":"Docs","url":"https://docs.greentic.ai"}]'
scripts/test_webchat_gui.sh 3aigent --nav-links-json @/tmp/nav-links.json
```

The harness writes a temporary tenant config for the selected skin, so
`scripts/test_webchat_gui.sh 3aigent` should actually load the `3aigent` skin
even when no demo links are provided.
