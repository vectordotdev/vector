package metadata

generated: components: sinks: file: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Controls whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source that supports end-to-end
				acknowledgements that is connected to that sink waits for events
				to be acknowledged by **all connected sinks** before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
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
	idle_timeout_secs: {
		description: """
			The amount of time that a file can be idle and stay open.

			After not receiving any events in this amount of time, the file is flushed and closed.
			"""
		required: false
		type: uint: {
			default: 30
			examples: [
				600,
			]
			unit: "seconds"
		}
	}
	internal_metrics: {
		description: "Configuration of internal metrics for file-based components."
		required:    false
		type: object: options: include_file_tag: {
			description: """
				Whether or not to include the "file" tag on the component's corresponding internal metrics.

				This is useful for distinguishing between different files while monitoring. However, the tag's
				cardinality is unbounded.
				"""
			required: false
			type: bool: default: false
		}
	}
	path: {
		description: """
			File path to write events to.

			Compression format extension must be explicit.
			"""
		required: true
		type: string: {
			examples: ["/tmp/vector-%Y-%m-%d.log", "/tmp/application-{{ application_id }}-%Y-%m-%d.log", "/tmp/vector-%Y-%m-%d.log.zst"]
			syntax: "template"
		}
		warnings: ["The rendered path can resolve to any location on the filesystem. Vector will write to it if the process has permission."]
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

generated: components: sinks: file: configuration: encoding: encodingBase & {
	type: object: options: codec: required: true
}
generated: components: sinks: file: configuration: framing: framingEncoderBase & {
	type: object: options: method: required: true
}
