# PR-01: Teams Setup Wizard Prototype

## Title

Add reusable Teams setup wizard web component and mount it in the local tester

## Goal

Create a focused, reusable setup wizard for Teams setup and validate it inside
`scripts/test_teams_bot.sh`.

This PR is intentionally limited to the wizard prototype and tester harness. It
does not migrate the Bot Framework runtime into Rust/WASM and does not yet ship
the component from the Teams pack.

## Context

The Teams Bot Framework tester proved the desired admin flow:

1. Graph/admin sign-in.
2. Teams app publish.
3. Teams app install for user.
4. Open bot chat.
5. Wait for first Teams activity.
6. Send Adaptive Card.
7. Click card action and receive follow-up card.

The raw tester UI exposed too many internal controls. The target UX is a guided
wizard with one clear action per step and recoverable device-code OAuth.

## Scope

Add:

```text
messaging-teams/webcomponent/
  greentic-teams-setup.js
  greentic-teams-setup.d.ts
  README.md
  package.json
  examples/basic.html
```

Update `scripts/test_teams_bot.sh` to:

- serve the component from source with `Cache-Control: no-store`
- mount `<greentic-teams-setup>` at the top of the tester
- keep the raw tester controls below as diagnostics only
- keep the existing tester API shape available for the component:
  - `GET /api/state`
  - `POST /api/setup/next`
  - `POST /api/config`
  - `POST /api/oauth/{graph,management}/start`
  - `POST /api/oauth/{graph,management}/complete`
  - `POST /api/teams-app/publish`
  - `POST /api/teams-app/install-me`
  - `GET /teams-app/package.zip`

## Web Component Requirements

The component must:

- define `customElements.define("greentic-teams-setup", ...)`
- use plain browser Web Component APIs
- use Shadow DOM
- expose theme CSS variables
- support partial i18n overrides
- show progress, current action, completed outcome, and errors
- show exactly one primary action per step
- allow only targeted secondary recovery actions, such as `Refresh code`
- emit host-observable events:
  - `greentic-teams-setup-state`
  - `greentic-teams-setup-result`
  - `greentic-teams-setup-action-start`
  - `greentic-teams-setup-action-complete`
  - `greentic-teams-setup-action-timeout`
  - `greentic-teams-setup-device-login`
  - `greentic-teams-setup-error`
  - `greentic-teams-setup-copy-code`

## Device-Code OAuth Requirements

The wizard must:

- show the Microsoft user code before opening a new tab
- offer `Copy code`
- open Microsoft device login only after the admin clicks the primary action
- keep `Refresh code` visible during device-login polling
- refresh the code in place without hiding the code panel
- poll `/api/oauth/{kind}/complete`, not `/api/setup/next`
- call `/api/setup/next` only after OAuth succeeds
- clear stale pending OAuth state when appropriate
- show timeout/retry when login does not complete

## Acceptance Criteria

- `scripts/test_teams_bot.sh` starts and displays the embedded wizard.
- Browser refresh loads the latest component code from source.
- Wizard can drive the current tester happy path.
- Device-code login does not churn codes while polling.
- `Refresh code` works while login is pending.
- Graph sign-in completion advances the wizard.
- Timeout state is visible and recoverable.
- Raw tester controls remain available for debugging below the wizard.

## Tests

Minimum checks:

```text
node --check messaging-teams/webcomponent/greentic-teams-setup.js
bash -n scripts/test_teams_bot.sh
python3 -m py_compile /tmp/test_teams_bot_server.py
```

Manual validation:

- start `scripts/test_teams_bot.sh --no-open`
- hard-refresh tester page
- complete Graph device login
- publish/install Teams app
- open bot chat
- send first message
- send Adaptive Card
- click card action and receive follow-up card

## Out Of Scope

- Shipping the component from `packs/messaging-teams`.
- Moving setup endpoints out of the tester.
- Porting Node Bot Framework sidecar behavior into Rust/WASM.
- Removing Python/JavaScript tester code.
- Azure Bot Service as the default runtime.

## Follow-Up

- PR-02: package the setup wizard as a Teams pack asset and expose provider setup endpoints.
- PR-03: migrate Bot Framework ingress and Adaptive Card submit handling into Rust/WASM.
- PR-04: add conformance tests and retire the Node/Python happy-path dependencies.
