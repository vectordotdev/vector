package metadata

remap: functions: to_unix_timestamp: {
	category:    "Coerce"
	description: """
		Coerces the `value` into a [Unix timestamp](\(urls.unix_timestamp)).

		By default, the number of seconds since the Unix epoch is returned, but milliseconds or nanoseconds can be
		returned via the `unit` argument.
		"""

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
	return: types: ["integer"]

	examples: [
		{
			title: "Convert to a Unix timestamp (seconds)"
			source: #"""
				to_unix_timestamp(to_timestamp("2021-01-01T00:00:00+00:00"))
				"""#
			return: 1609459200
		},
		{
			title: "Convert to a Unix timestamp (milliseconds)"
			source: #"""
				to_unix_timestamp(to_timestamp("2021-01-01T00:00:00+00:00"), unit: "milliseconds")
				"""#
			return: 1609459200000
		},
		{
			title: "Convert to a Unix timestamp (nanoseconds)"
			source: #"""
				to_unix_timestamp(to_timestamp("2021-01-01T00:00:00+00:00"), unit: "nanoseconds")
				"""#
			return: 1609459200000000000
		},
	]
}
