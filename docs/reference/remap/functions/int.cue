package metadata

remap: functions: int: {
	category: "Type"
	description: """
		Returns the `value` if it's an integer and errors otherwise. This enables the type checker to guarantee that the
		returned value is an integer and can be used in any function that expects one.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value that you need to ensure is an integer."
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
			#"Returns the `value` if it's an integer."#,
			#"Raises an error if not an integer."#,
		]
	}
	examples: [
		{
			title: "Declare an integer type"
			input: log: value: 42
			source: #"""
				int!(.value)
				"""#
			return: input.log.value
		},
	]
}
