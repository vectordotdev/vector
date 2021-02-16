package metadata

remap: functions: array: {
	category: "Type"
	description: """
		Errors if `value` is not an array, if `value` is an array it is returned.

		This allows the type checker to guarantee that the returned value is an array and can be used in any function
		that expects this type.
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
	return: types: ["array"]
	examples: [
		{
			title: "Declare an array type"
			input: log: value: [1, 2, 3]
			source: #"""
				array(.value)
				"""#
			return: input.log.value
		},
	]
}
