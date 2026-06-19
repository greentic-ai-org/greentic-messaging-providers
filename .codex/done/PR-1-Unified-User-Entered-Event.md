# PR 1 - Unified "user entered channel/app" lifecycle event

## Goal

Show the default Greentic menu when a user enters a messaging surface without first sending arbitrary text.

Already-working reference behavior:

- WebChat starts the default flow when DirectLine creates a conversation. `components/messaging-provider-webchat/src/ops/ingest.rs` emits a normal `ChannelMessageEnvelope` with `metadata.autoStart=true`.
- Telegram starts the default flow when the user sends `/start`. `components/messaging-provider-telegram/src/ops/ingest.rs` emits a normal message envelope whose text is `/start`.

Slack, Teams, and WebEx should follow the same operator-facing model: emit a normal provider ingress event that the runner can treat as "start the default flow now", without requiring a user text message.

## Design

Do not require a new Rust enum in this PR. Use a metadata contract on the existing `ChannelMessageEnvelope` shape so all providers can adopt it without a cross-repo type migration.

Canonical event marker:

```json
{
  "metadata": {
    "event_type": "channel.user.entered",
    "autoStart": "true",
    "provider": "slack|teams|webex|telegram|webchat",
    "reason": "app_home_opened|bot_added|member_joined|space_membership_created|conversation_started|start_command"
  }
}
```

`autoStart=true` is kept for compatibility with the current WebChat behavior. `event_type=channel.user.entered` is the new provider-neutral signal.

Recommended full envelope fields:

```json
{
  "schema_version": "messaging.channel.envelope.v1",
  "message_id": "lifecycle:{provider}:{native_event_id_or_ts}",
  "provider": "slack|teams|webex|telegram|webchat",
  "channel": "slack|teams|webex|telegram|webchat",
  "session_id": "provider-native conversation identifier",
  "text": "",
  "from": {
    "id": "provider user id",
    "display_name": "optional display name",
    "email": "optional email"
  },
  "metadata": {
    "event_type": "channel.user.entered",
    "autoStart": "true",
    "reason": "app_home_opened|bot_added|member_joined|space_membership_created|conversation_started|start_command",
    "tenant_id": "optional provider tenant id",
    "team_id": "optional Slack workspace or Teams team id",
    "channel_id": "optional Slack channel id",
    "conversation_id": "optional Teams conversation id",
    "room_id": "optional WebEx room id",
    "chat_id": "optional Telegram chat id",
    "user_id": "provider user id",
    "idempotency_key": "lifecycle.user_entered:{provider}:{scope}:{conversation}:{user}:{reason}"
  },
  "raw": {}
}
```

Provider-specific identifiers stay in `metadata` rather than forcing them into a single top-level field. The common session key remains `session_id`.

## Provider Scope

Slack:

- App Home open should emit `channel.user.entered`.
- Channel membership events can emit the same event when they represent a user entering a bot-addressable conversation.

Teams:

- Bot Framework `conversationUpdate` and `installationUpdate` events should emit `channel.user.entered` when the bot is installed, opened, or a user is added to the bot conversation.
- Use the Bot Framework conversation id as `session_id`.

WebEx:

- `memberships.created` webhook events should emit `channel.user.entered` when the bot is added to a space or when a user joins a bot-addressable room.
- Direct-message first contact can keep working through `messages.created`; only add lifecycle handling for no-text entry points.

Telegram:

- Keep `/start` behavior working.
- Optionally annotate `/start` envelopes with `metadata.event_type=channel.user.entered` later, but do not block Slack/Teams/WebEx on that cleanup.

WebChat:

- Keep `metadata.autoStart=true`.
- Optionally add `metadata.event_type=channel.user.entered` to the existing auto-start envelope for consistency.

## Idempotency

Each lifecycle envelope must include a stable `metadata.idempotency_key` when enough native identifiers are available. Suggested format:

```text
lifecycle.user_entered:{provider}:{tenant_or_team}:{conversation_or_room}:{user}:{reason}
```

Native retry ids or timestamps may be included in `message_id`, but the idempotency key should not change across delivery retries of the same logical entry event.

## Acceptance Criteria

- Slack App Home open can start the default flow without user text.
- Teams bot install/open can start the default flow without user text.
- WebEx bot added to a room can start the default flow without user text.
- WebChat conversation creation still auto-starts.
- Telegram `/start` still starts.
- Repeated provider retries or duplicate lifecycle events do not send duplicate menus.
