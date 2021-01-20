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
			name:        "unit"
			description: "The string of the duration unit the number should be output as."
			required:    true
			type: ["string"]
			enum: {
				ns: "Nanoseconds (1 billion nanoseconds in a second)"
				us: "Microseconds (1 million microseconds in a second)"
				µs: "Microseconds (1 million microseconds in a second)"
				ms: "Milliseconds (1 thousand microseconds in a second)"
				cs: "Centiseconds (100 centiseconds in a second)"
				ds: "Deciseconds (10 deciseconds in a second)"
				s:  "Seconds"
				m:  "Minutes (60 seconds in a minute)"
				h:  "Hours (60 minutes in an hour)"
				d:  "Days (24 hours in a day)"
			}
		},
	]
	internal_failure_reasons: [
		"`value` is not a properly formatted duration",
	]
	return: ["float"]
	category: "Parse"
	description: #"""
		Parses the provided `value` representing a duration in the specified `unit`.
		"""#
	examples: [
		{
			title: "Parse duration (milliseconds)"
			source: #"""
				parse_duration("1005ms", unit: "s")
				"""#
			return: 1.005
		},
	]
}
