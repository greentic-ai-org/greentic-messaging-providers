# PR 6 - Rename ambiguous Slack setup action

## Goal

Remove user-facing `Add to Slack` wording from the Slack setup flow and replace it with the more precise setup label.

Use:

- `Setup Slack App` for the existing Slack setup step.
- `Add to Slack` only for the future generic final setup screen action described in PR 5.

The current Slack setup action does not add the app to Slack from the user's point of view; it sets up the Slack app. That is why the setup-flow label should be `Setup Slack App`.

## Current Occurrences

Known source occurrences:

- `scripts/test_slack.sh`: setup action title currently says `Add to Slack`.
- `scripts/test_slack.sh`: UI button currently says `Create Slack app`.
- `scripts/test_slack.sh`: UI button currently says `Add to Slack`.
- `scripts/test_slack.sh`: error text says to use `Add to Slack`.
- `packs/messaging-slack/assets/setup.yaml`: action label currently says `Add to Slack`.

Generated pack manifests or lock files may also contain the old label after rebuilding. Update generated artifacts only if this repo normally checks them in for pack metadata changes.

## Design

Split Slack wording into two distinct concepts.

Provider setup flow:

- Label: `Setup Slack App`.
- Kind: `provider_action` if the local setup machine creates the app via Slack API.
- Kind: `external_link` if the user must open Slack's app setup/configuration page.
- This replaces the current misleading setup-flow `Add to Slack` label.

Generic final setup screen:

- Label: `Add to Slack`.
- Kind: generic setup action from `greentic.setup.actions.v1`.
- URL comes from setup output, for example `slack_add_url`.
- This action is rendered by greentic-setup only after provider setup is complete and only when the bundle includes Slack.

## File-Level Plan

`scripts/test_slack.sh`:

- Change visible `Create Slack app` text to `Setup Slack App` unless it is clearly a secondary provider-specific action.
- Change visible setup-flow `Add to Slack` text to `Setup Slack App`.
- Update error/help text to say: set up the Slack app first.
- Keep existing DOM ids when possible to avoid breaking JavaScript handlers.
- If the setup action object at the top represents the current Slack setup flow, label it `Setup Slack App`.

`packs/messaging-slack/assets/setup.yaml`:

- Label the current setup action `Setup Slack App`.
- Do not leave `Add to Slack` in checked-in Slack setup-flow metadata.
- The later generic final-screen `Add to Slack` descriptor belongs with `greentic.setup.actions.v1`, not as the setup-flow action label.

Pack metadata:

- Rebuild or update generated pack metadata only according to the repo's normal pack generation flow.
- Verify `pack.manifest.json` and fixtures do not reintroduce `Add to Slack` as a Slack setup-flow action if they are checked in.
- It is valid for the future generic final-screen action descriptor from PR 5 to contain `Add to Slack`.

Docs:

- Update provider docs or testing docs only where they describe the setup UI.
- Keep generic marketplace language untouched unless it specifically labels the Slack setup action.

## Search Command

Before finishing the implementation PR, run:

```sh
rg -n "Add to Slack|Create Slack app|Create Slack App|Install Slack App" scripts packs docs .codex
```

Expected result:

- No setup-flow `Add to Slack`.
- No lowercase `Create Slack app`.
- No `Create Slack App` or `Install Slack App` labels unless a separate, explicit flow is later introduced.
- `Add to Slack` appears only in the generic final-screen action descriptor/tests from PR 5.
- `Setup Slack App` appears for the current Slack setup action.

## Acceptance Criteria

- Slack setup no longer displays `Add to Slack`.
- Slack setup displays `Setup Slack App`.
- Generic final-screen metadata can still provide `Add to Slack`.
- Tests and fixtures agree with the new wording.
