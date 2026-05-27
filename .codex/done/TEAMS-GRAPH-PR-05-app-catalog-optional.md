# Optional PR 5: Teams App Catalog Publish/Install Flow

## Review Status

Reviewed against the current codebase on 2026-05-27 and adapted.

This is a valid later enhancement, but it should not be part of the Graph-first migration critical path. The current provider can and should support Graph egress/ingress without publishing a Teams app package to the tenant catalog.

## Title

Add optional Teams app catalog publish/install flow

## Goal

Add optional Microsoft Teams app catalog support for a nicer install experience, without reintroducing Azure Bot Service or making app catalog install required for message send/receive.

## Ordering Constraint

Implement only after:

1. PR 1 Graph egress.
2. PR 2 Graph setup/schema/docs.
3. PR 3 Graph ingress/subscriptions.
4. PR 4 tester.

## Scope

Add a cleanly separated extension, for example:

```text
messaging.microsoft_app_catalog.v1
```

Include:

- manifest template path
- icons
- install scopes: `team`, `personal`
- whether admin review is required or supported

Add optional tester UI controls only when permissions exist:

- Generate app package
- Publish to tenant app catalog
- Install into selected Team

Document that this is optional polish, not a prerequisite for Graph messaging.

## Non-Goals

- Do not require Azure Bot Service.
- Do not require the app catalog flow for send/receive.
- Do not mix this with OAuth, egress, or subscription setup defaults.

## Acceptance Criteria

- Core Teams Graph send/receive still works without this flow.
- App catalog metadata is isolated behind its own extension.
- Tester buttons are optional and permission-aware.
- Docs explain this is optional.
