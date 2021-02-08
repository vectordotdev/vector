package metadata

remap: functions: to_string: {
	category: "Coerce"
	description: """
		Coerces the `value` into a string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to return a string representation of."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["string"]
		rules: [
			#"If `value` is an integer then its string representation is returned."#,
			#"If `value` is an float then its string representation is returned."#,
			#"If `value` is an boolean then `"true"` or `"false"` is returned."#,
			#"If `value` is an timestamp then its RFC3339 representation is returned."#,
			#"If `value` is a map then it is encoded into JSON."#,
			#"If `value` is a list then it is encoded into JSON."#,
		]
	}

	examples: [
		{
			title: "Coerce to a string (boolean)"
			source: #"""
				to_string(true)
				"""#
			return: true
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
