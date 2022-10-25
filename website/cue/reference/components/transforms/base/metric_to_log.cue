package metadata

base: components: transforms: metric_to_log: configuration: {
	host_tag: {
		description: """
			Name of the tag in the metric to use for the source host.

			If present, the value of the tag is set on the generated log event in the "host" field,
			where the field key will use the [global `host_key` option][global_log_schema_host_key].

			[global_log_schema_host_key]: https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key
			"""
		required: false
		type: string: {
			examples: ["host", "hostname"]
			syntax: "literal"
		}
	}
	timezone: {
		description: """
			The name of the timezone to apply to timestamp conversions that do not contain an explicit
			time zone.

			This overrides the [global `timezone`][global_timezone] option. The time zone name may be
			any name in the [TZ database][tz_database], or `local` to indicate system local time.

			[global_timezone]: https://vector.dev/docs/reference/configuration//global-options#timezone
			[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
	}
}
