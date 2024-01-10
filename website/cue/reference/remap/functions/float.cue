package metadata

remap: functions: float: {
	category: "Type"
	description: """
		Returns `value` if it is a float, otherwise returns an error. This enables the type checker to guarantee that the
		returned value is a float and can be used in any function that expects a float.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to check if it is a float."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a float.",
	]
	return: {
		types: ["float"]
		rules: [
			#"Returns the `value` if it's a float."#,
			#"Raises an error if not a float."#,
		]
	}
	examples: [
		{
			title: "Declare a float type"
			input: log: value: 42.0
			source: #"""
				float!(.value)
				"""#
			return: input.log.value
		},
	]
}
