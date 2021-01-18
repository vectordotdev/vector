package metadata

remap: functions: to_unix_timestamp: {
	arguments: [
		{
			name:        "value"
			description: "The timestamp to convert to Unix."
			required:    true
			type: ["timestamp"]
		},
		{
			name:        "unit"
			description: "The time unit"
			type: ["string"]
			required: false
			enum: {
				seconds:      "Express Unix time in seconds"
				milliseconds: "Express Unix time in milliseconds"
				nanoseconds:  "Express Unix time in nanoseconds"
			}
			default: "seconds"
		},
	]
	internal_failure_reasons: []
	return: ["integer"]
	category:    "Coerce"
	description: """
		Coerces the provided `value` into a [Unix timestamp](\(urls.unix_timestamp)).

		By default, the number of seconds since the Unix epoch is returned, but milliseconds or nanoseconds can be
		returned via the `unit` argument.
		"""
	examples: [
		{
			title: "Convert to a Unix timestamp"
			input: log: date: "2021-01-01T00:00:00+00:00"
			source: #"""
				.no_default = to_unix_timestamp(to_timestamp(.date))
				.seconds = to_unix_timestamp(to_timestamp(.date))
				.milliseconds = to_unix_timestamp(to_timestamp(.date), unit: "milliseconds")
				.nanoseconds = to_unix_timestamp(to_timestamp(.date), unit: "nanoseconds")
				"""#
			output: input & {log: {
				default:      1609459200
				seconds:      1609459200
				milliseconds: 1609459200000
				nanoseconds:  1609459200000000000
			}}
		},
	]
}
