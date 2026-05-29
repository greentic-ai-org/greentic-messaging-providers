# Messaging Ingress Teams Component

Ingress and subscription lifecycle component for Microsoft Teams.

## Component ID
- `messaging-ingress-teams`

## Provider types
- `messaging.teams.bot`
- Graph subscription inputs are also supported for channel message resources
  when `team_id` and `channel_id` are present.

## Setup Contract

Teams setup is Graph-first for outbound channel messaging. That path persists
`team_id`, `team_name`, `channel_id`, and `channel_name`; ingress and egress
must use the IDs for Graph resource paths. Display names are labels only.

Bot Framework ingress remains supported separately through `ms_bot_app_id`,
`ms_bot_app_password`, `bot_display_name`, and `messaging_endpoint`. Those
fields describe the Azure Bot and Teams app manifest endpoint; they do not
replace Graph `team_id`/`channel_id` values for Graph channel subscriptions.

## Subscription Desired State

The Teams pack declares `messaging.subscriptions.v1.inline.desired_state` so
hosts can build subscription state without hardcoding Teams Graph resource
formats. The metadata names setup-answer source keys (`team_id`, `channel_id`,
`chat_id`), the default `change_type`, notification URL template, expiration
policy, and resource templates for channel and chat messages.

Graph subscription creation still requires a host-supplied
`expiration_datetime`; the pack metadata declares this requirement instead of
embedding a fixed timestamp.

## Secrets
- `MS_GRAPH_REFRESH_TOKEN` (tenant): Refresh token used for Graph token acquisition when configured.
