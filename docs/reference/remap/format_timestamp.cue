package metadata

remap: functions: format_timestamp: {
	arguments: [
		{
			name:        "value"
			description: "The timestamp to format as text."
			required:    true
			type: ["timestamp"]
		},
		{
			name:        "format"
			description: "The format string"
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "Timestamp"
	description: #"""
		Formats a `timestamp` as a given string.
		The format string used is specified by the [Chrono library](\(urls.chrono_time_formats)).
		"""#
	examples: [
		{
			title: "Format a timestamp (ISO8601/RFC 3339)"
			input: log: {}
			source: #"""
				.timestamp = format_timestamp(now(), format: "%+")
				"""#
			output: log: timestamp: "2020-10-21T16:00:00Z"
		},
		{
			title: "Format a timestamp (custom)"
			input: log: {}
			source: #"""
				.timestamp = format_timestamp(now(), format: "%v %R")
				"""#
			output: log: timestamp: "10-Oct-2020 16:00"
		},
	]
}
