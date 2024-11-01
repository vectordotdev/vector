package metadata

remap: functions: object: {
	category: "Type"
	description: """
		Returns `value` if it is an object, otherwise returns an error. This enables the type checker to guarantee that the
		returned value is an object and can be used in any function that expects an object.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to check if it is an object."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` is not an object.",
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
