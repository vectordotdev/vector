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
			description: "The timestamp format as represented by [Chrono library](\(urls.chrono_time_formats))."
			required:    true
			type: ["string"]
		},

	]
	internal_failure_reasons: [
		"`value` fails to parse via the provided `format`",
	]
	return: ["timestamp"]
	category: "Parse"
	description: #"""
		Parses the provided `value` via the provided `format`.
		"""#
	examples: [
		{
			title: "Parse timestamp"
			source: #"""
				parse_timestamp("10-Oct-2020 16:00", format: "%v %R")
				"""#
			return: "2020-10-10T16:00:00Z"
		},
	]
}
