---
what: "Ignored configuration fields"
deprecated_since: "0.61.0"
---

<a id="ignored-configuration-fields"></a>

Configuration fields that Vector silently ignores are deprecated. Vector now warns about these
fields when loading configuration, including during `vector validate`. Existing configurations
continue to work. We will give users sufficient time to migrate, including at least one minor
release after this announcement, before rejecting ignored fields.

Correct misspelled fields or remove fields that do not apply to the selected configuration.
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

For HTTP authentication, keep only the fields used by the selected strategy:

- `basic` (also the default when `strategy` is omitted): `username` and `password`.
- `bearer`: `token`.
- `custom`: `source`.

Warnings identify field paths without including their values. Maps that accept arbitrary keys,
such as HTTP headers, remain supported.
