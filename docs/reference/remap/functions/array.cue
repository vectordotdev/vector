package metadata

remap: functions: array: {
	category: "Coerce"
	description: """
		Errors if `value` is not an array, if `value` is an array it is returned. This allows the type checker
		to guarantee that the returned value is an array and can be used in any function that expects this type.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to ensure is an array."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` is not an array.",
	]
	return: {
		types: ["array"]
		rules: [
			#"If `value` is an array then it is returned."#,
			#"Otherwise an error is raised."#,
		]
	}
	examples: [
		{
			title: "Array"
			input: log: {
				field1: [1, 2, 3]
				field2: [4, 5, 6]
			}
			source: #"""
				append(array!(.field1), array!(.field2))
				"""#
			return: [1, 2, 3, 4, 5, 6]
		},
	]
}
