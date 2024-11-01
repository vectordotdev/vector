package metadata

remap: functions: is_empty: {
	category: "Type"
	description: """
		Check if the object, array, or string has a length of `0`.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check."#
			required:    true
			type: ["object", "array", "string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is empty."#,
			#"Returns `false` if `value` is non-empty."#,
		]
	}

	examples: [
		{
			title: "Empty array"
			source: """
				is_empty([])
				"""
			return: true
		},
		{
			title: "Non-empty string"
			source: """
				is_empty("a string")
				"""
			return: false
		},
		{
			title: "Non-empty object"
			source: """
				is_empty({"foo": "bar"})
				"""
			return: false
		},
	]
}
