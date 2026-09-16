---
what: "host `process_runtime` counter without the `_total` suffix"
deprecated_since: "0.59.0"
---

The `host_metrics` source now also emits the process runtime counter with the `_total` suffix, matching Vector's counter naming convention:

- `process_runtime_total`

The previous name without the `_total` suffix is deprecated and will be removed in a future release. Migrate any dashboards or alerts that reference:

- `process_runtime`
