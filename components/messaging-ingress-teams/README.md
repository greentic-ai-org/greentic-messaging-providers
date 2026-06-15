# Messaging Ingress Teams Component

Ingress and subscription lifecycle component for Microsoft Teams.

## Component ID
- `messaging-ingress-teams`

## Provider types
- `messaging.teams.graph`

## Setup Contract

Teams Graph setup persists
`team_id`, `team_name`, `channel_id`, and `channel_name`; ingress and egress
must use the IDs for Graph resource paths. Display names are labels only.

This component handles Microsoft Graph change notifications, subscription
lifecycle, and Bot Framework-compatible Teams message/invoke activities for the
answer-generated `messaging-teams` pack.

## Subscription Desired State

The Teams pack declares `messaging.subscriptions.v1.inline.desired_state` so
hosts can build subscription state without hardcoding Teams Graph resource
formats. The metadata names setup-answer source keys (`team_id`, `channel_id`,
`chat_id`), the default `change_type`, notification URL template, expiration
policy, and resource templates for channel and chat messages.

The same extension declares `component_config.include` separately. Hosts should
pass only the listed auth/client fields to `sync-subscriptions`; identifiers
such as `team_id` and `channel_id` are desired-state template inputs, not
subscription component config.

Graph subscription creation still requires a host-supplied
`expiration_datetime`; the pack metadata declares this requirement instead of
embedding a fixed timestamp.

## Secrets
- `MS_GRAPH_REFRESH_TOKEN` (tenant): Refresh token used for Graph token acquisition when configured.
