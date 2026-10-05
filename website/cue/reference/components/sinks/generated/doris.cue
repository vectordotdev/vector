package metadata

generated: components: sinks: doris: configuration: {
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
		description: "Compression algorithm to use for HTTP requests."
		required:    false
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
	database: {
		description: "The database that contains the table data will be inserted into."
		required:    true
		type: string: {
			examples: ["mydatabase"]
			syntax: "template"
		}
	}
	distribution: {
		description: "Options for determining the health of Doris endpoints."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector::sinks::util::service::health::HealthConfig>"]
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
	endpoints: {
		description: """
			A list of Doris endpoints to send logs to.

			The endpoint must contain an HTTP scheme, and may specify a
			hostname or IP address and port.
			"""
		required: true
		type: array: items: type: string: examples: ["http://127.0.0.1:8030"]
	}
	framing: {
		description: "Framing configuration."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::framing::framer::FramingConfig"]
	}
	headers: {
		description: """
			Custom HTTP headers to add to the request.

			These headers can be used to set Doris-specific Stream Load parameters:
			- `format`: Data format (json, csv.)
			- `read_json_by_line`: Whether to read JSON line by line
			- `strip_outer_array`: Whether to strip outer array brackets
			- Column mappings and transformations

			See [Doris Stream Load documentation](https://doris.apache.org/docs/data-operate/import/import-way/stream-load-manual)
			for all available parameters.
			"""
		required: false
		type: object: options: "*": {
			description: "An HTTP header value."
			required:    true
			type: string: {}
		}
	}
	label_prefix: {
		description: """
			The prefix for Stream Load label.
			The final label will be in format: `{label_prefix}_{database}_{table}_{timestamp}_{uuid}`.
			"""
		required: false
		type: string: {
			default: "vector"
			examples: [
				"vector"
			]
		}
	}
	log_request: {
		description: "Enable request logging."
		required:    false
		type: bool: default: false
	}
	max_retries: {
		description: "Number of retries attempted before failing."
		required:    false
		type: int: default: -1
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["vector::sinks::util::service::TowerRequestConfig"]
	}
	table: {
		description: "The table data is inserted into."
		required:    true
		type: string: {
			examples: ["mytable"]
			syntax: "template"
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
