package metadata

generated: components: sinks: axiom: configuration: {
	acknowledgements: {
		description: "Controls how acknowledgements are handled for this sink."
		required:    false
		type:        _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: "The batch settings for the sink."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
	}
	compression: {
		description: "The compression algorithm to use."
		required:    false
		type: string: {
			default: "zstd"
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
	dataset: {
		description: "The Axiom dataset to write to."
		required:    true
		type: string: examples: ["${AXIOM_DATASET}", "vector_rocks"]
	}
	org_id: {
		description: """
			The Axiom organization ID.

			Only required when using personal tokens.
			"""
		required: false
		type: string: examples: ["${AXIOM_ORG_ID}", "123abc"]
	}
	region: {
		description: """
			The Axiom regional edge domain to use for ingestion.

			Specify the domain name only (no scheme, no path).
			When set, data is sent to `https://{region}/v1/ingest/{dataset}`.
			Cannot be used together with `url`.
			"""
		required: false
		type: string: examples: ["mumbai.axiom.co", "${AXIOM_REGION}", "eu-central-1.aws.edge.axiom.co"]
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
		description: """
			The TLS settings for the connection.

			Optional, constrains TLS settings for this sink.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
	token: {
		description: "The Axiom API token."
		required:    true
		type: string: examples: ["${AXIOM_TOKEN}", "123abc"]
	}
	url: {
		description: """
			URI of the Axiom endpoint to send data to.

			If a path is provided, the URL is used as-is.
			If no path (or only `/`) is provided, `/v1/datasets/{dataset}/ingest` is appended for backwards compatibility.
			This takes precedence over `region` if both are set (but both should not be set).
			"""
		required: false
		type: string: {}
	}
}
