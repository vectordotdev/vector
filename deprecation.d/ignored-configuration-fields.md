---
what: "Ignored configuration fields"
deprecated_since: "0.61.0"
---

<a id="ignored-configuration-fields"></a>

Configuration fields that Vector accepts but discards during deserialization are deprecated.
Vector now warns about these fields when loading configuration, including during `vector validate`.
Existing configurations continue to work.

Correct misspelled fields or remove fields that do not belong to the selected enum variant.
For example, replace this HTTP sink request setting:

```yaml
request:
  retry_atempts: 3
```

with:

```yaml
request:
  retry_attempts: 3
```

For options that select an enum variant, keep only the fields used by that variant. For example,
HTTP authentication with `strategy: bearer` uses `token`. Remove `username` and `password`, which
belong to the `basic` variant:

```yaml
auth:
  strategy: bearer
  token: example-token
```

Warnings identify field paths without including their values. Maps that accept arbitrary keys,
such as HTTP headers, remain supported. Recognized settings documented as inactive in a particular
mode, such as `value_limit` with `mode: excluded` in `tag_cardinality_limit`, are not affected by this
deprecation.
