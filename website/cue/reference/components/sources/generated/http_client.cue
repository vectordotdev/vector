package metadata

generated: components: sources: http_client: configuration: {
	auth: {
		description: "HTTP Authentication."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::http::Auth>"]
	}
	body: {
		description: """
			Raw data to send as the HTTP request body.

			Can be a static string or a VRL expression.

			When a body is provided, the `Content-Type` header is automatically set to
			`application/json` unless explicitly overridden in the `headers` configuration.
			"""
		required: false
		type:     _schemaDefinitions["vector::http::ParameterValue"]
	}
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::codecs::decoding::DeserializerConfig::185fc3d66caef78f8b28ff32"]
	}
	endpoint: {
		description: """
			The HTTP endpoint to collect events from.

			The full path must be specified.
			"""
		required: true
		type: string: examples: ["http://127.0.0.1:9898/logs"]
	}
	framing: {
		description: "Framing to use in the decoding."
		required:    false
		type:        _schemaDefinitions["derived::codecs::decoding::FramingConfig::2a4f9b813a8f495175f80ccf"]
	}
	headers: {
		description: """
			Headers to apply to the HTTP requests.

			One or more values for the same header can be provided.
			"""
		required: false
		type: object: {
			examples: [{
				Accept: ["text/plain", "text/html"]
				"X-My-Custom-Header": ["a", "vector", "of", "values"]
			}]
			options: "*": {
				description: "An HTTP request header and its value(s)."
				required:    true
				type: array: items: type: string: {}
			}
		}
	}
	method: {
		description: "Specifies the method of the HTTP request."
		required:    false
		type: string: {
			default: "GET"
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
	query: {
		description: """
			Custom parameters for the HTTP request query string.

			One or more values for the same parameter key can be provided.

			The parameters provided in this option are appended to any parameters
			manually provided in the `endpoint` option.

			VRL functions are supported within query parameters. You can
			use functions like `now()` to dynamically modify query
			parameter values.
			"""
		required: false
		type: object: {
			examples: [{
				field: "value"
				fruit: ["mango", "papaya", "kiwi"]
				start_time: {
					type:  "vrl"
					value: "now()"
				}
			}]
			options: "*": {
				description: "Configuration of the query parameter value for HTTP requests."
				required:    true
				type:        _schemaDefinitions["vector::http::ParameterValue"]
			}
		}
	}
	redirects: {
		description: "HTTP redirect configuration. Redirects are not followed by default."
		required:    false
		type: object: {
			examples: [{follow: true}]
			options: {
				cross_origin: {
					description: "The behavior to apply to redirects that point to a different origin."
					required:    false
					type: object: options: {
						not_forward_headers: {
							description: "The names of the headers that are not forwarded to a different origin."
							required:    false
							type: array: items: type: string: {}
						}
					}
				}
				follow: {
					description: "Whether to follow HTTP redirects."
					required:    false
					type:        bool: {}
				}
				max: {
					description: "The maximum number of redirects to follow. This only applies when redirects are followed."
					required:    false
					type:        uint: default: 5
				}
			}
		}
	}
	scrape_interval_secs: {
		description: """
			The interval between scrapes. Requests are run concurrently so if a scrape takes longer
			than the interval a new scrape will be started. This can take extra resources, set the timeout
			to a value lower than the scrape interval to prevent this from happening.
			"""
		required: false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
	scrape_timeout_secs: {
		description: "The timeout for each scrape request."
		required:    false
		type: float: {
			default: 5.0
			unit:    "seconds"
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
