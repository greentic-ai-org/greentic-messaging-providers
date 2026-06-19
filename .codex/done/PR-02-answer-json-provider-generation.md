# PR-02 — Add answer.json generation support for HTTP and WebSocket only

## Goal

Add generation scripts for the two new providers so `messaging-http` and `messaging-websocket` are built from answer files in the same spirit as `greentic-demo` and the existing answer-owned Teams flow.

This PR must not make existing providers depend on the new generation path.

## Non-goals

- Do not convert existing providers to answer.json.
- Do not make `generate_provider.sh <existing-provider>` work.
- Do not rewrite existing `packs/*/pack.yaml`, `pack.manifest.json`, `pack.lock.cbor`, or component manifests.
- Do not remove existing provider-specific packaging scripts.

## Required Scripts

Add narrow scripts with an allowlist:

```bash
scripts/generate_answer_provider.sh messaging-http
scripts/generate_answer_provider.sh messaging-websocket
scripts/package_answer_provider.sh messaging-http
scripts/package_answer_provider.sh messaging-websocket
```

If a caller passes any other provider ID, fail with a clear error:

```text
answer-owned provider generation currently supports only messaging-http and messaging-websocket
```

## Generation Responsibilities

For each allowed provider:

1. Load `<provider>/build-answer.json`.
2. Validate the answer file schema and required fields.
3. Apply the answer file using the same canonical Greentic wizard/pack path already used by generated packs in this repo.
4. Materialize generated pack inputs under `target/generated/<provider>.pack`.
5. Build any provider WASM component needed by the generated pack.
6. Build the GTPACK into `dist/packs/<provider>.gtpack`.
7. Run pack validation/doctor using the repo's existing validator behavior.

Prefer reusing or generalizing the proven `messaging-teams/build_pack.sh` pattern where it makes sense. Any shared helper must remain compatible with `messaging-teams`.

## Answer File Requirements

Use the existing Greentic pack wizard answer format where possible:

```json
{
  "schema_id": "greentic-pack.wizard.answers",
  "schema_version": "1.0.0",
  "answers": {
    "create_pack_id": "messaging-http",
    "pack_dir": "target/generated/messaging-http.pack"
  }
}
```

The exact file may contain additional provider-specific sections, but `build-answer.json` must remain the source of truth for generated pack metadata.

## Manual Edit Rule

For the two new providers, do not manually maintain these generated files:

- generated `pack.yaml`
- generated `pack.manifest.json`
- generated `pack.lock.cbor`
- generated component manifests
- generated GTPACK artifact internals

Generated output may be committed only if the repo already expects that artifact class to be checked in for provider packs. If committed, it must be reproducible from `build-answer.json`.

## Acceptance Criteria

- `scripts/generate_answer_provider.sh messaging-http` works.
- `scripts/generate_answer_provider.sh messaging-websocket` works.
- `scripts/package_answer_provider.sh messaging-http` produces `dist/packs/messaging-http.gtpack`.
- `scripts/package_answer_provider.sh messaging-websocket` produces `dist/packs/messaging-websocket.gtpack`.
- Unsupported provider IDs fail without touching existing provider files.
- Existing provider build/package commands still work.
