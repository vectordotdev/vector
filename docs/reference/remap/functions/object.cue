package metadata

remap: functions: object: {
	category: "Type"
	description: """
		Errors if `value` is not an object, if `value` is an object it is returned.

		This allows the type checker to guarantee that the returned value is an object and can be used in any function
		that expects this type.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to ensure is an object."
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
			#"If `value` is an object then it is returned."#,
			#"Otherwise an error is raised."#,
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
				object(.value)
				"""#
			return: input.log.value
		},
	]
}
