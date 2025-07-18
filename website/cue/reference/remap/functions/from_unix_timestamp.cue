package metadata

remap: functions: from_unix_timestamp: {
	category:    "Convert"
	description: """
		Converts the `value` integer from a [Unix timestamp](\(urls.unix_timestamp)) to a VRL `timestamp`.

		Converts from the number of seconds since the Unix epoch by default. To convert from milliseconds or nanoseconds, set the `unit` argument to `milliseconds` or `nanoseconds`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The Unix timestamp to convert."
			required:    true
			type: ["integer"]
		},
		{
			name:        "unit"
			description: "The time unit."
			type: ["string"]
			required: false
			enum: {
				seconds:      "Express Unix time in seconds"
				milliseconds: "Express Unix time in milliseconds"
				nanoseconds:  "Express Unix time in nanoseconds"
				microseconds: "Express Unix time in microseconds"
			}
			default: "seconds"
		},
	]
	internal_failure_reasons: []
	return: types: ["timestamp"]

	examples: [
		{
			title: "Convert from a Unix timestamp (seconds)"
			source: #"""
				from_unix_timestamp!(5)
				"""#
			return: "1970-01-01T00:00:05Z"
		},
		{
			title: "Convert from a Unix timestamp (milliseconds)"
			source: #"""
				from_unix_timestamp!(5000, unit: "milliseconds")
				"""#
			return: "1970-01-01T00:00:05Z"
		},
		{
			title: "Convert from a Unix timestamp (nanoseconds)"
			source: #"""
				from_unix_timestamp!(5000, unit: "nanoseconds")
				"""#
			return: "1970-01-01T00:00:00.000005Z"
		},
	]
}
