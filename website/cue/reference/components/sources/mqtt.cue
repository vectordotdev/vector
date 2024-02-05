package metadata

components: sources: mqtt: {
	title: "MQTT"

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.mqtt
				interface: {
					socket: {
						api: {
							title: "MQTT protocol"
							url:   urls.mqtt_protocol
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
		delivery:      "best_effort"
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
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: base.components.sources.mqtt.configuration

	installation: {
		platform_name: null
	}

	output: logs: record: {
		description: "An individual MQTT message."
		fields: {
			message: {
				description: "The raw line from the MQTT message."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					syntax: "literal"
				}
			}
			timestamp: fields._current_timestamp & {
				description: "The current time when the message has been received."
			}
			topic: {
				description: "The MQTT topic that the message came from."
				required:    true
				type: string: {
					examples: ["topic/logs"]
					syntax: "literal"
				}
			}
		}
	}

	how_it_works: components._amqp.how_it_works

	telemetry: metrics: {
		open_connections:                 components.sources.internal_metrics.output.metrics.open_connections
		connection_shutdown_total:        components.sources.internal_metrics.output.metrics.connection_shutdown_total
		connection_errors_total:          components.sources.internal_metrics.output.metrics.connection_errors_total
		events_in_total:                  components.sources.internal_metrics.output.metrics.events_in_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
	}
}
