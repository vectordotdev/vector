package metadata

components: sinks: mqtt: {
	title: "MQTT"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		acknowledgements: true
		healthcheck: enabled: false
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
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		host: {
			description: """
				The MQTT broker to connect to.
				"""
			required: true
			warnings: []
			type: string: {
				examples: ["mqtt.example.com"]
				syntax: "literal"
			}
		}
		port: {
			description: """
				MQTT service port to connect to.
				"""
			required: false
			type: uint: {
				default: 1883
			}
		}
		topic: {
			description: """
				MQTT publish topic
				"""
			required: true
			type: str: {
				default: "vector"
			}
		}
		user: {
			description: """
				MQTT username
				"""
			required: false
			type: str: {}
		}
		password: {
			description: """
				MQTT password
				"""
			required: false
			type: str: {}
		}
		quality_of_service: {
			description: "Supported Quality of Service types for MQTT."
			required:    false
			type: string: {
				default: "exactly_once"
				enum: {
					at_least_once: "At least once."
					at_most_once:  "At most once."
					exactly_once:  "Exactly once."
				}
			}
		}
		client_id: {
			description: """
				MQTT client id
				"""
			required: false
			type: str: {
				default: "vector"
			}
		}
		keep_alive: {
			description: """
				MQTT keep-alive
				"""
			required: false
			type: uint: {
				default: 60
			}
		}
		clean_session: {
			description: """
				Removes all the state from queues & instructs the broker to clean all the client state after disconnect.
				"""
			required: false
			type: bool: {
				default: false
			}
		}
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

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
