package metadata

generated: components: sources: http_server: configuration: {
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
			The socket address to listen for connections on.

			It _must_ include a port.
			"""
		required: true
		type: string: examples: ["0.0.0.0:80", "localhost:80"]
	}
	auth: {
		description: """
			HTTP authentication configuration.

			Use HTTP authentication with HTTPS only. The authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.

			When using the `custom` strategy, the VRL program may write `%field = value` to enrich
			authenticated events. These metadata fields are injected into the event body (legacy
			namespace) or under `http_server.<field>` in event metadata (Vector namespace).
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
		type:     _schemaDefinitions["codecs::decoding::DeserializerConfig"]
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["codecs::decoding::FramingConfig"]
	}
	headers: {
		description: """
			A list of HTTP headers to include in the log event.

			Accepts the wildcard (`*`) character for headers matching a specified pattern.

			Specifying "*" results in all headers included in the log event.

			These headers are not included in the JSON payload if a field with a conflicting name exists.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["User-Agent", "X-My-Custom-Header", "X-*", "*"]
		}
	}
	host_key: {
		description: "If set, the name of the log field used to add the remote IP to each event"
		required:    false
		type: string: {
			default: ""
			examples: ["hostname"]
		}
	}
	keepalive: {
		description: "Configuration of HTTP server keepalive parameters."
		required:    false
		type:        _schemaDefinitions["vector::http::KeepaliveConfig"]
	}
	method: {
		description: "Specifies the action of the HTTP request."
		required:    false
		type: string: {
			default: "POST"
			enum: {
				DELETE:  "HTTP DELETE method."
				GET:     "HTTP GET method."
				HEAD:    "HTTP HEAD method."
				OPTIONS: "HTTP OPTIONS method."
				PATCH:   "HTTP PATCH method."
				POST:    "HTTP POST method."
				PUT:     "HTTP Put method."
			}
		}
	}
	path: {
		description: "The URL path on which log event POST requests are sent."
		required:    false
		type: string: {
			default: "/"
			examples: ["/event/path", "/logs"]
		}
	}
	path_key: {
		description: "The event key in which the requested URL path used to send the request is stored."
		required:    false
		type: string: {
			default: "path"
			examples: ["vector_http_path"]
		}
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
	response_code: {
		description: "Specifies the HTTP response status code that will be returned on successful requests."
		required:    false
		type: uint: {
			default: 200
			examples: [
				202
			]
		}
	}
	strict_path: {
		description: """
			Whether or not to treat the configured `path` as an absolute path.

			If set to `true`, only requests using the exact URL path specified in `path` are accepted. Otherwise,
			requests sent to a URL path that starts with the value of `path` are accepted.

			With `strict_path` set to `false` and `path` set to `""`, the configured HTTP source accepts requests from
			any URL path.
			"""
		required: false
		type: bool: default: true
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
