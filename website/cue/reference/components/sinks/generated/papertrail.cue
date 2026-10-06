package metadata

generated: components: sinks: papertrail: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
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
	endpoint: {
		description: "The TCP endpoint to send logs to."
		required:    true
		type: string: examples: ["logs.papertrailapp.com:12345"]
	}
	keepalive: {
		description: "TCP keepalive settings for socket-based components."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tcp::TcpKeepaliveConfig>"]
	}
	process: {
		description: "The value to use as the `process` in Papertrail."
		required:    false
		type: string: {
			default: "vector"
			examples: ["{{ process }}", "my-process"]
			syntax: "template"
		}
	}
	send_buffer_bytes: {
		description: "Configures the send buffer size using the `SO_SNDBUF` option on the socket."
		required:    false
		type: uint: {}
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
