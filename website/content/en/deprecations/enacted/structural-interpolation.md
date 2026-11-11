---
what: "Environment-variable and secret placeholders in non-string positions"
deprecated_since: "0.57.0"
removed_in: "0.62.0"
---

As of 0.62.0, Vector parses configuration files before substituting `${VAR}` and
`SECRET[backend.key]` references. Substituted values cannot change the configuration
structure.

Quote placeholders in TOML and JSON values and in YAML flow collections. Use literal
map keys, component names, and table headers. Replace placeholders that expanded into
configuration fragments or multiple array elements with explicit configuration.
String substitutions, including values converted to numeric or boolean fields, remain
supported.

Environment-variable interpolation remains disabled by default. Enable it with
`--dangerously-allow-env-var-interpolation` or
`VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`.

See the [environment-variable documentation](/docs/reference/environment_variables/)
and [secrets documentation](/docs/reference/configuration/secrets/) for migration guidance.
