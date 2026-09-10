package metadata

generated: components: sources: websocket: configuration: {
	auth: {
		description: "HTTP Authentication."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::http::Auth>"]
	}
	connect_timeout_secs: {
		description: "Number of seconds before timing out while connecting."
		required:    false
		type: uint: {
			default: 30
			examples: [
				10
			]
			unit: "seconds"
		}
	}
	decoding: {
		description: "Decoder to use on each received message."
		required:    false
		type:        _schemaDefinitions["derived::2c0db0ef1f05c78303bd4391"]
	}
	framing: {
		description: "Framing to use in the decoding."
		required:    false
		type:        _schemaDefinitions["derived::2a4f9b813a8f495175f80ccf"]
	}
	initial_message: {
		description: "An optional message to send to the server upon connection."
		required:    false
		type: string: examples: ["SUBSCRIBE logs"]
	}
	initial_message_timeout_secs: {
		description: """
			Number of seconds before timing out while waiting for a reply to the initial message.
			This is only used when `initial_message` is also configured.
			"""
		required: false
		type: uint: {
			default: 2
			examples: [
				5
			]
			unit: "seconds"
		}
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
	ping_message: {
		description: """
			An optional application-level ping message to send over the WebSocket connection.
			If not set, a standard WebSocket ping control frame is sent instead.
			"""
		required: false
		type: string: {}
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
	pong_message: {
		description: """
			The expected application-level pong message to listen for as a response to a custom `ping_message`.
			This is only used when `ping_message` is also configured. When a custom ping is sent,
			receiving this specific message confirms that the connection is still alive.
			"""
		required: false
		type: {
			object: options: {
				type: {
					description: "The matching strategy to use for the pong message."
					required:    true
					type: string: enum: {
						contains: "The message must contain the value as a substring."
						exact:    "The entire message must be an exact match."
					}
				}
				value: {
					description: "The string value to match against."
					required:    true
					type: string: {}
				}
			}
			string: {}
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
