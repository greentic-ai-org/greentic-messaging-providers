# PR: Build Messaging Provider Packs From Answer Documents

## Problem

Messaging provider packs are currently maintained as a mix of hand-authored pack metadata, generated manifests, copied assets, generated registry fixtures, and compiled components. That makes provider contract changes brittle:

- Pack metadata can drift from component behavior.
- Setup assets can collect answers that `apply-answers` does not persist.
- Generated `.gtpack` manifests can differ from checked-in `pack.yaml` intent.
- Provider tests validate final artifacts, but the source of truth for rebuilding them is not explicit enough.

The email audit exposed this clearly: the setup asset is Microsoft Graph-oriented, the provider QA contract is SMTP-oriented, and the pack does not make the intended build/update flow obvious.

## Reference Pattern

`../greentic-demo` uses checked-in answer documents as durable build intent:

- `build-answer.json` contains bundle, pack-create, pack-update, and flow wizard answers.
- Build helpers extract sub-documents with `jq`.
- `greentic-pack wizard apply --answers ...` recreates or updates generated pack source.
- Crate-local `assets/` are overlaid onto generated pack source before final build.
- The checked-in generated pack tree is reproducible from answer files plus assets.

Provider packs should follow the same idea: answer documents define the pack/component/setup intent; generated pack metadata and artifacts are derived outputs.

## Design

Introduce answer-driven provider pack builds for `greentic-messaging-providers`.

1. Add a provider build answer schema.
   - Suggested schema id: `greentic-messaging-provider.build-answer`
   - Suggested file location: `packs/<provider>/build-answer.json`
   - It should include provider id, display name, version, component source, component manifest metadata, setup assets, docs assets, extension metadata, pack aliases, OCI category, and fixture expectations.

2. Add a provider build helper.
   - Suggested script: `tools/build_provider_from_answers.sh <provider>`
   - It extracts the provider answer document, rebuilds the pack source, overlays `packs/<provider>/assets`, copies compiled component artifacts, runs pack validation, builds the `.gtpack`, and regenerates registry fixtures.
   - Existing `scripts/build_providers.sh` can call this helper once the first provider is migrated.

3. Treat `pack.yaml` and generated manifests as derived artifacts where practical.
   - During migration, keep committed generated files for compatibility.
   - Tests must prove generated output from `build-answer.json` matches committed provider metadata.
   - Long term, reduce hand edits to answer files, component source, docs, and assets.

4. Make setup contracts answer-owned.
   - The build answer should define setup fields, secret mappings, provider-owned runtime keys, and startup extension metadata.
   - Generated setup assets and pack extension metadata must be derived from the same answer source.
   - This prevents split-brain cases where setup collects `ms_graph_client_id` while runtime reads a different key.

5. Add drift tests.
   - A provider fixture test should rebuild the pack metadata from `build-answer.json` and compare the generated manifest/extension metadata to committed artifacts.
   - Registry fixture tests should remain, but the failure message should point to the answer-driven rebuild command.

## Migration Plan

1. Pilot on email providers because they currently have the most confused contract.
2. Migrate one stable provider after that, likely Webex or Telegram, to prove the model is not email-specific.
3. Convert Teams once subscription desired-state metadata is fully stable.
4. Replace direct `pack.yaml` edits in provider tasks with answer-file edits.

## Tests

- Unit test the answer parser and generated extension metadata.
- Integration test `tools/build_provider_from_answers.sh messaging-microsoft-email` once the Microsoft provider exists.
- Fixture test that generated provider pack metadata is byte-for-byte stable after normalization.
- CI check that rejects provider pack metadata changes without corresponding answer changes.

## Non-Goals

- No host-side changes in this PR.
- No removal of existing `pack.yaml` files until at least two providers have migrated.
- No live provider API calls.

## Status

Done.

Implemented the initial migration slice with `packs/messaging-email/build-answer.json`, a provider build-answer validator, a build helper, and tests that lock build answers to committed pack metadata. The normal provider build script validates answer-owned providers before building.
