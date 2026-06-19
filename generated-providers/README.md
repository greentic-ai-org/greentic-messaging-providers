# Answer-Owned Providers

This directory is for new messaging providers whose generated pack inputs are
owned by a checked-in `build-answer.json`.

Current allowlist:

- `messaging-http`
- `messaging-websocket`

Existing providers under `components/`, `packs/`, and provider-specific roots
keep their current build and packaging flow. Do not move or convert existing
providers as part of this lane.

For providers in this directory:

- `build-answer.json` is the source of truth for generated pack metadata.
- `src/` contains provider implementation source.
- `assets/` contains setup specs, schemas, docs, and fixtures.
- `tests/` contains provider-local tests and fixtures.
- generated pack inputs belong under `target/generated/<provider>.pack`.
- final package artifacts belong under `dist/packs/<provider>.gtpack`.

Generated YAML, JSON, CBOR, and package internals for these providers should be
reproducible from `build-answer.json` and provider source/assets.
