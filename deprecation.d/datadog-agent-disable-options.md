---
what: "Datadog Agent source `disable_*` configuration options"
deprecated_since: "0.62.0"
---

<a id="datadog_agent-disable_options"></a>

The `datadog_agent` source's `disable_logs`, `disable_metrics`, `disable_traces`, and `disable_llmobs` options are deprecated.
Switch to `enabled_data_types` and list the data types the source accepts.

For example, replace:

```yaml
disable_metrics: true
disable_traces: true
disable_llmobs: true
```

with:

```yaml
enabled_data_types: ["logs"]
```

Existing settings still work, but produce a deprecation warning.

When migrating:

- Leave out `enabled_data_types` to accept all data types except `apm_stats`.
- Use the data type names `logs`, `metrics`, `traces`, and `llmobs`. When `multiple_outputs` is enabled,
  each data type is sent to the output of the same name.
- Remove all `disable_*` options when adding `enabled_data_types`. If both are set, data types disabled by
  `disable_*` are removed from `enabled_data_types`.
