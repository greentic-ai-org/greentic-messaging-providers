# Generated Runtime Secrets

Provider packs can declare secrets that are required at runtime but should not be
entered by users during setup. Hosts use this metadata to seed missing secrets
generically before invoking provider components.

Generated secrets are declared in the pack extension
`greentic.generated-secrets.v1`, not in regular `secret_requirements`.
Regular secret requirements are treated as user-provided setup inputs by older
hosts, so generated runtime secrets must stay out of that list.

```json
{
  "key": "jwt_signing_key",
  "aliases": ["JWT_SIGNING_KEY"],
  "required": true,
  "policy": "random",
  "length": 20,
  "encoding": "raw_text",
  "scope": {
    "level": "tenant",
    "team": "_"
  },
  "regenerate_if_present": false,
  "description": "Secret key used for Direct Line JWT token signing and verification."
}
```

Fields:

- `key`: canonical secret key used by provider metadata.
- `aliases`: equivalent environment-style keys accepted by hosts or provider
  secret lookup.
- `required`: the secret must exist before runtime operations that read it.
- `policy`: generation strategy. `random` means the host may create a
  missing secret with cryptographically random bytes/text.
- `length`: output length for text encodings.
- `encoding`: generated value encoding, for example `raw_text`, `base64url`,
  or `hex`.
- `scope`: precise storage scope. `level=tenant` and `team=_` means a
  tenant-wide provider secret, not a team-scoped value.
- `regenerate_if_present`: defaults to false. Hosts must not rotate an
  existing value unless a future policy explicitly allows it.

Generated secrets must not be exposed as setup questions, public config fields,
or setup answers. Providers may emit them in `secrets_patch` during setup for new
bundles, but startup hosts should also be able to seed missing values from this
metadata alone for existing bundles.

Slack is intentionally not part of this generated-secret contract:
`SLACK_SIGNING_SECRET` is returned by Slack app registration and stored by the
Slack provider as an external secret. Hosts must not generate a replacement
signing secret.
