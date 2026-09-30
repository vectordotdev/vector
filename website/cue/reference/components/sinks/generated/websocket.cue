package metadata

generated: components: sinks: websocket: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	auth: {
		description: "HTTP Authentication."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::http::Auth>"]
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
	ping_interval: {
		description: """
			The interval, in seconds, between sending [Ping][ping]s to the remote peer.

			If this option is not configured, pings are not sent on an interval.

			If the `ping_timeout` is not set, pings are still sent but there is no expectation of pong
			response times.

			[ping]: https://www.rfc-editor.org/rfc/rfc6455#section-5.5.2
			"""
		required: false
		type: uint: {
			examples: [
				30
			]
			unit: "seconds"
		}
	}
	ping_timeout: {
		description: """
			The number of seconds to wait for a [Pong][pong] response from the remote peer.

			If a response is not received within this time, the connection is re-established.

			[pong]: https://www.rfc-editor.org/rfc/rfc6455#section-5.5.3
			"""
		required: false
		type: uint: {
			examples: [
				5
			]
			unit: "seconds"
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
	uri: {
		description: """
			The WebSocket URI to connect to.

			This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
			 **Note**: Using the `wss://` protocol requires enabling `tls`.
			"""
		required: true
		type: string: examples: ["ws://localhost:8080", "wss://example.com/socket"]
	}
}
