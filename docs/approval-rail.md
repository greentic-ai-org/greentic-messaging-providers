# Approval rail delivery

Human-in-the-loop approvals travel on two NATS subjects owned by
greentic-designer:

| Direction | Subject |
|---|---|
| Request | `greentic.approval.request.v1` |
| Response | `greentic.approval.response.v1` |

The authoritative contract is `greentic-designer/docs/approval-rail-contract-v2.md`.
Its two conformance fixtures are vendored here at
`tests/fixtures/approval_rail/` and asserted against directly, so a drift in the
designer's payload fails a test in this repo rather than in production.

## What this repo owns, and what it does not

This repo produces WASM provider components. A component has exactly two host
imports — `http-client` and `secrets-store` — so **it cannot subscribe to or
publish on NATS**, and nothing here does. Per §8 of the contract this repo owes
one thing: render the approve/reject affordance for a channel, and parse that
channel's interactive payload back into a response body. Carrying those bodies
on and off the rail is `greentic-start`'s job.

Concretely, the Slack provider exposes:

- **`approval_request`** (op) — a `greentic.approval.request.v1` body in, a
  Slack API call out, in the same `ProviderPayloadV1` shape `encode` produces,
  so the existing `send_payload` step executes it unchanged.
- **`ingest_http`** — a `block_actions` payload whose `action_id` is
  `greentic_approval_approve` / `_deny` is answered with the
  `greentic.approval.response.v1` body on the emitted envelope's
  `extensions["greentic.approval.response"]`:

  ```json
  {
    "subject": "greentic.approval.response.v1",
    "headers": {"Greentic-Correlation-Id": "default::run=RUN-1::node=gate"},
    "body": { "target": "…", "operation": "response", "output": { … } }
  }
  ```

  The caller publishes `body` on `subject` with `headers`. Nothing else is
  required of it.

### `approval_request` input

```json
{
  "correlation_id": "default::run=RUN-1::node=gate",
  "request": { <the request.v1 body verbatim> },
  "channel": "C123",
  "message_ts": "1700000000.000100"
}
```

`correlation_id` falls back to `request.target`; `channel` falls back to `to`
and then to the provider's `default_channel`.

`message_ts` is what makes a republish an **update**. Quorum
(`min_approvals ≥ 2`) makes the designer republish the same correlation id with
a fresh token every time a vote lands, and a second `chat.postMessage` would
read as a second approval. Supplying the ts of the first delivery switches the
call to `chat.update`, so one gate stays one message. **The caller keeps the
`correlation_id → ts` map** — a WASM component holds no state between
invocations. The delivered message also carries Slack message metadata
(`event_type: "greentic_approval"`, `event_payload.correlation_id`) so a click
can be tied back to its gate without that map.

## Token handling

The `decision_token` is a bearer credential. In this repo it lives in exactly
one place on the wire: the **`value` of the two buttons** — the opaque
per-message state Slack already provides.

- It is never placed in a URL. Query strings and path segments land in proxy
  access logs, browser history and `Referer` headers.
- It is never logged. `DecisionToken` (`provider-common::approval`) has a
  redacted `Debug`, no `Display`, and one greppable accessor, `expose()`.
- It is not in the notification `text`, not in Slack message metadata, and not
  in the envelope metadata the click produces — that last one matters because
  the generic `block_actions` path forwards every action-value key into
  metadata, which is exactly how a token reaches telemetry. Approval clicks are
  routed *before* that path.
- The click's HTTP response body is an acknowledgement message carrying
  `replace_original: true`, so the request message — and the token in its
  buttons — is replaced as soon as somebody decides. This depends on the
  ingress forwarding `HttpOutV1.body_b64` to Slack verbatim, which is how the
  URL-verification challenge already works; if a host does not, the message
  simply is not updated and nothing else changes.

**Residual exposure, stated rather than implied:** while the request is
outstanding, the button state is readable by any member of the Slack
conversation it was delivered to, through `conversations.history`. Slack offers
no per-message secret for a posted message — `private_metadata` is a modal
(view) field, not a message one. Deliver approvals to a conversation whose
members you are willing to have hold the token: a DM to the approver, or a
private channel of approvers. This is a delivery-target decision, not something
the component can enforce.

`input.title` is author-supplied and never trusted as markup: it renders in
`plain_text` blocks, and in the fallback `text` field — which Slack *does*
parse as mrkdwn — it is escaped, so a title cannot smuggle a `<!channel>` ping
into the notification.

## Shape rules that have a wrong reading

- `tier.position`, not `tier.level`, is what the token binds to. `level` is the
  policy author's display label and nothing validates it for uniqueness. The
  rendered "Tier: 1 of 2" line is derived from `position`.
- `tier.position: null` means the gate has not escalated. It is a bound value,
  never `0`, and is preserved as `null` — a gate with no position renders no
  tier line rather than claiming the first one.
- `tier.deadline_ms: null` means this tier never escalates; it renders as
  "Escalates after: never".
- The whole `routing` block may be absent, and when present may carry
  `decision_token` alone. Both render a usable generic affordance. A request
  with no token at all is a gate parked before tokens shipped — it still
  renders, and its response omits `decision_token` rather than inventing one.
- Unknown keys are ignored in both directions, and each field is read
  independently, so the reserved per-approver token shape (`approvers` becoming
  an array) degrades that one field instead of failing the delivery.
- `channels` is advisory. The designer neither enforces nor verifies it.

## `resolved_by` is a claim

The token authenticates the **sender**, not the person named in `resolved_by`.
Nothing rendered here says otherwise: the acknowledgement names the Slack user
who actually clicked (`<@U123>`, an identity Slack asserts), never the email in
the response body.

Slack hands us a workspace user id, and the rail wants an email. The provider
reads `user.profile.email` from the interaction, then falls back to a
`users.info` lookup (which needs the `users:read.email` scope). If neither
resolves, the response is sent with `resolved_by: null` and a policy-governed
gate refuses it as `no_claimed_identity` — a refused vote is a better outcome
than a guessed identity.

## Operator requirement — subject-level read authorization

From §6 of the contract, and this repo cannot enforce it:

> The token travels in plaintext in the body of a message on a shared subject.
> Anyone who can subscribe to `greentic.approval.request.v1` can read a token
> out of it and approve the gates they can see.

Per-tenant subject-level read authorization on that subject is **mandatory**
wherever this rail is live, and publish permission on the response subject
should be scoped the same way. `routing.approvers.emails` puts tenant member
addresses on the same subject, which is a second, independent reason. Do not
fan either subject out to a general-purpose bus tap, an archive, or a debug
consumer that logs payloads.

## Channels covered

**Slack only.** Telegram is the obvious next channel and does not fall out
cheaply: `callback_data` is capped at **64 bytes**, which cannot hold a
43-character token plus a correlation id, so Telegram needs a server-side
handle → state map that no other channel requires. Teams and WebChat render
Adaptive Cards natively and could carry the token in an `Action.Execute` `data`
object; that is a straightforward port of `ops/approval/` once someone needs it.
