package metadata

remap: functions: is_boolean: {
	category: "Type"
	description: """
		Check if the `value`'s type is a boolean.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check if it is a Boolean."#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is a boolean."#,
			#"Returns `false` if `value` is anything else."#,
		]
	}

	examples: [
		{
			title: "Valid boolean"
			source: """
				is_boolean(false)
				"""
			return: true
		},
		{
			title: "Non-matching type"
			source: """
				is_boolean("a string")
				"""
			return: false
		},
	]
}
