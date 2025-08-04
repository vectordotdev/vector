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
		description: """
			The supported input types depend on the encoding configuration.
			This sink accepts any input type supported by the specified encoder.
			"""
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
		websocket_messages_sent_total: {
			description:       "Number of messages sent from the websocket server."
			type:              "counter"
			default_namespace: "vector"
			tags:              components.sources.internal_metrics.output.metrics._component_tags
		}
		websocket_bytes_sent_total: {
			description:       "Bytes sent from the websocket server."
			type:              "counter"
			default_namespace: "vector"
			tags:              components.sources.internal_metrics.output.metrics._component_tags
		}
	}

	configuration: generated.components.sinks.websocket_server.configuration

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
						type: "websocket_server"
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
		custom_metric_tags: {
			title: "Additional metrics tags"
			body: """
				To provide more details about connected clients, this component supports
				defining additional custom tags to attach to metrics. Additional tags are only applied
				to `connection_established_total`, `active_clients`, `component_errors_total`,
				`connection_shutdown_total`, `websocket_messages_sent_total`, and
				`websocket_bytes_sent_total`.

				Example configuration:
				```yaml
				sinks:
				  websocket_sink:
					type: "websocket_server"
					# ...
					internal_metrics:
					  extra_tags:
						test_extra_tag:
						  type: fixed
						  value: test_value
						user_auth:
						  type: header
						  name: Authorization
						client_ip:
						  type: ip_address
						full_url:
						  type: url
						last_received_query:
						  type: query
						  name: last_received
					# ...
				```

				This configuration adds a fixed tag (`test_extra_tag`) to each metric with the value,
				`test_value`. It also puts the `Authorization` header found in connection requests
				into the `user_auth` tag, IP address of the client under the `client_ip` tag, full
				connection URL under the `full_url` tag and the `last_received` query parameter under
				the `last_received_query` tag.
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
		message_buffering_ack_support: {
			title: "Message buffering ACK support"
			body: """
				The `message_buffering` can be made easier for clients by enabling ACK support. This
				changes the responsibility of tracking last received message from the clients to
				this component.

				To enable this, use `client_ack_config` configuration option for
				`message_buffering`.

				Example config:
				```yaml
				sinks:
				  websocket_sink:
					type: "websocket_server"
					inputs: ["demo_logs_test"]
					address: "0.0.0.0:1234"
					message_buffering:
						max_events: 1000
					  message_id_path: "message_id"
					  client_ack_config:
						ack_decoding:
						  codec: "json"
					    message_id_path: "id"
					encoding:
					  codec: "json"
				```

				This configuration will expect clients to send messages in format `{"id": "{message_id}"}`,
				and received message IDs will be stored in the component as the last received
				message for that client. By default, clients are identified by their IP address, but the
				`client_key` configuration option can be used to use different identification for
				clients.
				"""
		}
	}
}
