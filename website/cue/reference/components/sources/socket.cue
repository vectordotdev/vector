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
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "`newline_delimited` for TCP and Unix stream, `bytes` for UDP and Unix datagram"
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
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		address: {
			description:   "The address to listen for connections on, or `systemd#N` to use the Nth socket passed by systemd socket activation. If an address is used it _must_ include a port."
			relevant_when: "mode = `tcp` or `udp`"
			required:      true
			type: string: {
				examples: ["0.0.0.0:\(_port)", "systemd", "systemd#3"]
			}
		}
		host_key: {
			category:    "Context"
			common:      false
			description: """
				The key name added to each event representing the current host. This can also be globally set via the
				[global `host_key` option](\(urls.vector_configuration)/global-options#log_schema.host_key).
				"""
			required:    false
			type: string: {
				default: "host"
			}
		}
		max_length: {
			common:      true
			description: "The maximum buffer size of incoming messages. Messages larger than this are truncated."
			required:    false
			type: uint: {
				default: 102400
				unit:    "bytes"
			}
		}
		mode: {
			description: "The type of socket to use."
			required:    true
			type: string: {
				enum: {
					tcp:           "TCP socket."
					udp:           "UDP socket."
					unix_datagram: "Unix domain datagram socket."
					unix_stream:   "Unix domain stream socket."
				}
			}
		}
		path: {
			description:   "The unix socket path. *This should be an absolute path*."
			relevant_when: "mode = `unix_datagram` or `unix_stream`"
			required:      true
			type: string: {
				examples: ["/path/to/socket"]
			}
		}
		shutdown_timeout_secs: {
			common:        false
			description:   "The timeout before a connection is forcefully closed during shutdown."
			relevant_when: "mode = `tcp`"
			required:      false
			type: uint: {
				default: 30
				unit:    "seconds"
			}
		}
		connection_limit: {
			common:        false
			description:   "The max number of TCP connections that will be processed."
			relevant_when: "mode = `tcp`"
			required:      false
			type: uint: {
				default: null
				unit:    "concurrency"
			}
		}
	}

	output: logs: line: {
		description: "A single socket event."
		fields: {
			host:      fields._local_host
			message:   fields._raw_line
			timestamp: fields._current_timestamp
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
				timestamp: _values.current_timestamp
				message:   _line
				host:      _values.local_host
			}
		},
	]

	telemetry: metrics: {
		events_in_total:                  components.sources.internal_metrics.output.metrics.events_in_total
		connection_errors_total:          components.sources.internal_metrics.output.metrics.connection_errors_total
		connection_failed_total:          components.sources.internal_metrics.output.metrics.connection_failed_total
		connection_established_total:     components.sources.internal_metrics.output.metrics.connection_established_total
		connection_failed_total:          components.sources.internal_metrics.output.metrics.connection_failed_total
		connection_send_errors_total:     components.sources.internal_metrics.output.metrics.connection_send_errors_total
		connection_send_ack_errors_total: components.sources.internal_metrics.output.metrics.connection_send_ack_errors_total
		connection_shutdown_total:        components.sources.internal_metrics.output.metrics.connection_shutdown_total
		component_received_bytes_total:   components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:  components.sources.internal_metrics.output.metrics.component_received_events_total
	}
}
