package metadata

remap: functions: format_timestamp: {
	category: "Timestamp"
	description: #"""
		Formats `value` into a string representation of the timestamp.
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
			description: "The format string as described by the [Chrono library](\(urls.chrono_time_formats))."
			required:    true
			type: ["string"]
		},
		{
			name:        "timezone"
			description: "The timezone to use when formatting the timestamp. The parameter uses the TZ identifier or `local`."
			required:    false
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Format a timestamp (ISO8601/RFC 3339)"
			source: #"""
				format_timestamp!(t'2020-10-21T16:00:00Z', format: "%+")
				"""#
			return: "2020-10-21T16:00:00+00:00"
		},
		{
			title: "Format a timestamp (custom)"
			source: #"""
				format_timestamp!(t'2020-10-21T16:00:00Z', format: "%v %R")
				"""#
			return: "21-Oct-2020 16:00"
		},
	]
}
