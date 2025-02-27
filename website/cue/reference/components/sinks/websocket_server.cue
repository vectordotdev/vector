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
		has_auth:         true
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

	configuration: base.components.sinks.websocket_server.configuration

	how_it_works: {
		simple_configuration: {
			title: "Example configuration"
			body: """
				The `websocket_server` sink component can be useful when data needs to be broadcasted to
				a number of clients. Here is an example of a very simple websocket server sink
				configuration:

				```yaml
				sources:
					demo_logs_test:
						type: "demo_logs"
						format: "json"

				sinks:
					websocket_sink:
						inputs: ["demo_logs_test"]
						type: "websocket_listener"
						address: "0.0.0.0:1234"
						auth:
							username: "test"
							password: "test"
						encoding:
							codec: "json"
				```

				With this configuration, a websocket server will listen for connections on 1234 port.
				For clients to connect, they need to provide credentials in the Authorization header,
				because `auth` configuration is defined (by default, no auth is required). In this case,
				it requires Basic auth with defined username and password.
				"""
		}
		message_buffering: {
			title: "Message buffering"
			body: """
				The `message_buffering` configuration option enables event buffering. It defines the number
				of events to buffer, allowing clients to replay messages after a disconnection.
				To provide clients with the message ID, define `message_buffering.message_id_path`.
				This encodes the outgoing messages ID. The buffer is backed by a ring buffer, so
				the oldest messages are discarded when the size limit is reached.

				After clients have the ID, they can send it in the `last_received` query parameter on future connections.
				All buffered messages since that ID are then sent to the client immediately upon connection.
				If the message can't be found, the entire buffer is replayed.

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
