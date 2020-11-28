package metadata

remap: functions: parse_timestamp: {
	arguments: [
		{
			name:        "value"
			description: "The text of the timestamp."
			required:    true
			type: ["string"]
		},
		{
			name:        "format"
			description: "The format string the timestamp text is expected to be in."
			required:    true
			type: ["string"]
		},
		{
			name:        "default"
			description: "If `value` cannot be converted to a timestamp, if `default` is a string attempt to parse this. If it is a timestamp, return this timestamp."
			required:    false
			type: ["string", "timestamp"]
		},

	]
	return: ["timestamp"]
	category: "coerce"
	description: #"""
		Parses a string representing a timestamp using a provided format string. If the string is unable to be parsed, and a `default` is specified,
		use this. `default` can be either a `string` or a `timestamp`. If a `string`, it is parsed and the result returned. If a `timestamp`, this
		is returned.

		The format string used is specified by the [Chrono library](https://docs.rs/chrono/0.4.19/chrono/format/strftime/index.html).
		"""#
	examples: [
		{
			title: "Success"
			input: {
				".timestamp_bad":  "I am not a timestamp"
				".timestamp_good": "10-Oct-2020 16:00"
			}
			source: #"""
				.timestamp = parse_timestamp(.timestamp_bad, format="%v %R", default=.timestamp_bad)
				"""#
			output: {
				".timestamp":      "10-Oct-2020 16:00:00"
				".timestamp_bad":  "I am not a timestamp"
				".timestamp_good": "10-Oct-2020 16:00"
			}
		},
		{
			title: "Error"
			input: {
				".timestamp_bad": "I am not a timestamp"
			}
			source: #"""
				.timestamp = parse_timestamp(.timestamp_bad, format="%v %R", default=.timestamp_bad)
				"""#
			output: {
				error: remap.errors.ParseError
			}
		},
	]
}
