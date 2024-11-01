package metadata

remap: functions: to_string: {
	category: "Coerce"
	description: """
		Coerces the `value` into a string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to convert to a string."
			required:    true
			type: ["integer", "float", "boolean", "string", "timestamp", "null"]
		},
	]
	internal_failure_reasons: [
		"`value` is not an integer, float, boolean, string, timestamp, or null.",
	]
	return: {
		types: ["string"]
		rules: [
			#"If `value` is an integer or float, returns the string representation."#,
			#"If `value` is a boolean, returns `"true"` or `"false"`."#,
			#"If `value` is a timestamp, returns an [RFC 3339](\(urls.rfc3339)) representation."#,
			#"If `value` is a null, returns `""`."#,
		]
	}

	examples: [
		{
			title: "Coerce to a string (Boolean)"
			source: #"""
				to_string(true)
				"""#
			return: "true"
		},
		{
			title: "Coerce to a string (int)"
			source: #"""
				to_string(52)
				"""#
			return: "52"
		},
		{
			title: "Coerce to a string (float)"
			source: #"""
				to_string(52.2)
				"""#
			return: "52.2"
		},
	]
}
