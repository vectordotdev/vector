package metadata

remap: functions: from_unix_timestamp: {
	category: "String"
	description: """
		Parses the string `value` representing a floating point number to a float.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["float"]

	examples: [
		{
			title:  "Parse negative integer"
			source: #"parse_float!("-42")"#
			return: "-42.0"
		},
		{
			title:  "Parse negative integer"
			source: #"parse_float!("42.38")"#
			return: "42.38"
		},
		{
			title:  "Scientific notation"
			source: #"parse_float!("2.5e3")"#
			return: "2500.0"
		},
	]

}
