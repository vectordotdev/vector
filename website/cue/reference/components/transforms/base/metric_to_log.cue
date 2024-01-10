package metadata

base: components: transforms: metric_to_log: configuration: {
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
	metric_tag_values: {
		description: """
			Controls how metric tag values are encoded.

			When set to `single`, only the last non-bare value of tags are displayed with the
			metric.  When set to `full`, all metric tags are exposed as separate assignments as
			described by [the `native_json` codec][vector_native_json].

			[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
			"""
		required: false
		type: string: {
			default: "single"
			enum: {
				full: "All tags are exposed as arrays of either string or null values."
				single: """
					Tag values are exposed as single strings, the same as they were before this config
					option. Tags with multiple values show the last assigned value, and null values
					are ignored.
					"""
			}
		}
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
}
