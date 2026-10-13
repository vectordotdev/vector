package metadata

generated: components: sinks: opentelemetry: configuration: {
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
		relevant_when: "protocol = \"http\""
		required:      false
		type:          _schemaDefinitions["core::option::Option<vector::http::Auth>"]
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
		warnings: ["The `grpc` protocol only supports `none`, `gzip`, and `zstd`. Specifying any other algorithm causes Vector to fail at startup."]
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
		relevant_when: "protocol = \"http\""
		required:      true
		type:          _schemaDefinitions["codecs::encoding::config::EncodingConfig"]
	}
	framing: {
		description:   "Framing configuration."
		relevant_when: "protocol = \"http\""
		required:      false
		type:          _schemaDefinitions["codecs::encoding::framing::framer::FramingConfig"]
	}
	method: {
		description:   "The HTTP method to use. Defaults to `post`."
		relevant_when: "protocol = \"http\""
		required:      false
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
		description:   "A string to prefix the payload with."
		relevant_when: "protocol = \"http\""
		required:      false
		type: string: default: ""
	}
	payload_suffix: {
		description:   "A string to suffix the payload with."
		relevant_when: "protocol = \"http\""
		required:      false
		type: string: default: ""
	}
	protocol: {
		description: "The transport protocol to use."
		required:    true
		type: string: enum: {
			grpc: "Send OTLP data over gRPC."
			http: "Send OTLP data over HTTP."
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
		relevant_when: "protocol = \"http\""
		required:      false
		type:          _schemaDefinitions["derived::vector::sinks::util::http::RetryStrategy::64fe77681a1c274eed24cca6"]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
	uri: {
		description: """
			The URI to send requests to.

			Supports template syntax (e.g. `http://localhost:4317/{{ tenant }}`). Must include a scheme
			(`http://` or `https://`) and a port.

			For the gRPC transport, the template is rendered once per batch using the first event
			in the batch.

			# Examples

			- `http://localhost:5318/v1/logs` (HTTP)
			- `http://localhost:4317` (gRPC)
			"""
		required: true
		type: string: {
			examples: ["http://localhost:5318/v1/logs", "http://localhost:4317"]
			syntax: "template"
		}
		warnings: ["URI templates are confined to their configured authority and path prefix. Dynamic authorities require `dangerously_allow_unconfined_template_resolution = true`."]
	}
}
