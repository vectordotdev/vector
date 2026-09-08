Configurations carrying the removed `api.playground` or `api.graphql` fields no longer fail to load. Both options were dropped in 0.55.0 along with the GraphQL observability API; setting them now is accepted at deserialize time but logs a deprecation warning at startup, and the values are otherwise ignored. This makes upgrades from 0.54.0 and earlier non-breaking when those fields are still present in pinned configs.

The deprecation follows a graduated policy: the field is accepted with a `warn!` at startup for `WARN_WINDOW_MINORS = 12` minor releases after the version in which it was removed (about a year at Vector's monthly cadence), and the warning text names the exact future version in which the field becomes a hard configuration error. Once that future version ships, an attempt to load a config that still sets the field fails fast with a clear "remove this field" message — no silent breaking changes. Operators have a deterministic deadline for cleanup.

The policy lives in a new reusable module, `src/config/deprecation`, so future field removals can opt into the same behavior with a single call site rather than reimplementing per-field warnings.

Remove `api.playground` / `api.graphql` from your configuration to silence the warning — they have no runtime effect.

authors: joshcoughlan
