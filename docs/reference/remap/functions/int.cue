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
	return: types: ["integer"]
	examples: [
		{
			title: "Declare an integer type"
			input: log: value: 42
			source: #"""
				int(.value)
				"""#
			return: input.log.value
		},
	]
}
