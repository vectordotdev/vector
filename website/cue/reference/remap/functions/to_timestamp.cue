package metadata

remap: functions: to_timestamp: {
	category: "Coerce"
	description: """
		Coerces the `value` into a timestamp.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value that is to be converted to a timestamp. If a string, must be a valid representation of a `timestamp`, and no `default` exists, an `ArgumentError` will be raised."
			required:    true
			type: ["string", "float", "integer", "timestamp"]
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
			title: "Coerce to a timestamp"
			source: """
				to_timestamp!("2020-10-21T16:00:00Z")
				"""
			return: "2020-10-21T16:00:00Z"
		},
	]
}
