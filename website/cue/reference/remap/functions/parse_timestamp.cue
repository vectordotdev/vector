package metadata

remap: functions: parse_timestamp: {
	category:    "Parse"
	description: """
		Parses the `value` in [strptime](\(urls.strptime_specifiers)) `format`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The text of the timestamp."
			required:    true
			type: ["string"]
		},
		{
			name:        "format"
			description: "The [strptime](\(urls.strptime_specifiers)) format."
			required:    true
			type: ["string"]
		},
		{
			name:        "timezone"
			description: """
				The [TZ database](\(urls.tz_time_zones)) format. By default, this function parses the timestamp by global [`timezone` option](\(urls.vector_configuration)/global-options#timezone).
				This argument overwrites the setting and is useful for parsing timestamps without a specified timezone, such as `16/10/2019 12:00:00`.
				"""
			required:    false
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` fails to parse using the provided `format`.",
		"`value` fails to parse using the provided `timezone`.",
	]
	return: types: ["timestamp"]

	examples: [
		{
			title: "Parse timestamp"
			source: #"""
				parse_timestamp!("10-Oct-2020 16:00+00:00", format: "%v %R %:z")
				"""#
			return: "2020-10-10T16:00:00Z"
		},
		{
			title: "Parse timestamp with timezone"
			source: #"""
				parse_timestamp!("16/10/2019 12:00:00", format: "%d/%m/%Y %H:%M:%S", timezone: "Asia/Taipei")
				"""#
			return: "2019-10-16T04:00:00Z"
		},
	]
}
