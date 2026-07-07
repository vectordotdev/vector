---
what: "Nested `protocol.*` configuration format for the `opentelemetry` sink"
deprecated_since: "0.57.0"
---

The nested `protocol.*` configuration format for the `opentelemetry` sink is deprecated. The legacy
format is still accepted temporarily but logs a warning on startup and will be removed in a future
release.

Migrate to the flat format by moving all fields from `protocol.*` to the top level and replacing
`protocol.type` with `protocol`.
