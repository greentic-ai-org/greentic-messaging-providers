# PR-05 — Add checks for the two answer-owned providers only

## Goal

Add local/CI checks for the new answer-owned providers `messaging-http` and `messaging-websocket`.

This PR must not redesign CI for all providers and must not change existing provider CI behavior except to include the two new providers where appropriate.

## Non-goals

- Do not replace `ci/local_check.sh`.
- Do not introduce repo-wide provider change detection.
- Do not require existing providers to use `build-answer.json`.
- Do not change existing provider build matrix entries except for adding the two new provider entries.

## Local Checks

Add a narrow script or extend an existing script with an explicit allowlist:

```bash
scripts/check_answer_provider.sh messaging-http
scripts/check_answer_provider.sh messaging-websocket
```

For any other provider ID, fail clearly and do nothing.

Each check should run:

1. answer file validation
2. provider source formatting/linting relevant to that provider
3. focused provider tests
4. answer-owned generation
5. answer-owned packaging
6. pack validation/doctor

## CI Integration

Add the two new providers to existing provider metadata in the least disruptive way.

Requirements:

- add `messaging-http` to `ci/provider-matrix.json`
- add `messaging-websocket` to `ci/provider-matrix.json`
- wire their test command to the answer-provider check script
- include paths only for their new answer-owned directories and focused tests
- avoid broad `crates/**` or `scripts/**` changes unless needed by existing matrix conventions

Do not use this PR to rework matrix generation for old providers.

## Change Detection

If the existing CI already supports provider path filtering, add paths for:

```text
generated-providers/messaging-http/**
generated-providers/messaging-websocket/**
```

If it does not, leave broader CI behavior intact and document that provider-scoped optimization is deferred.

## Acceptance Criteria

- `scripts/check_answer_provider.sh messaging-http` passes.
- `scripts/check_answer_provider.sh messaging-websocket` passes.
- CI can run checks for both new providers.
- Existing provider CI behavior is preserved.
- Unsupported provider IDs are rejected by the answer-provider check script.
