Expanded MQTT support across the `mqtt` source and `mqtt` sink:

- Added MQTT v5 protocol support alongside the existing v3.1.1 implementation, selectable via a new `protocol_version` option (defaults to `v311` for backward compatibility).
- The sink exposes v5 publish properties (payload format indicator, message expiry, topic alias, response topic, correlation data, content type, user properties) under a new `publish_properties` config section.
- The source surfaces incoming v5 properties as event metadata (content type, response topic, correlation data, payload format indicator, message expiry interval, user properties, protocol version).
- Added end-to-end acknowledgements to the source. When enabled, the source switches the MQTT client to manual ack mode and only sends `PubAck` to the broker once Vector has successfully delivered the event downstream, providing at-least-once delivery semantics for both v3.1.1 and v5.
- The sink now accepts the data types its configured encoder supports, so logs, metrics, and traces can all be transported over MQTT (with the default `json` codec, all three types work; use `native_json` for lossless Vector-to-Vector pipelines).
- Source and sink now emit `open_connections` and `connection_shutdown_total` telemetry as their connections open and close, plus richer `component_errors_total` coverage for connection, subscribe, and acknowledgement failures.

authors: vitalvas
