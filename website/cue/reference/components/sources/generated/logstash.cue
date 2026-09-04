package metadata

generated: components: sources: logstash: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::SourceAcknowledgementsConfig"]
	}
	address: {
		description: """
			The socket address to listen for connections on, or `systemd{#N}` to use the Nth socket passed by
			systemd socket activation.

			If a socket address is used, it _must_ include a port.
			"""
		required: true
		type: string: examples: ["0.0.0.0:9000", "systemd", "systemd#3"]
	}
	connection_limit: {
		description: "The maximum number of TCP connections that are allowed at any given time."
		required:    false
		type: uint: unit: "connections"
	}
	keepalive: {
		description: "TCP keepalive settings for socket-based components."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tcp::TcpKeepaliveConfig>"]
	}
	permit_origin: {
		description: "List of allowed origin IP networks. IP addresses must be in CIDR notation."
		required:    false
		type: array: items: type: string: examples: ["192.168.0.0/16", "127.0.0.1/32", "::1/128", "9876:9ca3:99ab::23/128"]
	}
	receive_buffer_bytes: {
		description: "The size of the receive buffer used for each connection."
		required:    false
		type: uint: {
			examples: [
				65536
			]
			unit: "bytes"
		}
	}
	tls: {
		description: "`TlsEnableableConfig` for `sources`, adding metadata from the client certificate."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsSourceConfig>"]
	}
	tls_handshake_timeout_secs: {
		description: """
			The timeout, in seconds, before a TLS handshake is aborted if it has not completed.

			This bounds how long a connection can hold its slot against `connection_limit`
			before the TLS handshake finishes, protecting against clients that open a
			connection but never complete (or never start) a handshake.
			"""
		required: false
		type: uint: unit: "seconds"
	}
}
