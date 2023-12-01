package metadata

remap: functions: is_json: {
	category: "Type"
	description: """
		Check if the string is a valid JSON document.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check if it is a valid JSON document."#
			required:    true
			type: ["string"]
		},
		{
			name:        "variant"
			description: "The variant of the JSON type to explicitly check for."
			enum: {
				"object": "JSON object - {}"
				"array":  "JSON array - []"
				"string": "JSON-formatted string values wrapped with quote marks"
				"number": "Integer or float numbers"
				"bool":   "True or false"
				"null":   "Exact null value"
			}
			required: false
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
		{
			title: "Exact variant"
			source: """
				is_json("{}", variant: "object")
				"""
			return: true
		},
		{
			title: "Non-valid exact variant"
			source: """
				is_json("{}", variant: "array")
				"""
			return: false
		},
	]
}
