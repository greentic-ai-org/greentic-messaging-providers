# PR 2 - Add provider normalizers for Slack, Teams, and WebEx

## Goal

Convert provider-native lifecycle webhooks into the `channel.user.entered` envelope contract from PR 1.

This PR should not add menu logic. It only makes provider ingress produce normalized lifecycle events.

## Shared Contract

Every new normalized lifecycle event must set:

```json
{
  "metadata": {
    "event_type": "channel.user.entered",
    "autoStart": "true",
    "provider": "slack|teams|webex",
    "reason": "...",
    "idempotency_key": "..."
  }
}
```

The event should be returned through the same ingress response shape already used for user messages, normally under `events`.

## Slack

Primary files:

- `components/messaging-provider-slack/src/ops/ingest.rs`
- `components/messaging-provider-slack/src/ops/mod.rs`
- `components/messaging-ingress-slack/src/lib.rs`

The repo currently has two Slack normalization paths: the universal ops provider and the `provider:common/ingress@0.0.2` wrapper. Update both paths or extract a small shared helper so they cannot diverge.

Native events to normalize:

- `event_callback.event.type == "app_home_opened"`
- `event_callback.event.type == "member_joined_channel"`

`app_home_opened` mapping:

- `session_id`: Slack user id if no channel id is present; otherwise the native channel id.
- `from.id`: `event.user`.
- `metadata.provider`: `slack`.
- `metadata.reason`: `app_home_opened`.
- `metadata.team_id`: top-level `team_id` or authorizations team id when present.
- `metadata.channel_id`: `event.channel` when present.
- `metadata.user_id`: `event.user`.
- `metadata.idempotency_key`: `lifecycle.user_entered:slack:{team_id}:{channel_or_app_home}:{user}:app_home_opened`.

`member_joined_channel` mapping:

- `session_id`: `event.channel`.
- `from.id`: `event.user`.
- `metadata.reason`: `member_joined_channel`.
- `metadata.team_id`: top-level `team_id`.
- `metadata.channel_id`: `event.channel`.
- `metadata.user_id`: `event.user`.
- `metadata.idempotency_key`: `lifecycle.user_entered:slack:{team_id}:{channel}:{user}:member_joined_channel`.

Keep existing safeguards:

- URL verification still returns the Slack challenge.
- Slack retry headers are still acknowledged without duplicate processing.
- Bot-authored message events are still ignored.

## Teams

Primary files:

- `components/messaging-ingress-teams/src/lib.rs`
- `messaging-teams/src/bot_framework.rs`

The active Teams path is Bot Framework ingress, not the legacy Teams Graph provider. Implement lifecycle normalization in `messaging-teams/src/bot_framework.rs` so both setup/runtime callers receive the same normalized result.

Native activities to normalize:

- `activity.type == "conversationUpdate"` with `membersAdded`.
- `activity.type == "installationUpdate"` with action/add semantics, if present in test fixtures or live payloads.

`conversationUpdate` mapping:

- Emit one lifecycle envelope for each relevant member in `membersAdded`.
- Ignore members that clearly represent the bot recipient when that can be determined from `activity.recipient.id`.
- `session_id`: `activity.conversation.id`.
- `from.id`: member id, or `activity.from.id` when member id is unavailable.
- `metadata.provider`: `teams`.
- `metadata.reason`: `members_added` or `bot_added` when the bot installation is the native event.
- `metadata.tenant_id`: `activity.channelData.tenant.id`.
- `metadata.conversation_id`: `activity.conversation.id`.
- `metadata.user_id`: member id.
- `metadata.service_url`: existing service URL.
- `metadata.idempotency_key`: `lifecycle.user_entered:teams:{tenant_id}:{conversation_id}:{user_id}:{reason}`.

`installationUpdate` mapping:

- `reason`: `app_installed` or `bot_added`.
- `session_id`: `activity.conversation.id`.
- Use the same tenant/conversation/user/idempotency metadata as `conversationUpdate`.

Keep existing behavior:

- Bearer token validation remains unchanged.
- Existing message, submit, reply, and invoke normalization remains unchanged.
- The returned JSON should continue to include `event` and `events` in the current shape, with lifecycle envelopes added only for the lifecycle activity types.

## WebEx

Primary files:

- `components/messaging-provider-webex/src/ops/ingest.rs`
- `components/messaging-provider-webex/src/ops/ingest_helpers.rs`

Native events to normalize:

- `resource == "memberships"` and `event == "created"`.

Mapping:

- `session_id`: `data.roomId`.
- `from.id`: `data.personId` when present.
- `from.email`: `data.personEmail` when present.
- `metadata.provider`: `webex`.
- `metadata.reason`: `space_membership_created`.
- `metadata.room_id`: `data.roomId`.
- `metadata.user_id`: `data.personId`.
- `metadata.person_email`: `data.personEmail`.
- `metadata.membership_id`: `data.id`.
- `metadata.idempotency_key`: `lifecycle.user_entered:webex:{room_id}:{person_id_or_email}:space_membership_created`.

Do not fetch message details for membership events. The current WebEx message path correctly fetches message content for `messages.created`; lifecycle events should be built directly from the webhook body.

Filtering:

- Continue ignoring bot-authored `messages.created` events.
- For `memberships.created`, avoid emitting a user-entered event for the bot's own membership if the bot identity is available in config or webhook data. If bot identity is not available, emit the event with a clear `reason` and rely on idempotency instead of dropping valid user events.

## Tests

Each provider normalizer should add unit tests with representative native payloads:

- Slack `app_home_opened` returns one `channel.user.entered` envelope.
- Slack `member_joined_channel` returns one `channel.user.entered` envelope.
- Teams `conversationUpdate.membersAdded` returns one lifecycle envelope for the human member.
- Teams `installationUpdate` returns one lifecycle envelope when action/add semantics are present.
- WebEx `memberships.created` returns one lifecycle envelope without calling message lookup.

Acceptance requires the tests to assert `metadata.event_type`, `metadata.autoStart`, `metadata.reason`, `session_id`, and `metadata.idempotency_key`.
