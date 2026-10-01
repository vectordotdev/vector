package metadata

generated: components: sinks: humio_metrics: configuration: {
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
		type: string: default: "host"
	}
	host_tag: {
		description: """
			Name of the tag in the metric to use for the source host.

			If present, the value of the tag is set on the generated log event in the `host` field,
			where the field key uses the [global `host_key` option][global_log_schema_host_key].

			[global_log_schema_host_key]: https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key
			"""
		required: false
		type: string: examples: ["host", "hostname"]
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
	metric_tag_values: {
		description: """
			Controls how metric tag values are encoded.

			When set to `single`, only the last non-bare value of tags is displayed with the
			metric.  When set to `full`, all metric tags are exposed as separate assignments as
			described by [the `native_json` codec][vector_native_json].
			When set to `auto`, tag values are encoded using their underlying shape.

			[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
			"""
		required: false
		type: string: {
			default: "single"
			enum: {
				auto: """
					Tag values are exposed using their underlying shape: single-value tags as strings,
					multi-value tags as arrays. A length-1 array round-trips as a scalar; use `Full` to
					force array shape.
					"""
				full: "All tags are exposed as arrays of either string or null values."
				single: """
					Tag values are exposed as single strings, the same as they were before this config
					option. Tags with multiple values show the last assigned value, and null values
					are ignored.
					"""
			}
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

			Typically the filename the metrics originated from. Maps to `@source` in Humio.
			"""
		required: false
		type: string: syntax: "template"
	}
	timezone: {
		description: """
			The name of the time zone to apply to timestamp conversions that do not contain an explicit
			time zone.

			This overrides the [global `timezone`][global_timezone] option. The time zone name may be
			any name in the [TZ database][tz_database] or `local` to indicate system local time.

			[global_timezone]: https://vector.dev/docs/reference/configuration//global-options#timezone
			[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
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
