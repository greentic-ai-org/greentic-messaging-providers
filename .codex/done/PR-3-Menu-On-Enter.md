# PR 3 - Menu-on-enter flow handler

## Goal

Make the default flow start when the runner receives a `channel.user.entered` lifecycle envelope from any provider.

This is mostly a greentic-start/operator behavior, but this repo must define the provider contract precisely so the runtime change can be implemented without provider-specific branching.

## Input Contract

The handler should react to envelopes where:

```json
{
  "metadata": {
    "event_type": "channel.user.entered",
    "autoStart": "true"
  }
}
```

For backward compatibility, the handler should also continue to react to existing WebChat envelopes with only:

```json
{
  "metadata": {
    "autoStart": "true"
  }
}
```

Telegram `/start` should continue through the existing text-message path. If Telegram later adds `event_type=channel.user.entered`, the same handler can process it.

## Handler Behavior

```text
receive ChannelMessageEnvelope
  -> if metadata.event_type == channel.user.entered or metadata.autoStart == true
  -> compute channel session key from provider + session_id + tenant/team metadata
  -> check lifecycle idempotency key
  -> create or resume channel session
  -> start the default flow without requiring user text
  -> send the configured welcome/menu response through normal provider egress
```

No provider should send a menu directly from ingress. Ingress only reports the lifecycle event. The runner decides which default flow starts and how the response is rendered.

## Idempotency and Spam Control

Use the provider-supplied `metadata.idempotency_key` when present. Fallback key:

```text
lifecycle.user_entered:{provider}:{session_id}:{from.id}:{reason}
```

Recommended policy:

- Provider retries of the same native event must not send another menu.
- Repeated App Home opens should not spam the user. A short cooldown or "last menu sent" marker per user/session is acceptable.
- A new session or a different conversation can start the menu again.
- The idempotency record should include provider, session id, user id, reason, and timestamp for debugging.

## Session Selection

Use the same session identity as normal inbound messages from that provider:

- Slack: channel id for channel events; user id or App Home scope for App Home events.
- Teams: Bot Framework conversation id.
- WebEx: room id.
- WebChat: DirectLine conversation id.
- Telegram: chat id.

This keeps later user replies in the same flow session that was created by the lifecycle event.

## Provider Output Requirements

Providers should not invent menu text. They should emit:

- `text: ""` or a small neutral native text only when existing envelope validation requires text.
- `metadata.event_type=channel.user.entered`.
- `metadata.autoStart=true`.
- `metadata.reason`.
- `metadata.idempotency_key`.
- Native raw payload in `raw` or existing debug metadata.

## Acceptance Criteria

- A Slack App Home open causes exactly one default menu response.
- A Teams bot install/open causes exactly one default menu response.
- A WebEx membership-created event causes exactly one default menu response.
- WebChat DirectLine conversation creation still causes the default menu response.
- Telegram `/start` still causes the default menu response.
- Replaying the same lifecycle webhook does not send duplicate menus.
