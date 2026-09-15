# Config is now parsed before environment variable and secret interpolation

## Summary

Vector now parses configuration files into a typed value tree before performing
environment variable (`${VAR}`) and `SECRET[...]` substitution. Interpolation operates
only on string-typed leaves in the parsed tree. An unquoted placeholder in a non-string
position of a TOML or JSON config (e.g. `count = ${MY_COUNT}`) is no longer valid and
will cause the config to fail to load.

YAML configurations are unaffected: YAML parses `${VAR}` as a string scalar, and the
new schema-coercion pass converts that string to the declared type at load time.

## Migration

In TOML or JSON, wrap any placeholder that appears in a non-string field in quotes so
the parser sees a string scalar. Vector will coerce the value to the declared type
(integer, boolean, float) automatically.

Before (TOML):

```toml
[sources.in]
type = "demo_logs"
count = ${MY_COUNT}
```

After (TOML):

```toml
[sources.in]
type = "demo_logs"
count = "${MY_COUNT}"
```

Before (JSON):

```json
{ "sources": { "in": { "type": "demo_logs", "count": ${MY_COUNT} } } }
```

After (JSON):

```json
{ "sources": { "in": { "type": "demo_logs", "count": "${MY_COUNT}" } } }
```

The same applies to `SECRET[backend.key]` references in non-string fields.

authors: pront
