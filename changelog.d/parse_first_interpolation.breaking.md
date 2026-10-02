# Configuration is parsed before interpolation

## Summary

Vector now parses configuration files before substituting environment variables and
`SECRET[backend.key]` references. Substitution applies only to string values, followed
by conversion to the field's declared type. Quotes, newlines, and other configuration
syntax in substituted values cannot add keys, components, or array elements.

This removes structural interpolation, deprecated in 0.57.0. Environment-variable
interpolation remains disabled by default and still requires
`--dangerously-allow-env-var-interpolation` or
`VECTOR_DANGEROUSLY_ALLOW_ENV_VAR_INTERPOLATION=true`.

## Migration

Configuration must be valid YAML, TOML, or JSON before substitution. Quote placeholders
in TOML and JSON values, including numeric and boolean fields. For example, change
`count = ${MY_COUNT}` to `count = "${MY_COUNT}"`. The same applies to secret references.

YAML string placeholders remain supported, including in numeric fields:

```yaml
sources:
  demo:
    type: demo_logs
    format: json
    count: "${MY_COUNT}"
```

Replace interpolated map keys, component names, and table headers with literal names.
Placeholders in keys and comments are no longer substituted or collected as secrets.
Values cannot expand into configuration fragments or multiple array elements. For
example, replace a comma-separated `${VECTOR_INPUTS}` expansion with an explicit list:

```yaml
inputs:
  - "${FIRST_INPUT}"
  - "${SECOND_INPUT}"
```

Environment-variable and secret values are now inserted as literal strings. Remove
escaping that was needed only for the outer configuration format. Multiline environment
values are supported within a string field, but cannot inject configuration blocks.
Escaping required by an embedded language, such as VRL, is still your responsibility.

For configurations that rely on structural environment-variable expansion, preprocess
the file with a tool such as `envsubst`, inspect the resulting configuration, and pass
that static file to Vector. `envsubst` does not resolve `SECRET[...]` references or
Vector's extended default/required-variable syntax.

authors: pront
