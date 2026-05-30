package metadata

components: sinks: mqtt: {
	title: "MQTT"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      false
			}
			to: {
				service: services.mqtt
				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["tcp"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.sinks.mqtt.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: true
	}

	how_it_works: {
		protocol_versions: {
			title: "MQTT 3.1.1 and 5.0 support"
			body: """
				The sink supports both MQTT 3.1.1 (default) and MQTT 5.0, selected via the
				`protocol_version` configuration option. MQTT 5.0 unlocks additional features
				such as message expiry, topic aliases, response topics, correlation data,
				content type, payload format indicators, and user properties.
				"""
		}
		publish_properties: {
			title: "MQTT v5 publish properties"
			body: """
				When `protocol_version` is set to `v5`, the sink can attach MQTT 5.0 publish
				properties to outgoing messages via the `publish_properties` section. Supported
				properties include the payload format indicator, message expiry interval, topic
				alias, response topic, correlation data, content type, and user properties.
				These properties are ignored when `protocol_version` is `v311`.
				"""
		}
		data_transport: {
			title: "Transporting logs, metrics, and traces"
			body: """
				The sink accepts logs, metrics, and traces. The set of event types actually
				accepted is derived from the chosen encoding codec and validated at
				configuration load time, so misrouted events are rejected with a clear error
				instead of silently dropped.

				Codec compatibility:

				- `json`, `native_json`, `native`: accept logs, metrics, and traces. Use
				  `native` or `native_json` for lossless Vector-to-Vector pipelines where
				  receivers should reconstruct the original event types.
				- `text`: accepts logs and metrics; traces are rejected at config time.
				- `gelf`, `logfmt`, `syslog`, `raw_message`: logs only.
				- `avro`, `cef`, `csv`, `protobuf`: depend on the user-supplied schema.

				The QoS, retain flag, and (for v5) `publish_properties` apply to every
				outgoing message regardless of event type.
				"""
		}
	}

	telemetry: metrics: {
		open_connections:                     components.sources.internal_metrics.output.metrics.open_connections
		connection_shutdown_total:            components.sources.internal_metrics.output.metrics.connection_shutdown_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_events_count:      components.sources.internal_metrics.output.metrics.component_received_events_count
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		component_sent_bytes_total:           components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:          components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total:     components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
	}
}
