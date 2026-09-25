package metadata

generated: components: sinks: syslog: configuration: {
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

			Both IP address and hostname are accepted formats.

			The address _must_ include a port.
			"""
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      true
		type: string: examples: ["92.12.333.224:5000", "https://somehost:5000"]
	}
	framing: {
		description: """
			Stream framing configuration.

			Applies only to stream-oriented transports: TCP and Unix stream sockets. UDP
			sends exactly one syslog message per datagram and does not use framing.
			"""
		relevant_when: "mode = \"tcp\" or mode = \"unix_stream\""
		required:      false
		type: object: {
			examples: [{
				method: "octet_counting"
			}]
			options: method: {
				description: "The framing method used to separate syslog messages in stream transports."
				required:    false
				type: string: {
					default: "newline_delimited"
					enum: {
						newline_delimited: """
															Terminates each syslog message with a newline (LF) character.

															This is RFC 6587 non-transparent framing. Use octet-counting if
															messages can contain embedded newlines.
															"""
						octet_counting: """
															Prefixes each syslog message with its byte length and a space.

															This is RFC 6587 octet-counting framing. When used with TCP, TLS, and
															RFC 5424 messages, this is the framing required by RFC 5425.
															"""
					}
				}
			}
		}
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
			tcp:         "Send over TCP."
			udp:         "Send over UDP."
			unix_stream: "Send over a Unix domain socket (UDS), in stream mode."
		}
	}
	path: {
		description: """
			The Unix socket path.

			This should be an absolute path.
			"""
		relevant_when: "mode = \"unix_stream\""
		required:      true
		type: string: examples: ["/path/to/socket"]
	}
	send_buffer_bytes: {
		description: """
			The size of the socket's send buffer.

			If set, the value of the setting is passed via the `SO_SNDBUF` option.
			"""
		relevant_when: "mode = \"tcp\" or mode = \"udp\""
		required:      false
		type: uint: {
			examples: [
				65536,
			]
			unit: "bytes"
		}
	}
	syslog: {
		description: """
			Syslog encoding options.

			Controls the RFC format, facility, severity, and field mappings for the syslog output.
			"""
		required: false
		type: object: {
			examples: [{
				app_name: ".app_name"
				facility: ".facility"
				msg_id:   ".msg_id"
				proc_id:  ".proc_id"
				rfc:      "rfc5424"
				severity: ".severity"
			}]
			options: {
				app_name: {
					description: """
						Path to a field in the event to use for the app name.

						If not provided, the encoder checks for a semantic "service" field.
						If that is also missing, it defaults to "vector".
						"""
					required: false
					type: string: {}
				}
				facility: {
					description: "Path to a field in the event to use for the facility. Defaults to \"user\"."
					required:    false
					type: string: {}
				}
				msg_id: {
					description: "Path to a field in the event to use for the msg ID."
					required:    false
					type: string: {}
				}
				proc_id: {
					description: "Path to a field in the event to use for the proc ID."
					required:    false
					type: string: {}
				}
				rfc: {
					description: "RFC to use for formatting."
					required:    false
					type: string: {
						default: "rfc5424"
						enum: {
							rfc3164: "The legacy RFC3164 syslog format."
							rfc5424: "The modern RFC5424 syslog format."
						}
					}
				}
				severity: {
					description: "Path to a field in the event to use for the severity. Defaults to \"informational\"."
					required:    false
					type: string: {}
				}
			}
		}
	}
	tls: {
		description:   "Configures the TLS options for incoming/outgoing connections."
		relevant_when: "mode = \"tcp\""
		required:      false
		type:          _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
