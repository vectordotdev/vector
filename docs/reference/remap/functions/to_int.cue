remap: functions: to_int: {
	arguments: [
		{
			name:        "value"
			description: """
				The value to convert to an integer.

				* If a string, it must be the string representation of an integer or else an error
					is raised.
				* If a Boolean, returns `0` for `false` and `1` for `true`.
				* If a timestamp, returns the [Unix timestamp](\(urls.unix_timestamp)) in seconds.
				"""
			required:    true
			type: ["integer", "float", "boolean", "string", "timestamp"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a supported integer representation",
	]
	return: ["integer"]
	category: "Coerce"
	description: #"""
		Coerces the provided `value` into a `string`.
		"""#
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
