Vector now parses configuration files before performing environment-variable and `SECRET[...]` substitution. Interpolation operates only on parsed string values, so an unquoted `${VAR}` or `SECRET[...]` in a non-string position of a TOML or JSON config is no longer valid syntax and will cause the config to fail to load.

YAML configurations are unaffected -- YAML parses `${VAR}` as a string scalar and the new schema-coercion pass converts the resulting string to the declared scalar type at load time.

Migration: in TOML or JSON, wrap the placeholder in quotes so the parser sees a string scalar. The new loader will coerce the value to the declared type (integer, boolean, float) automatically.

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

The same applies to `SECRET[backend.key]` references in non-string fields. Note that the quoted form was previously rejected with a serde type error; both quoted and unquoted forms are now consistent across all three formats, with the quoted form as the supported syntax.

authors: pront
