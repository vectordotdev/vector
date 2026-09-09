package metadata

generated: components: sources: splunk_hec: configuration: {
	acknowledgements: {
		description: "Acknowledgement configuration for the `splunk_hec` source."
		required:    false
		type: object: options: {
			ack_idle_cleanup: {
				description: """
					Whether or not to remove channels after idling for `max_idle_time` seconds.

					A channel is idling if it is not used for sending data or querying acknowledgement statuses.
					"""
				required: false
				type: bool: default: false
			}
			enabled: {
				description: "Enables end-to-end acknowledgements."
				required:    false
				type: bool: {}
			}
			max_idle_time: {
				description: """
					The amount of time, in seconds, a channel is allowed to idle before removal.

					Channels can potentially idle for longer than this setting but clients should not rely on such behavior.

					Minimum of `1`.
					"""
				required: false
				type: uint: default: 300
			}
			max_number_of_ack_channels: {
				description: """
					The maximum number of Splunk HEC channels clients can use with this source.

					Minimum of `1`.
					"""
				required: false
				type: uint: default: 1000000
			}
			max_pending_acks: {
				description: """
					The maximum number of acknowledgement statuses pending query across all channels.

					Equivalent to the `max_number_of_acked_requests_pending_query` Splunk HEC setting.

					Minimum of `1`.
					"""
				required: false
				type: uint: default: 10000000
			}
			max_pending_acks_per_channel: {
				description: """
					The maximum number of acknowledgement statuses pending query for a single channel.

					Equivalent to the `max_number_of_acked_requests_pending_query_per_ack_channel` Splunk HEC setting.

					Minimum of `1`.
					"""
				required: false
				type: uint: default: 1000000
			}
		}
	}
	address: {
		description: """
			The socket address to listen for connections on.

			The address _must_ include a port.
			"""
		required: false
		type: string: default: "0.0.0.0:8088"
	}
	event: {
		description: """
			Codec configuration applied to events received on `/services/collector/event`.

			When `decoding` is set, Vector applies a second decoding pass after parsing the
			HEC envelope. The envelope's `event` field is passed through the codec,
			and a single envelope can fan out to multiple events. Decode failures are
			swallowed and do not return an error to the Splunk client.

			The VRL codec can access HEC envelope metadata, such as host, sourcetype, and,
			channel, and the authentication token via `%splunk_hec.*` paths and
			`get_secret!("splunk_hec_token")` before the program executes.
			"""
		required: false
		type:     _schemaDefinitions["vector::sources::splunk_hec::CodecConfig"]
	}
	keepalive: {
		description: "Configuration of HTTP server keepalive parameters."
		required:    false
		type:        _schemaDefinitions["vector::http::KeepaliveConfig"]
	}
	raw: {
		description: """
			Codec configuration applied to events received on `/services/collector/raw`.

			When `decoding` is set, the (decompressed) request body is fed through the
			codec instead of being emitted as a single event. Decode failures are
			swallowed and do not return an error to the Splunk client. When unset, the
			endpoint preserves its existing behavior of one event per request body.
			"""
		required: false
		type:     _schemaDefinitions["vector::sources::splunk_hec::CodecConfig"]
	}
	store_hec_token: {
		description: """
			Whether or not to forward the Splunk HEC authentication token with events.

			If set to `true`, when incoming requests contain a Splunk HEC token, the token used is kept in the
			event metadata and preferentially used if the event is sent to a Splunk HEC sink.
			"""
		required: false
		type: bool: default: false
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
	token: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, use `valid_tokens` instead."
		description: """
			Optional authorization token.

			If supplied, incoming requests must supply this token in the `Authorization` header, just as a client would if
			it was communicating with the Splunk HEC endpoint directly.

			If _not_ supplied, the `Authorization` header is ignored and requests are not authenticated.
			"""
		required: false
		type: string: {}
	}
	valid_tokens: {
		description: """
			A list of valid authorization tokens.

			If supplied, incoming requests must supply one of these tokens in the `Authorization` header, just as a client
			would if it was communicating with the Splunk HEC endpoint directly.

			If _not_ supplied, the `Authorization` header is ignored and requests are not authenticated.
			"""
		required: false
		type: array: items: type: string: examples: ["A94A8FE5CCB19BA61C4C08"]
	}
}
