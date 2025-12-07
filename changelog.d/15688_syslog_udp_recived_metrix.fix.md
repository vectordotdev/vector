The `syslog` source in UDP mode now emits the standard "received" metrics, aligning behavior with TCP and the Component Specification:

- `component_received_events_total`
- `component_received_event_bytes_total`
- `component_received_bytes_total`

This makes internal telemetry consistent and restores compliance checks for UDP syslog.

authors: sghall


test123