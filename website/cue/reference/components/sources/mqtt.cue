package metadata

components: sources: mqtt: {
	title: "MQTT"

	features: {
		auto_generated:   true
		acknowledgements: true
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.mqtt
				interface: {
					socket: {
						api: {
							title: "MQTT protocol"
							url:   urls.mqtt
						}
						direction: "incoming"
						port:      1883
						protocols: ["tcp"]
						ssl: "optional"
					}
				}
			}
		}
		multiline: enabled: false
	}

	classes: {
		commonly_used: false
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		stateful:      false
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

	configuration: generated.components.sources.mqtt.configuration

	installation: {
		platform_name: null
	}

	output: {
		logs: record: {
			description: "An individual MQTT message."
			fields: {
				message: {
					description: "The decoded payload of the MQTT message."
					required:    true
					type: string: {
						examples: ["{\"level\":\"info\",\"msg\":\"hello\"}"]
						syntax: "literal"
					}
				}
				timestamp: fields._current_timestamp & {
					description: "The current time when the message was received."
				}
				topic: {
					description: "The MQTT topic that the message was published to."
					required:    true
					type: string: {
						examples: ["sensors/temperature/room1"]
						syntax: "literal"
					}
				}
				protocol_version: {
					description: "MQTT protocol version of the connection that delivered the message."
					required:    true
					type: string: {
						enum: {
							v311: "MQTT 3.1.1"
							v5:   "MQTT 5.0"
						}
					}
				}
				content_type: {
					description: "MQTT v5 content type indicator (for example a MIME type) describing the payload encoding. Absent for v3.1.1 messages or v5 messages without the property set."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["application/json"]
						syntax: "literal"
					}
				}
				response_topic: {
					description: "MQTT v5 response topic used for the request/response pattern. Absent for v3.1.1 messages or v5 messages without the property set."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["responses/abc123"]
						syntax: "literal"
					}
				}
				correlation_data: {
					description: "MQTT v5 correlation data used for the request/response pattern. Stored as raw bytes. Absent for v3.1.1 messages or v5 messages without the property set."
					required:    false
					common:      false
					type: string: {
						default: null
						examples: ["abc123"]
						syntax: "literal"
					}
				}
				payload_format_indicator: {
					description: "MQTT v5 payload format indicator. 0 indicates unspecified bytes and 1 indicates UTF-8 encoded data. Absent for v3.1.1 messages or v5 messages without the property set."
					required:    false
					common:      false
					type: uint: {
						default: null
						unit:    null
						examples: [0, 1]
					}
				}
				message_expiry_interval: {
					description: "MQTT v5 message expiry interval in seconds. Absent for v3.1.1 messages or v5 messages without the property set."
					required:    false
					common:      false
					type: uint: {
						default: null
						unit:    "seconds"
					}
				}
				user_properties: {
					description: "MQTT v5 user properties as an ordered list of `{key, value}` string pairs. Absent for v3.1.1 messages or v5 messages without any user properties."
					required:    false
					common:      false
					type: array: {
						default: null
						items: type: object: {
							examples: [{"key": "source", "value": "device-42"}]
							options: {}
						}
					}
				}
			}
		}
		metrics: "": {
			description: "Metric events that may be emitted by this source when using a metric-producing codec."
		}
		traces: "": {
			description: "Trace events that may be emitted by this source when using a trace-producing codec."
		}
	}

	how_it_works: {
		rumqttc: {
			title: "rumqttc"
			body:  """
				The `mqtt` source uses [`rumqttc`](\(urls.rumqttc)) under the hood, a pure-Rust
				MQTT client supporting both MQTT 3.1.1 and MQTT 5.0.
				"""
		}
		topic_subscription: {
			title: "Topic subscriptions"
			body:  """
				The `topic` option accepts either a single topic string or a list of topics.
				All subscriptions are made at MQTT QoS 1 (AtLeastOnce), and MQTT wildcards
				are supported:

				- `+` matches exactly one topic level (for example `sensors/+/temperature`)
				- `#` matches zero or more trailing levels (for example `sensors/#`)

				Subscribing to multiple topics in a single source shares one MQTT connection
				and one decoder configuration.
				"""
		}
		acknowledgements: {
			title: "End-to-end acknowledgements"
			body:  """
				When `acknowledgements` is enabled, the source switches the MQTT client to manual
				ack mode and only sends `PubAck` to the broker once Vector has successfully
				delivered the event downstream. Messages whose downstream processing fails or
				whose process crashes before delivery are redelivered by the broker on the next
				session. This requires `clean_session: false` (the default).
				"""
		}
		data_transport: {
			title: "Transporting logs, metrics, and traces"
			body:  """
				The source can emit logs, metrics, or traces depending on the chosen decoding
				codec.

				Codec compatibility:

				- `bytes`, `gelf`, `json`, `syslog`: emit logs.
				- `influxdb`: emits metrics.
				- `otlp`: emits logs and traces.
				- `native`, `native_json`: emit any event type, reconstructing what the
				  producing Vector instance sent. Use these for lossless Vector-to-Vector
				  pipelines over MQTT.
				- `avro`, `protobuf`: emit logs (schema-dependent).

				MQTT-specific source metadata (topic, protocol version, v5 properties) is
				attached only to log events, matching Vector's framework convention for source
				metadata. For metric or trace pipelines that need the topic, include it in the
				message payload on the producer side.
				"""
		}
	}

	telemetry: metrics: {
		open_connections:                     components.sources.internal_metrics.output.metrics.open_connections
		connection_shutdown_total:            components.sources.internal_metrics.output.metrics.connection_shutdown_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		component_sent_events_total:          components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total:     components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
	}
}
