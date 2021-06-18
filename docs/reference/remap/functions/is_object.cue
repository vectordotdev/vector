package metadata

remap: functions: is_object: {
	category: "Type"
	description: """
		Check if the type of a `value` is an object or not.
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
			#"Returns `true` if `value` is an object."#,
			#"Returns `false` if `value` is anything else."#,
		]
	}

	examples: [
		{
			title: "Valid object"
			source: """
				is_object({"foo": "bar"})
				"""
			return: true
		},
		{
			title: "Non-matching type"
			source: """
				is_object("a string")
				"""
			return: false
		},
	]
}
