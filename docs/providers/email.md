# Email Provider

## What It Does

Email sends outbound email through SMTP-style configuration. It is useful for notifications, alerts, and workflows where the recipient is an email address.

## Features

- Sends email messages to configured recipients.
- Supports setup and validation of SMTP connection settings.
- Converts rich message content to email-friendly text or HTML fallback.
- Supports provider tests and pack builds without sending real mail by default.

## Setup Inputs

Common setup values include:

- SMTP host.
- SMTP port.
- SMTP username.
- SMTP password secret name.
- From address.
- Default recipient or destination settings, when used by a bundle.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `EMAIL_PASSWORD` | Yes for live send | SMTP password used when sending mail. |

Nightly e2e uses GitHub Secrets with `E2E_EMAIL_` prefixes. See [Provider nightly e2e](../provider-e2e.md).

## Message Features

- Outbound: email send.
- Inbound: not currently a primary feature.
- Adaptive Cards: converted to email-friendly text or HTML fallback.
- Attachments: provider-specific handling should be verified before relying on production attachment delivery.
- External read-back: not required; SMTP acceptance is the baseline live check.

## Owned Files

- `components/messaging-provider-email/`
- `packs/messaging-email/`
- `crates/provider-tests/tests/provider_core_email.rs`
- `crates/provider-tests/tests/universal_ops_email.rs`
- `e2e/providers/email/`

## Focused Checks

```bash
cargo test -p messaging-provider-email
cargo test -p provider-tests provider_core_email
cargo test -p provider-tests universal_ops_email
PACK_FILTER=messaging-email ./ci/steps/11_build_packs.sh
```

## Agent Notes

Do not put SMTP credentials in examples or fixtures. Prefer mock HTTP/SMTP paths in tests unless the task explicitly asks for live e2e.

