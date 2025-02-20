package metadata

components: sinks: websocket_server: {
	_port: 8080
	title: "WebSocket server"

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
		auto_generated:   true
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
				enabled_by_scheme:      true
			}
			to: {
				service: services.websocket_client
				interface: {
					socket: {
						direction: "incoming"
						protocols: ["tcp"]
						ssl:  "optional"
						port: _port
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

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			summary:      true
			set:          true
		}
		traces: true
	}

	telemetry: metrics: {
		active_clients:               components.sources.internal_metrics.output.metrics.active_clients
		open_connections:             components.sources.internal_metrics.output.metrics.open_connections
		connection_established_total: components.sources.internal_metrics.output.metrics.connection_established_total
		connection_shutdown_total:    components.sources.internal_metrics.output.metrics.connection_shutdown_total
	}

	how_it_works: {
		message_buffering: {
			title: "Message buffering"
			body: """
				The `message_buffering` configuration option can be used to enable this feature. It can
				be used to define a number of events to be buffered, to enable replay for clients that
				may want to continue from the last message after disconnection. To provide clients with the
				message ID, `message_buffering.message_id_path` needs to be defined, which is used
				to encode the ID inside outgoing messages. The buffer is backed by a ring buffer, so
				oldest messages will be lost when the size limit is reached.

				Once clients have the ID, on future connections that ID can be sent in the
				`last_received` query parameter and all buffered messages since that message are
				sent to the client immediately on connection. If the message can't be found, the entire
				buffer will be replayed.

				Example config:
				```yaml
				sinks:
					websocket_export:
						type: websocket_server
						inputs: ["demo_logs_test"]
						address: "0.0.0.0:1234"
							message_buffering:
								max_events: 1000
								message_id_path: "message_id"
						encoding:
							codec: "json"
				```
				"""
		}
	}
}
