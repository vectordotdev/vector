package metadata

generated: components: sinks: file: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	base_dir: {
		description: """
			Directory under which all rendered `path` values must resolve.

			When `path` contains event-field references (`{{ field }}`), Vector
			confines every rendered path to this directory. If unset, the base
			directory is derived from the literal prefix of `path` (the portion
			before the first `{{` or `%`). Configuration fails if `path`
			references event fields and no non-root base directory can be
			derived.
			"""
		required: false
		type: string: examples: ["/var/log/vector"]
	}
	compression: {
		description: "Compression configuration."
		required:    false
		type: string: {
			default: "none"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
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
	idle_timeout_secs: {
		description: """
			The amount of time that a file can be idle and stay open.

			After not receiving any events in this amount of time, the file is flushed and closed.
			"""
		required: false
		type: uint: {
			default: 30
			examples: [
				600
			]
			unit: "seconds"
		}
	}
	internal_metrics: {
		description: "Configuration of internal metrics for file-based components."
		required:    false
		type:        _schemaDefinitions["vector::internal_events::file::FileInternalMetricsConfig"]
	}
	path: {
		description: """
			File path to write events to.

			Compression format extension must be explicit.
			"""
		required: true
		type: string: {
			examples: ["/var/log/vector/vector-%Y-%m-%d.log", "/tmp/application-{{ application_id }}-%Y-%m-%d.log", "/tmp/vector-%Y-%m-%d.log.zst"]
			syntax: "template"
		}
		warnings: ["Rendered paths are confined to `base_dir` (derived from the literal prefix of `path` when unset). See the `base_dir` option."]
	}
	timezone: {
		description: """
			Timezone to use for any date specifiers in template strings.

			This can refer to any valid timezone as defined in the [TZ database][tzdb], or "local" which refers to the system local timezone. It will default to the [globally configured timezone](https://vector.dev/docs/reference/configuration/global-options/#timezone).

			[tzdb]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
	}
	truncate: {
		description: "Configuration for truncating files."
		required:    false
		type: object: options: {
			after_close_time_secs: {
				description: "If this is set, files will be truncated after being closed for a set amount of seconds."
				required:    false
				type: uint: {}
			}
			after_modified_time_secs: {
				description: "If this is set, files will be truncated after set amount of seconds of no modifications."
				required:    false
				type: uint: {}
			}
			after_secs: {
				description: "If this is set, files will be truncated after set amount of seconds regardless of the state."
				required:    false
				type: uint: {}
			}
		}
	}
}
