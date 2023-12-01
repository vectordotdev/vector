package metadata

remap: functions: is_array: {
	category: "Type"
	description: """
		Check if the `value`'s type is an array.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check if it is an array."#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is an array."#,
			#"Returns `false` if `value` is anything else."#,
		]
	}

	examples: [
		{
			title: "Valid array"
			source: """
				is_array([1, 2, 3])
				"""
			return: true
		},
		{
			title: "Non-matching type"
			source: """
				is_array("a string")
				"""
			return: false
		},
	]
}
