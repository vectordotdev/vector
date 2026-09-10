package metadata

generated: components: sources: heroku_logs: configuration: {
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
		description: "The socket address to listen for connections on."
		required:    true
		type: string: examples: ["0.0.0.0:80", "localhost:80"]
	}
	auth: {
		description: """
			Configuration of the authentication strategy for server mode sinks and sources.

			Use the HTTP authentication with HTTPS only. The authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::common::http::server_auth::HttpServerAuthConfig>"]
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::2c0db0ef1f05c78303bd4391"]
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["derived::2a4f9b813a8f495175f80ccf"]
	}
	keepalive: {
		description: "Configuration of HTTP server keepalive parameters."
		required:    false
		type:        _schemaDefinitions["vector::http::KeepaliveConfig"]
	}
	query_parameters: {
		description: """
			A list of URL query parameters to include in the log event.

			Accepts the wildcard (`*`) character for query parameters matching a specified pattern.

			Specifying "*" results in all query parameters included in the log event.

			These override any values included in the body with conflicting names.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["application", "source", "param*", "*"]
		}
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
