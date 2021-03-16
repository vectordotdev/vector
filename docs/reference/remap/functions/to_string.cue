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
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["string"]
		rules: [
			#"If `value` is an integer or float, returns the string representation."#,
			#"If `value` is a Boolean, returns `"true"` or `"false"`."#,
			#"If `value` is a timestamp, returns an [RFC 3339](\(urls.rfc3339)) representation."#,
			#"If `value` is an object or array, returns a JSON-encoded string."#,
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
