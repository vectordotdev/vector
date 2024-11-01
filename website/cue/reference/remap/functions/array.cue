package metadata

remap: functions: array: {
	category: "Type"
	description: """
		Returns `value` if it is an array, otherwise returns an error. This enables the type checker to guarantee that the
		returned value is an array and can be used in any function that expects an array.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to check if it is an array."
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
			#"Returns the `value` if it's an array."#,
			#"Raises an error if not an array."#,
		]
	}
	examples: [
		{
			title: "Declare an array type"
			input: log: value: [1, 2, 3]
			source: #"""
				array!(.value)
				"""#
			return: input.log.value
		},
	]
}
