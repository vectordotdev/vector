package metadata

remap: functions: int: {
	category: "Type"
	description: """
		Errors if `value` is not an integer, if `value` is an integer it is returned.

		This allows the type checker to guarantee that the returned value is an integer and can be used in any function
		that expects this type.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to ensure is an integer."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` is not an integer.",
	]
	return: {
		types: ["integer"]
		rules: [
			#"If `value` is an integer then it is returned."#,
			#"Otherwise an error is raised."#,
		]
	}
	examples: [
		{
			title: "Integer"
			input: log: {
				value: 42
			}
			source: #"""
				84 / int!(.value)
				"""#
			return: 2
		},
	]
}
