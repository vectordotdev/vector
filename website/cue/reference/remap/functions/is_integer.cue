package metadata

remap: functions: is_integer: {
	category: "Type"
	description: """
		Check if the type of a `value` is an integer or not.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check"#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is an integer."#,
			#"Returns `false` if `value` is anything else."#,
		]
	}

	examples: [
		{
			title: "Valid integer"
			source: """
				is_integer(1)
				"""
			return: true
		},
		{
			title: "Non-matching type"
			source: """
				is_integer("a string")
				"""
			return: false
		},
	]
}
