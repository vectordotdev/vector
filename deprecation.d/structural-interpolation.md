---
what: "Environment-variable and secret placeholders in non-string positions"
deprecated_since: "0.57.0"
---

Placeholders (`${VAR}` and `SECRET[backend.key]`) in structural positions of
a Vector configuration file are deprecated and will be removed in a future
release. Current behavior is unchanged.

Affected patterns:

- Unquoted placeholders in non-string fields (`count = ${MY_COUNT}`).
- Placeholders in map keys (`${KEY} = value`).
- Placeholders in TOML table headers (`[${SECTION}]`).
- Placeholders as inline array elements that expand to multiple values
  (`inputs = [${VECTOR_INPUTS}]` where `VECTOR_INPUTS=a,b`).

Migration guidance will accompany the removal PR. `envsubst` can be used today
to pre-expand env-var placeholders into a fully static config as a workaround.

See [RFC: parse-first config interpolation](https://github.com/vectordotdev/vector/blob/master/rfcs/2026-06-09-parse-first-config-interpolation.md)
for the full rationale.
