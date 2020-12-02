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
	category: "text"
	description: #"""
		Formats a `timestamp` as a given string.
		The format string used is specified by the [Chrono library](https://docs.rs/chrono/0.4.19/chrono/format/strftime/index.html).
		"""#
	examples: [
		{
			title: "Success"
			input: {
				date: "2020-10-21T16:00:00Z"
			}
			source: #"""
				.timestamp = to_timestamp(.date)
				.formatted = format_timestamp(.timestamp, format = "%v %R")
				"""#
			output: {
				formatted: "10-Oct-2020 16:00"
			}
		},
		{
			title: "Error"
			input: {
				date: "2020-10-21T16:00:00Z"
			}
			source: #"""
				.timestamp = to_timestamp(.date)
				.formatted = format_timestamp(.timestamp, format = "NOTAFORMAT")
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
