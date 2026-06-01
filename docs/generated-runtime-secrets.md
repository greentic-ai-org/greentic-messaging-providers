# Generated Runtime Secrets

Provider packs can declare secrets that are required at runtime but should not be
entered by users during setup. Hosts use this metadata to seed missing secrets
generically before invoking provider components.

Generated secrets are declared in `secret_requirements` with a `generated`
object:

```json
{
  "name": "jwt_signing_key",
  "aliases": ["JWT_SIGNING_KEY"],
  "required": true,
  "scope": "tenant",
  "description": "Secret key used for Direct Line JWT token signing and verification.",
  "generated": {
    "policy": "random",
    "length": 20,
    "encoding": "raw_text",
    "scope": {
      "level": "tenant",
      "team": "_"
    },
    "regenerate_if_present": false
  }
}
```

Fields:

- `name`: canonical secret key used by provider metadata.
- `aliases`: equivalent environment-style keys accepted by hosts or provider
  secret lookup.
- `required`: the secret must exist before runtime operations that read it.
- `scope`: coarse requirement scope for existing pack tooling.
- `generated.policy`: generation strategy. `random` means the host may create a
  missing secret with cryptographically random bytes/text.
- `generated.length`: output length for text encodings.
- `generated.encoding`: generated value encoding, for example `raw_text`,
  `base64url`, or `hex`.
- `generated.scope`: precise storage scope. `level=tenant` and `team=_` means a
  tenant-wide provider secret, not a team-scoped value.
- `generated.regenerate_if_present`: defaults to false. Hosts must not rotate an
  existing value unless a future policy explicitly allows it.

Generated secrets must not be exposed as setup questions, public config fields,
or setup answers. Providers may emit them in `secrets_patch` during setup for new
bundles, but startup hosts should also be able to seed missing values from this
metadata alone for existing bundles.
