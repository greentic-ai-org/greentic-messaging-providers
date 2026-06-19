# PR 4 - Add provider test scripts for lifecycle events and setup links

## Goal

Extend the local provider test scripts so developers can validate both parts of the feature:

- provider ingress emits `channel.user.entered` for native lifecycle events;
- setup/test UIs expose the final provider action links with the right labels.

## Files

Primary script targets:

- `scripts/test_slack.sh`
- `scripts/test_teams_bot.sh`
- `scripts/test_webex.sh`
- Telegram setup checks where existing Telegram script or fixtures live.

`scripts/test_teams.sh` appears to cover older Graph/device-code behavior. Do not make it the primary Bot Framework lifecycle harness unless the implementation still routes through that path.

## Slack Script Design

Add local test actions that post representative Slack Events API payloads to the local ingress endpoint:

- `app_home_opened`
- `member_joined_channel`

For each action, assert or display:

- HTTP response success.
- One normalized event is returned or logged.
- `metadata.event_type == "channel.user.entered"`.
- `metadata.autoStart == "true"`.
- `metadata.reason` matches the native event.
- `metadata.idempotency_key` is present.

Update UI labels:

- Use `Setup Slack App` for the current Slack setup step. That step configures/creates the Slack app; it does not add the app to a workspace.
- Do not use `Add to Slack` inside the Slack setup flow.
- Reserve `Add to Slack` for the future generic final setup screen action described in PR 5.

Keep DOM ids stable where practical, because local script JavaScript and existing docs may refer to ids such as `addToSlackBtn`.

## Teams Script Design

Add a Bot Framework lifecycle simulation in `scripts/test_teams_bot.sh`:

- POST a `conversationUpdate` activity with `membersAdded`.
- POST an `installationUpdate` add activity if the handler supports it.

For each action, assert or display:

- normalized `events` contains a lifecycle envelope;
- `metadata.event_type == "channel.user.entered"`;
- `metadata.reason` is `members_added`, `bot_added`, or `app_installed`;
- `metadata.conversation_id` matches the Bot Framework conversation id;
- `metadata.tenant_id` is preserved when present;
- `metadata.idempotency_key` is present.

Preserve existing Teams setup behavior:

- Keep `Add to Teams` as the user-facing button/link for installing the Teams app package.
- Keep `Open bot chat` or equivalent secondary link where already generated.

## WebEx Script Design

Add a local webhook simulation in `scripts/test_webex.sh`:

- POST a `memberships.created` webhook payload with `roomId`, `personId`, `personEmail`, and membership id.

Assert or display:

- normalized lifecycle event is emitted;
- `metadata.event_type == "channel.user.entered"`;
- `metadata.reason == "space_membership_created"`;
- `metadata.room_id` matches the WebEx room id;
- `from.id` or `from.email` is populated;
- `metadata.idempotency_key` is present.

The script should not require a live WebEx Messages API lookup for the membership-created lifecycle test.

## Telegram Script/Fixture Design

Keep `/start` validation as the acceptance test for Telegram auto-start.

If a Telegram setup script or fixture already exists, add output for the final action link:

- Label: `Add to Telegram`.
- URL: `https://t.me/{bot_username}` or `https://t.me/{bot_username}?start={payload}` when the bot username is known.

## Regression Tests

Add focused non-live tests where possible so CI can validate the feature without Slack/Teams/WebEx credentials:

- Slack sample payload normalization.
- Teams Bot Framework sample payload normalization.
- WebEx membership-created sample payload normalization.
- Setup action metadata fixtures include expected labels and URL templates.

Live scripts may remain manual, but CI should cover the JSON normalization and setup action descriptors.

## Acceptance Criteria

- Running Slack tests can prove App Home and channel-join lifecycle normalization.
- Running Teams Bot Framework tests can prove conversation/update lifecycle normalization.
- Running WebEx tests can prove membership-created lifecycle normalization.
- Slack setup UI wording no longer shows `Add to Slack`; it shows `Setup Slack App`.
- Setup/test output displays generic final action links for Slack, Teams, WebEx, and Telegram when each provider has enough data to generate the URL.
