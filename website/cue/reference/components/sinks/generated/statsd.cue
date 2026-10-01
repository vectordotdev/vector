package metadata

generated: components: sinks: statsd: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	address: {
		description: """
			The address to connect to.

			Both IP addresses and hostnames/fully qualified domain names (FQDNs) are accepted formats.

			The address _must_ include a port.
			"""
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      true
		type: string: examples: ["92.12.333.224:5000", "somehost:5000"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized or compressed.
					"""
				required: false
				type: uint: {
					default: 1300
					unit:    "bytes"
				}
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: {
					default: 1000
					unit:    "events"
				}
			}
			timeout_secs: {
				description: "The maximum age of a batch before it is flushed."
				required:    false
				type: float: {
					default: 1.0
					unit:    "seconds"
				}
			}
		}
	}
	default_namespace: {
		description: """
			Sets the default namespace for any metrics sent.

			This namespace is only used if a metric has no existing namespace. When a namespace is
			present, it is used as a prefix to the metric name, and separated with a period (`.`).
			"""
		required: false
		type: string: examples: ["service"]
	}
	keepalive: {
		description:   "TCP keepalive settings for socket-based components."
		relevant_when: "mode = \"tcp\""
		required:      false
		type:          _schemaDefinitions["core::option::Option<vector_core::tcp::TcpKeepaliveConfig>"]
	}
	mode: {
		description: "The type of socket to use."
		required:    true
		type: string: enum: {
			tcp:  "Send over TCP."
			udp:  "Send over UDP."
			unix: "Send over a Unix domain socket (UDS)."
		}
	}
	path: {
		description: """
			The Unix socket path.

			This should be an absolute path.
			"""
		relevant_when: "mode = \"unix\""
		required:      true
		type: string: examples: ["/path/to/socket"]
	}
	send_buffer_size: {
		description: """
			The size of the socket's send buffer.

			If set, the value of the setting is passed via the `SO_SNDBUF` option.
			"""
		required: false
		type: uint: {
			examples: [
				65536
			]
			unit: "bytes"
		}
	}
	tls: {
		description:   "Configures the TLS options for incoming/outgoing connections."
		relevant_when: "mode = \"tcp\""
		required:      false
		type:          _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
	unix_mode: {
		description:   "The Unix socket mode to use."
		relevant_when: "mode = \"unix\""
		required:      false
		type: string: {
			default: "Stream"
			enum: {
				Datagram: "Datagram-oriented (`SOCK_DGRAM`)."
				Stream:   "Stream-oriented (`SOCK_STREAM`)."
			}
		}
	}
}
