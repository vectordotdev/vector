package metadata

remap: functions: parse_duration: {
	arguments: [
		{
			name:        "value"
			description: "The string of the duration."
			required:    true
			type: ["string"]
		},
		{
			name:        "output"
			description: "The string of the duration unit the number should be output as."
			required:    true
			type: ["string"]
		},
	]
	return: ["float"]
	category: "parse"
	description: #"""
		Parses a string representing a duration and returns a number of this duration in another specified unit.

		Available units:
		- **ns** - nanoseconds (1 billion nanoseconds in a second)
		- **us** - microseconds (1 million microseconds in a second)
		- **µs** - microseconds (1 million microseconds in a second)
		- **ms** - milliseconds (1 thousand microseconds in a second)
		- **cs** - centisecond (100 centiseconds in a second)
		- **ds** - decisecond (10 deciseconds in a second)
		- **s** - second
		- **m** - minute (60 seconds in a minute)
		- **h** - hour (60 minutes in an hour)
		- **d** - day (24 hours in a day)
		"""#
	examples: [
		{
			title: "Success"
			input: {
				duration: "1005ms"
			}
			source: #"""
				.seconds = parse_duration(.duration, "s")
				"""#
			output: {
				seconds: 1.005
			}
		},
		{
			title: "Error"
			input: {
				duration: "1005years"
			}
			source: #"""
				.seconds = parse_duration(.duration, "s")
				"""#
			output: {
				error: remap.errors.ParseError
			}
		},
	]
}
