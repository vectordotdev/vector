# Lua transform now requires the `version` option {#lua-transform-version-required}

## Summary

The `lua` transform now requires the `version` option to be set to either `"1"` or `"2"`.
Previously, omitting `version` defaulted to version 1.

The transform now also rejects unknown configuration fields, as `deny_unknown_fields` was
previously bypassed by the flattened versioned configs.

This change also fixes the generated configuration documentation so that version 2-only options
such as `hooks`, `timers`, and `metric_tag_values` are correctly marked as only relevant when
`version = "2"`.

## Migration

Add `version: "1"` to any `lua` transform configuration that currently omits the `version`
option, and remove any unknown fields from `lua` transform configurations.

authors: thomasqueirozb
