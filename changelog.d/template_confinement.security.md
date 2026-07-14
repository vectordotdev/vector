Vector now prevents log producers from steering event data to unintended
destinations. Sinks that use event fields in routing templates (S3 keys,
Kafka topics, file paths, HTTP URIs, etc.) now validate that the rendered
value stays within the operator-configured prefix, and events that violate
that boundary are dropped or handled by the sink's fallback path.

Two internal metrics track this:

- `component_errors_total{error_type="condition_failed"}` — increments on every confinement violation. Use this to alert on attempted routing injection.
- `vector_security_confinement_disabled` — set to `1` for any sink running with `dangerously_allow_unconfined_template_resolution: true`. Use this to alert on any sink whose confinement policy is disabled.

An `ERROR` log line accompanies each violation with sink-specific detail.

authors: pront
