package metadata

remap: functions: object: {
	category: "Type"
	description: """
		Returns the `value` if it's an object and errors otherwise. This enables the type checker to guarantee that the
		returned value is an object and can be used in any function that expects one.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value that you need to ensure is an object."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't an object.",
	]
	return: {
		types: ["object"]
		rules: [
			#"Returns the `value` if it's an object."#,
			#"Raises an error if not an object."#,
		]
	}
	examples: [
		{
			title: "Declare an object type"
			input: log: value: {
				field1: "value1"
				field2: "value2"
			}
			source: #"""
				object!(.value)
				"""#
			return: input.log.value
		},
	]
}
