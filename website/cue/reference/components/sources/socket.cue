package metadata

components: sources: socket: {
	_port: 9000

	title: "Socket"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		deployment_roles: ["aggregator", "sidecar"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: false
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "`newline_delimited` for TCP and Unix stream modes when using codecs other than `native` (which defaults to `length_delimited`), `bytes` for UDP and Unix datagram modes"
		}
		receive: {
			from: {
				service: services.socket_client
				interface: socket: {
					direction: "incoming"
					port:      _port
					protocols: ["tcp", "unix_datagram", "unix_stream", "udp"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: {
				enabled:       true
				relevant_when: "mode = `tcp` or mode = `udp`"
			}
			keepalive: enabled: true
			tls: {
				enabled:                 true
				can_verify_certificate:  true
				can_add_client_metadata: true
				enabled_default:         false
			}
		}
		auto_generated: true
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.socket.configuration

	output: logs: line: {
		description: "A single socket event."
		fields: {
			host: {
				description: "The peer host IP address."
				required:    true
				type: string: {
					examples: ["129.21.31.122"]
				}
			}
			message:   fields._raw_line
			timestamp: fields._current_timestamp
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["socket"]
				}
			}
			port: {
				description: "The peer source port."
				required:    false
				common:      true
				type: uint: {
					default: null
					unit:    null
					examples: [2838]
				}
			}
			client_metadata: fields._client_metadata
		}
	}

	examples: [
		{
			_line: """
				2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
				"""
			title: "Socket line"

			configuration: {}

			input: "\( _line )"
			output: log: {
				timestamp:   _values.current_timestamp
				message:     _line
				host:        _values.local_host
				source_type: "socket"
			}
		},
	]

	telemetry: metrics: {
		connection_established_total: components.sources.internal_metrics.output.metrics.connection_established_total
		connection_send_errors_total: components.sources.internal_metrics.output.metrics.connection_send_errors_total
		connection_shutdown_total:    components.sources.internal_metrics.output.metrics.connection_shutdown_total
	}
}
