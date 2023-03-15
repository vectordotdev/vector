package metadata

remap: functions: to_timestamp: {
	category: "Coerce"
	description: """
		Coerces the `value` into a timestamp.
		"""
	notices: ["There is the possibility of precision loss due to float arithmetic when coercing floats."]

	arguments: [
		{
			name:        "value"
			description: "The value that is to be converted to a timestamp. If a string, must be a valid representation of a `timestamp` otherwise an `ArgumentError` will be raised."
			required:    true
			type: ["string", "float", "integer", "timestamp"]
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
			}
			default: "seconds"
		},
	]
	internal_failure_reasons: [
		"When `value` is a `string`, it is not a valid timestamp format",
		"When `value` is an `int`, it is not within the Unix timestamp range",
		"When `value` is a `float`, it is not within the Unix timestamp range",
	]
	return: {
		types: ["timestamp"]
		rules: [
			"If `value` is a `string`, the timestamp is parsed in these formats.",
			"If `value` is an `integer`, it is assumed to be a Unix representation of the timestamp (the number of seconds after January 1st, 1970).",
			"If `value` is a `float`, it s assumed to be a Unix representation of the timestamp (the number of seconds after January 1st, 1970) with fractional seconds.",
		]
	}

	examples: [
		{
			title: "Coerce a string to a timestamp"
			source: """
				to_timestamp!("2020-10-21T16:00:00Z")
				"""
			return: "2020-10-21T16:00:00Z"
		},
		{
			title: "Coerce a unix timestamp (integer) to a timestamp"
			source: """
				to_timestamp!(1675968923)
				"""
			return: "2023-02-09T18:55:23Z"
		},
		{
			title: "Coerce a unix timestamp (float) to a timestamp"
			source: """
				to_timestamp!(1675968923.567)
				"""
			return: "2023-02-09T18:55:23.566999912Z"
		},
		{
			title: "Coerce a unix timestamp, in milliseconds, to a timestamp"
			source: """
				to_timestamp!(1676478566639, unit: "milliseconds")
				"""
			return: "2023-02-15T16:29:26.639Z"
		},
		{
			title: "Coerce a unix timestamp, in nanoseconds, to a timestamp"
			source: """
				to_timestamp!(1675968923012312311, unit: "nanoseconds")
				"""
			return: "2023-02-09T18:55:23.012312311Z"
		},
	]
}
