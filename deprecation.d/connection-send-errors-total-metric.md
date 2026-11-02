---
what: "`connection_send_errors_total` metric"
deprecated_since: "0.62.0"
---

The `connection_send_errors_total` counter emitted by socket-based sources is deprecated. Use the
`component_errors_total` metric instead.

Migrate any dashboards or alerts that reference:

- `connection_send_errors_total`

to:

- `component_errors_total`
