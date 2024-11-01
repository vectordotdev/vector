package metadata

remap: functions: is_float: {
	category: "Type"
	description: """
		Check if the `value`'s type is a float.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check if it is a float."#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is a float."#,
			#"Returns `false` if `value` is anything else."#,
		]
	}

	examples: [
		{
			title: "Valid float"
			source: """
				is_float(0.577)
				"""
			return: true
		},
		{
			title: "Non-matching type"
			source: """
				is_float("a string")
				"""
			return: false
		},
	]
}
