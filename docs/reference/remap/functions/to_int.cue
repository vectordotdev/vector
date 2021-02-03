remap: functions: to_int: {
	category: "Coerce"
	description: """
		Coerces the `value` into an integer.
		"""

	arguments: [
		{
			name: "value"
			description: """
				The value to convert to an integer.
				"""
			required: true
			type: ["integer", "float", "boolean", "string", "timestamp"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a supported integer representation",
	]
	return: {
		types: ["integer"]
		rules: [
			"If `value` is a string, it must be the string representation of an integer or else an error is raised.",
			"If `value` is a boolean, `0` will be returned for `false` and `1` will be returned for `true`.",
			"If `value` is a timestamp, a [Unix timestamp](\(urls.unix_timestamp)) (in seconds) is returned.",
		]
	}

	examples: [
		{
			title: "Coerce to an int (string)"
			source: """
				to_int("2")
				"""
			return: 2
		},
		{
			title: "Coerce to an int (timestamp)"
			source: """
				to_int(to_timestamp("2020-12-30 22:20:53.824727 UTC"))
				"""
			return: 1609366853
		},
	]
}
