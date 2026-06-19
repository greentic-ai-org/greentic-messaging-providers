# PR 5 - Share provider final-action links with greentic-setup

## Goal

Expose provider action links in a generic metadata format from this repo so `greentic-setup` can later render provider buttons without hard-coding Slack, Teams, WebEx, or Telegram behavior.

The first consumer will be the final setup screen, which should be able to show:

- `Add to Slack`
- `Add to Teams`
- `Add to WebEx`
- `Add to Telegram`

Each action must include the URL that the UI should open.

## Current State

Slack:

- `packs/messaging-slack/assets/setup.yaml` currently has an action labeled `Add to Slack` with OAuth authorize fields.
- `scripts/test_slack.sh` also has `Create Slack app` and `Add to Slack` buttons.
- The current Slack setup action should be relabeled `Setup Slack App`, because it sets up the Slack app rather than adding it to a workspace.
- The future final setup screen can still show the generic provider action `Add to Slack`.

Teams:

- The Teams setup web component already computes app package, publish, install, and open-chat links.
- `scripts/test_teams_bot.sh` displays `Add to Teams` when a Teams app link is available.

WebEx:

- Setup currently focuses on bot token and webhook registration.
- No generic final `Add to WebEx` link descriptor is present.

Telegram:

- Setup guides tell users to open Telegram and send a message.
- The generic final action needs a bot username or deep link before it can produce a useful URL.

## Metadata Design

Add a pack-level setup action descriptor that greentic-setup can read from pack metadata or setup assets.

Suggested extension id:

```text
greentic.setup.actions.v1
```

Suggested JSON shape:

```json
{
  "schema_id": "greentic.setup.actions.v1",
  "provider_id": "messaging-slack",
  "actions": [
    {
      "id": "add-to-slack",
      "label": "Add to Slack",
      "kind": "deep_link",
      "url_template": "{slack_add_url}",
      "style": "primary",
      "opens_new_window": true,
      "copyable": true,
      "requires": ["slack_add_url"],
      "visible_when": {
        "setup_status.ok": true
      }
    }
  ]
}
```

The descriptor should be data-only. It should not contain HTML snippets as the primary contract. If a setup script wants to show a preview, it can derive button HTML from the descriptor.

The setup flow action and the final-screen action are different concepts:

- Setup action: provider-specific task that gets credentials/configuration into a ready state. For Slack, this is `Setup Slack App`.
- Final action: generic provider launch/install/open action rendered by greentic-setup when the provider is included in a bundle. For Slack, this is `Add to Slack`.

## Common Fields

Required:

- `id`: stable provider-local action id.
- `label`: user-facing button text.
- `kind`: `external_link`, `oauth_authorize`, `deep_link`, `download`, or `provider_action`.
- `url_template`: URL with `{placeholder}` variables, or an API path for provider actions.

Recommended:

- `style`: `primary` or `secondary`.
- `opens_new_window`: boolean.
- `copyable`: boolean.
- `requires`: list of setup values needed to render the URL.
- `visible_when`: optional data condition evaluated by greentic-setup.
- `description`: short text for logs/tooltips, not required for button rendering.

## Provider Actions

Slack:

- Setup flow label: `Setup Slack App`.
- Final generic action label: `Add to Slack`.
- URL source: setup output should expose the final Slack add/install URL as a data field, for example `slack_add_url`, so greentic-setup can render it without understanding Slack-specific OAuth details.

Teams:

- `Add to Teams`: Teams app install/deep link generated from the Teams setup state.
- `Open Teams Chat`: optional secondary action when the bot chat URL is available.
- The descriptor should reference the setup state fields already produced by the Teams setup machine, such as `teams_app.add_to_teams_url` or equivalent generated output.

WebEx:

- `Add to WebEx`: deep link to open a conversation with the bot when `bot_email` or bot identity is known.
- If the exact WebEx deep-link URL cannot be generated from current setup data, add the descriptor with `requires: ["bot_email"]` and do not show the button until setup can provide it.

Telegram:

- `Add to Telegram`: `https://t.me/{bot_username}`.
- Optional start payload: `https://t.me/{bot_username}?start={start_payload}`.
- The descriptor should require `bot_username`; setup should obtain it from BotFather input or Telegram `getMe`.

## Pack/Asset Placement

Preferred placement:

- Add `greentic.setup.actions.v1` in each provider pack metadata where other setup extensions live.
- Keep a source asset such as `packs/<provider>/assets/setup-actions.json` if the pack build process needs to copy or validate it.

Do not require greentic-setup to parse shell scripts. Scripts can read the same descriptor for local testing, but pack metadata is the contract.

## Tests

Add fixture tests that verify generated pack metadata includes valid setup actions for:

- Slack
- Teams
- WebEx
- Telegram

Validate:

- schema id is `greentic.setup.actions.v1`;
- every action has `id`, `label`, `kind`, and `url_template`;
- all URL template variables are listed in `requires` or are known runtime variables;
- Slack setup flow uses `Setup Slack App`;
- Slack final action descriptor uses `Add to Slack`;
- Teams includes `Add to Teams`;
- WebEx includes `Add to WebEx` but may require `bot_email`;
- Telegram includes `Add to Telegram` and requires `bot_username`.

## Acceptance Criteria

- This repo exposes setup action descriptors without changes in greentic-setup.
- greentic-setup can later render provider-specific buttons from a single generic descriptor shape.
- Slack wording is split correctly: the provider setup step is `Setup Slack App`; the generic final-screen action is `Add to Slack`.
- Teams, WebEx, and Telegram have data descriptors for their final `Add to X` links.
