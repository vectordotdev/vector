package metadata

generated: components: sinks: socket: configuration: {
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
	encoding: {
		description: """
			Encoding configuration.
			Configures how events are encoded into raw bytes.
			The selected encoding also determines which input types (logs, metrics, traces) are supported.
			"""
		required: true
		type:     _schemaDefinitions["codecs::encoding::config::EncodingConfig"]
	}
	framing: {
		description:   "Framing configuration."
		relevant_when: "mode = \"tcp\" or mode = \"unix_stream\" or mode = \"unix_datagram\""
		required:      false
		type:          _schemaDefinitions["codecs::encoding::framing::framer::FramingConfig"]
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
			tcp: "Send over TCP."
			udp: "Send over UDP."
			unix_datagram: """
				Send over a Unix domain socket (UDS), in datagram mode.
				Unavailable on macOS, due to send(2)'s apparent non-blocking behavior,
				resulting in ENOBUFS errors which we currently don't handle.
				"""
			unix_stream: "Send over a Unix domain socket (UDS), in stream mode."
		}
	}
	path: {
		description: """
			The Unix socket path.

			This should be an absolute path.
			"""
		relevant_when: "mode = \"unix_stream\" or mode = \"unix_datagram\""
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
}
