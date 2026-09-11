package metadata

generated: components: sinks: humio_logs: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::splunk_hec::common::util::SplunkHecDefaultBatchSettings>"]
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
	endpoint: {
		description: """
			The base URL of the Humio instance.

			The scheme (`http` or `https`) must be specified. No path should be included since the paths defined
			by the [`Splunk`][splunk] API are used.

			[splunk]: https://docs.splunk.com/Documentation/Splunk/8.0.0/Data/HECRESTendpoints
			"""
		required: false
		type: string: {
			default: "https://cloud.humio.com/"
			examples: ["http://127.0.0.1", "https://example.com"]
		}
	}
	event_type: {
		description: """
			The type of events sent to this sink. Humio uses this as the name of the parser to use to ingest the data.

			If unset, Humio defaults it to none.
			"""
		required: false
		type: string: {
			examples: ["json", "none", "event_type-{{ event_type }}"]
			syntax: "template"
		}
	}
	host_key: {
		description: """
			Overrides the name of the log field used to retrieve the hostname to send to Humio.

			By default, the [global `log_schema.host_key` option][global_host_key] is used if log
			events are Legacy namespaced, or the semantic meaning of "host" is used, if defined.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: default: ".host"
	}
	index: {
		description: """
			Optional name of the repository to ingest into.

			In public-facing APIs, this must (if present) be equal to the repository used to create the ingest token used for authentication.

			In private cluster setups, Humio can be configured to allow these to be different.

			For more information, see [Humio’s Format of Data][humio_data_format].

			[humio_data_format]: https://docs.humio.com/integrations/data-shippers/hec/#format-of-data
			"""
		required: false
		type: string: {
			examples: ["index-{{ host }}", "custom_index"]
			syntax: "template"
		}
	}
	indexed_fields: {
		description: """
			Event fields to be added to Humio’s extra fields.

			Can be used to tag events by specifying fields starting with `#`.

			For more information, see [Humio’s Format of Data][humio_data_format].

			[humio_data_format]: https://docs.humio.com/integrations/data-shippers/hec/#format-of-data
			"""
		required: false
		type: array: {
			default: []
			items: type: string: {}
		}
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
	source: {
		description: """
			The source of events sent to this sink.

			Typically the filename the logs originated from. Maps to `@source` in Humio.
			"""
		required: false
		type: string: syntax: "template"
	}
	timestamp_key: {
		description: """
			Overrides the name of the log field used to retrieve the timestamp to send to Humio.
			When set to `“”`, a timestamp is not set in the events sent to Humio.

			By default, either the [global `log_schema.timestamp_key` option][global_timestamp_key] is used
			if log events are Legacy namespaced, or the semantic meaning of "timestamp" is used, if defined.

			[global_timestamp_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.timestamp_key
			"""
		required: false
		type: string: default: ".timestamp"
	}
	timestamp_nanos_key: {
		description: "Overrides the name of the log field used to retrieve the nanosecond-enabled timestamp to send to Humio."
		required:    false
		type: string: default: "@timestamp.nanos"
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
	token: {
		description: "The Humio ingestion token."
		required:    true
		type: string: examples: ["${HUMIO_TOKEN}", "A94A8FE5CCB19BA61C4C08"]
	}
}
