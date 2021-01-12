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
	return: ["integer"]
	category: "Coerce"
	description: #"""
		Returns an `integer` whose text representation is `string`.
		"""#
	examples: [
		{
			title: "Convert string to int"
			input: log: integer: "2"
			source: ".integer = to_int(.integer)"
			output: log: integer: 2
		},
		{
			title: "Convert timestamp to int"
			input: log: timestamp: "2020-12-30 22:20:53.824727 UTC"
			source: ".timestamp = to_int(.timestamp)"
			output: log: timestamp: 1609366853
		},
	]
}
