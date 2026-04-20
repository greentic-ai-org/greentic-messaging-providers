# greentic-messaging-tester

Minimal CLI for driving a provider WASM component through the render/encode/send and ingest_http paths without depending on the full runtime.

## Commands

- `requirements --provider <name>`: prints `providers/<name>.requirements.json`.
- `send --provider <name> --values values.json (--text "..." | --card path/to/card.json)`: renders a plan, encodes it, performs `send_payload`, and prints the plan/encode/http_calls/result payload.
- `ingress --provider <name> --values values.json --http-in webhook.json --public-base-url https://example.com`: calls `ingest_http` with the supplied HTTP payload and prints the normalized envelopes.

## Sample `values.json`

```json
{
  "config": {
    "api_base": "https://api.telegram.org"
  },
  "secrets": {
    "TELEGRAM_BOT_TOKEN": "dummy-token"
  },
  "to": {
    "chat_id": "123456789",
    "channel": "telegram-cli"
  },
  "http": "mock",
  "state": {}
}
```

## Sending a text message

```bash
greentic-messaging-tester send \
  --provider telegram \
  --values values.json \
  --text "hello from the tester"
```

The command prints a JSON object containing the render plan, encode output, captured HTTP calls, and the final `send_payload` result.

When you point `send` at `--provider webchat`, your `values.json` should still include a routing key (either `route` or `tenant_channel_id`), `config.public_base_url` (the full callback URL the component will register), and the `jwt_signing_key` secret so Direct Line tokens can be signed. The provider no longer reads a `mode` value or mutates the base URL, it simply stores and surfaces what you pass.

## Sending an adaptive card

- Create a card file (e.g., `card.json`) with your adaptive card payload.
- Run:
  ```bash
  greentic-messaging-tester send \
    --provider telegram \
    --values values.json \
    --card card.json
  ```

The adaptive card JSON is stored in the envelope metadata before invoking the provider.

## Ingress replay

Create `http_in.json` matching the shape below, then replay the webhook:

```json
{
  "method": "POST",
  "path": "/telegram/webhook",
  "headers": {
    "content-type": "application/json"
  },
  "body": "{\"message\":{\"text\":\"hello\"}}"
}
```

```bash
greentic-messaging-tester ingress \
  --provider telegram \
  --values values.json \
  --http-in http_in.json
  --public-base-url https://example.com
```

The command prints the normalized envelopes emitted by `ingest_http`.

## Listening for webhook calls

`listen` lets you watch the outbound HTTP requests a provider sends back to the channel and also helps you produce `http_in` fixtures for the `ingress` command:

```bash
greentic-messaging-tester listen \
  --provider <name> \
  --values values.json \
  --host 127.0.0.1 \
  --port 8080 \
  --path /webhook \
  --public-base-url https://example.com
```

The command opens a local HTTP listener and writes a JSON blob for each incoming request (method, path, headers, body, etc.) so you can verify what the provider is calling or forward those details to other tooling. It keeps running until you hit `Ctrl+C`.

The `--public-base-url` argument is forwarded to the provider so the pack can derive its webhook endpoint without you needing to bake that value into `values.json`.
When testing `messaging.webchat`, set `--path` to the exact path portion of the `public_base_url` you plan to expose—the component does not append `/webhooks/webchat`, it respects the callback URL you supply.

If you prefer to generate the `http_in` payload without running the server, add `--http-in some.json`. You can tweak the simulated request with `--method`/`--path`/`--query`, add headers via repeated `--header "name:value"`, and provide a body with `--body` or `--body-file`. That JSON can then be fed into `greentic-messaging-tester ingress --http-in some.json`.

The typical workflow is to:

1. Run `listen --http-in webhook.json --body '{"text":"hi"}' --path /webchat` to capture the incoming webhook shape.
2. Start `listen` again (without `--http-in`) so it begins logging outbound calls.
3. In another terminal, run `greentic-messaging-tester ingress --http-in webhook.json ...` and watch the `listen` terminal to confirm the provider invoked the webhook.

## Webhook reconciliation

The standalone `telegram-webhook` / `webex-webhook` components were removed in 0.5. The equivalent operation now lives on the provider itself as `setup_webhook` (see `messaging-provider-telegram/src/ops/webhook.rs`). Invoke it through the provider's normal op dispatch rather than a dedicated sub-component.

## Building

```bash
cargo build -p greentic-messaging-tester
```
