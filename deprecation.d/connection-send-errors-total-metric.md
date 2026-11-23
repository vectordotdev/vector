---
what: "`connection_send_errors_total` metric"
deprecated_since: "0.62.0"
---

The `connection_send_errors_total` counter emitted by UDP sinks, such as the `socket` and `statsd` sinks
in `udp` mode, is deprecated. Use the `component_errors_total` metric instead.

Migrate any dashboards or alerts that reference:

- `connection_send_errors_total{mode="udp"}`

to:

- `component_errors_total{component_kind="sink", error_type="writer_failed", stage="sending"}`

The replacement does not include the `mode` label. Remove that selector from existing queries and keep any
component-specific selectors, such as `component_id` or `component_type`.

There is no exact one-to-one replacement. With these selectors, `component_errors_total` also counts other
sink send failures, such as UDP chunking errors and socket send errors.
