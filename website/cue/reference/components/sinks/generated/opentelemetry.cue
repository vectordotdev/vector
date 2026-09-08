package metadata

generated: components: sinks: opentelemetry: configuration: protocol: {
	description: "Protocol configuration"
	required:    true
	type: object: options: {
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
			description: """
				Configuration of the authentication strategy for HTTP requests.

				HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
				HTTP header without any additional encryption beyond what is provided by the transport itself.
				"""
			required: false
			type:     _schemaDefinitions["core::option::Option<vector::http::Auth>"]
		}
		batch: {
			description: "Event batching behavior."
			required:    false
			type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
		}
		compression: {
			description: """
				Compression configuration.

				All compression algorithms use the default compression level unless otherwise specified.
				"""
			required: false
			type: string: {
				default: "none"
				enum: {
					gzip: """
						[Gzip][gzip] compression.

						[gzip]: https://www.gzip.org/
						"""
					none: "No compression."
					snappy: """
						[Snappy][snappy] compression.

						[snappy]: https://github.com/google/snappy/blob/main/docs/README.md
						"""
					zlib: """
						[Zlib][zlib] compression.

						[zlib]: https://zlib.net/
						"""
					zstd: """
						[Zstandard][zstd] compression.

						[zstd]: https://facebook.github.io/zstd/
						"""
				}
			}
		}
		dangerously_allow_unconfined_template_resolution: {
			description: """
				Disable all template confinement checks for this sink.

				**DANGEROUS — disables a security control.**

				Bypasses both startup validation and runtime confinement for every
				templated field on this sink. When enabled, a log producer that
				controls any field used in a template can write to arbitrary keys,
				paths, or routing destinations. This flag is a full opt-out: it
				disables confinement even for templates that have a usable static
				prefix.
				"""
			required: false
			type: bool: default: false
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
			description: "Framing configuration."
			required:    false
			type:        _schemaDefinitions["codecs::encoding::framing::framer::FramingConfig"]
		}
		method: {
			description: "The HTTP method to use when making the request."
			required:    false
			type: string: {
				default: "post"
				enum: {
					delete:  "DELETE."
					get:     "GET."
					head:    "HEAD."
					options: "OPTIONS."
					patch:   "PATCH."
					post:    "POST."
					put:     "PUT."
					trace:   "TRACE."
				}
			}
		}
		payload_prefix: {
			description: """
				A string to prefix the payload with.

				This option is ignored if the encoding is not character delimited JSON.

				If specified, the `payload_suffix` must also be specified and together they must produce a valid JSON object.
				"""
			required: false
			type: string: {
				default: ""
				examples: ["{\"data\":"]
			}
		}
		payload_suffix: {
			description: """
				A string to suffix the payload with.

				This option is ignored if the encoding is not character delimited JSON.

				If specified, the `payload_prefix` must also be specified and together they must produce a valid JSON object.
				"""
			required: false
			type: string: {
				default: ""
				examples: ["}"]
			}
		}
		request: {
			description: "Outbound HTTP request settings."
			required:    false
			type:        _schemaDefinitions["vector::sinks::util::http::RequestConfig"]
		}
		retry_strategy: {
			description: """
				Configurable retry strategy for `http` based sinks.

				For more information about error responses, see [Client Error Responses][error_responses].

				[error_responses]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status#client_error_responses
				"""
			required: false
			type: object: options: {
				status_codes: {
					description:   "Retry on these specific HTTP status codes"
					relevant_when: "type = \"custom\""
					required:      true
					type: array: items: type: uint: {}
				}
				type: {
					description: "The retry strategy enum."
					required:    false
					type: string: {
						default: "default"
						enum: {
							all:     "Retry on *all* HTTP status codes except for success codes (2xx)"
							custom:  "Custom retry strategy"
							default: "Default strategy. See [`RetryStrategy::retry_action`] for more details."
							none:    "Don't retry any errors, including request timeouts."
						}
					}
				}
			}
		}
		tls: {
			description: "TLS configuration."
			required:    false
			type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
		}
		type: {
			description: "The communication protocol."
			required:    true
			type: string: enum: http: "Send data over HTTP."
		}
		uri: {
			description: """
				The full URI to make HTTP requests to.

				This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
				"""
			required: true
			type: string: {
				examples: ["https://10.22.212.22:9000/endpoint"]
				syntax: "template"
			}
		}
	}
}
