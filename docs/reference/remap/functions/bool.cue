package metadata

remap: functions: bool: {
	category: "Type"
	description: """
		Errors if `value` is not a boolean, if `value` is a boolean it is returned.

		This allows the type checker to guarantee that the returned value is a boolean and can be used in any function
		that expects this type.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to ensure is a boolean."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a boolean.",
	]
	return: {
		types: ["boolean"]
		rules: [
			#"If `value` is a boolean then it is returned."#,
			#"Otherwise an error is raised."#,
		]
	}
	examples: [
		{
			title: "Declare a boolean type"
			input: log: value: false
			source: #"""
				bool(.value)
				"""#
			return: input.log.value
		},
	]
}
