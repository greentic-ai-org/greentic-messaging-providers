# messaging-teams source

This directory owns the Rust/WASM source for the Bot Framework-compatible Teams
provider.

PR-02 only establishes the generated-pack source layout. PR-03 will move the
runtime implementation here and wire the generated `messaging-teams` pack to the
component produced from this source.
