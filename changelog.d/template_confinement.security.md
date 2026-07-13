Vector now prevents log producers from steering event data to unintended
destinations. Sinks that use event fields in routing templates (S3 keys,
Kafka topics, file paths, HTTP URIs, etc.) now validate that the rendered
value stays within the operator-configured prefix. Events that escape that
boundary are dropped instead of forwarded to the wrong destination.

**What to watch when this triggers:**

- `component_discarded_events_total`: increments for each dropped event (intentional discard).
- `component_errors_total`: increments alongside every drop, filterable by `error_type="condition_failed"`.
- An `ERROR` log line is emitted per dropped event: `"Rendered key is outside the configured base prefix; dropping event."` with the offending key attached.
- `vector_security_confinement_disabled` gauge: set to `1` (with `component_type` and `field` labels) whenever a sink is running with `dangerously_allow_unconfined_template_resolution: true`. Use this to alert on any sink that has opted out of confinement.

authors: pront
