package metadata

remap: functions: format_timestamp: {
	category: "Timestamp"
	description: #"""
		Formats the `value` into a string representation of the timestamp.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The timestamp to format as text."
			required:    true
			type: ["timestamp"]
		},
		{
			name:        "format"
			description: "The format string as decribed by the [Chrono library](\(urls.chrono_time_formats))."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Format a timestamp (ISO8601/RFC 3339)"
			source: #"""
				format_timestamp(now(), format: "%+")
				"""#
			return: "2020-10-21T16:00:00Z"
		},
		{
			title: "Format a timestamp (custom)"
			source: #"""
				format_timestamp(now(), format: "%v %R")
				"""#
			return: "10-Oct-2020 16:00"
		},
	]
}
