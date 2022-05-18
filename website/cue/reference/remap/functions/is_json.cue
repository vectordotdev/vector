package metadata

remap: functions: is_json: {
	category: "Type"
	description: """
		Check if the string is a valid JSON document.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check"#
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is a valid JSON document."#,
			#"Returns `false` if `value` is not JSON-formatted."#,
		]
	}

	examples: [
		{
			title: "Valid JSON object"
			source: """
				is_json("{}")
				"""
			return: true
		},
		{
			title: "Non-valid value"
			source: """
				is_json("{")
				"""
			return: false
		},
	]
}
