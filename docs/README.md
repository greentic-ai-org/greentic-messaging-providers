# Greentic Messaging Providers Documentation

This folder documents the Greentic messaging provider packs for two audiences:

- Web developers and operators who want to choose, configure, preview, and ship
  a provider without spelunking through Rust internals.
- Coding agents and maintainers who need stable file paths, commands, and
  implementation boundaries.

## Start Here

| Need | Read |
| --- | --- |
| Choose a provider | [Provider catalog](providers/README.md) |
| Understand one provider's features | Provider files under [docs/providers](providers/) |
| Embed WebChat GUI in an existing website | [WebChat GUI Web Component guide](guides/webchat-gui-embed-webcomponent.md) |
| Preview iframe/native/popup WebChat GUI modes | `scripts/test_webchat_gui.sh 3aigent --embedded` |
| Run provider tests locally | [Messaging tester guide](guides/testing/guide-messaging-tester.md) |
| Configure nightly live tests | [Provider nightly e2e](provider-e2e.md) |
| Release one provider | [Provider release operations](provider-release-operations.md) |
| Release shared provider code | [Shared crate release](shared-crate-release.md) |
| Understand the Rust/WASM architecture | [Architecture guide](guides/architecture/02-architecture.md) |
| Deliver a human approval to a channel | [Approval rail delivery](approval-rail.md) |

## Plain-Language Concepts

**Provider**

A provider connects Greentic to one messaging system, such as Slack, Telegram, Teams, or WebChat.

**Component**

A component is the compiled WebAssembly code that performs one job, such as sending a Slack message or handling a Telegram webhook.

**Pack**

A `.gtpack` is the deployable bundle for a provider. It contains components, setup questions, schemas, flows, and metadata.

**WebChat GUI**

The browser chat experience. It can be a direct full-page app or a
`<greentic-webchat>` Web Component with `mode` and `render` attributes.

**Secret**

A secret is a credential such as an API token or bot password. Secrets are referenced by name in docs and manifests. Secret values must never be committed.

**Setup Answers**

Setup answers are the values collected by `gtc setup` or an answers file. They become provider configuration.

## Coding Agent Rules

When changing provider behavior:

1. Start from [docs/providers/README.md](providers/README.md) and the provider-specific doc.
2. Use `ci/provider-matrix.json` to find owned components, pack paths, tests, and e2e metadata.
3. Keep provider-local changes scoped to that provider unless shared code is intentionally changed.
4. Run focused provider tests before broad workspace checks.
5. Do not put credentials in docs, fixtures, snapshots, or examples.

Useful commands:

```bash
python3 ci/provider_matrix.py list-providers
python3 tools/provider_versions.py validate --provider all
cargo test -p provider-tests provider_core_slack
PACK_FILTER=messaging-slack ./ci/steps/11_build_packs.sh
```

Replace `slack` and `messaging-slack` with the provider you are working on.
