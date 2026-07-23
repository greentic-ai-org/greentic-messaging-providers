# Events Ingress GitHub Component

Events-domain ingress component. Turns an inbound GitHub `push` webhook into a
`github.push` event that triggers a flow in the "Greentic Actions" pipeline.

## Component ID
- `events-ingress-github`

## Domain
- `events` (emits event envelopes, not messaging envelopes)

## Behaviour
- Reads GitHub webhook headers: `X-Hub-Signature-256`, `X-GitHub-Event`, `X-GitHub-Delivery`.
- When `GITHUB_WEBHOOK_SECRET` is set, verifies the `X-Hub-Signature-256` HMAC-SHA256
  over the raw request body (constant-time compare). Rejects on mismatch.
- Only `push` deliveries are converted. `ping` and any other event type are
  acknowledged with an empty event list (HTTP 200) and dropped.
- Emits one event: `event_type = "github.push"` with
  `payload = { repo, ref, commits, head_commit }`.

## Output contract
Returns `{ "ok": true, "status": 200, "events": [ EventEnvelopeV1 ] }`. The event
shape mirrors `greentic-start` `src/ingress_types.rs::EventEnvelopeV1`, which is
what `ingress_dispatch.rs::parse_events` deserializes. `scope.tenant` is emitted
as `"default"`; the effective tenant/team is resolved from the HTTP route by
greentic-start's `event_router` (the ingress ABI only forwards headers + body,
so the component cannot see the route tenant).

## Secrets
- `GITHUB_WEBHOOK_SECRET` (tenant): GitHub webhook secret used to verify webhook
  signatures. Optional — when absent, signature verification is skipped.
