package metadata

remap: functions: bool: {
	category: "Type"
	description: """
		Returns the `value` if it's a Boolean and errors otherwise. This enables the type checker to guarantee that the
		returned value is a Boolean and can be used in any function that expects one.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value that you need to ensure is a Boolean."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a Boolean.",
	]
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `value` if it's a Boolean."#,
			#"Raises an error if not a Boolean."#,
		]
	}
	examples: [
		{
			title: "Declare a Boolean type"
			input: log: value: false
			source: #"""
				bool!(.value)
				"""#
			return: input.log.value
		},
	]
}
