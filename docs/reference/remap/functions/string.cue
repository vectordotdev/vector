package metadata

remap: functions: string: {
	category: "Type"
	description: """
		Errors if `value` is not a string, if `value` is a string it is returned.

		This allows the type checker to guarantee that the returned value is a string and can be used in any function
		that expects this type.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to ensure is a string."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a string.",
	]
	return: {
		types: ["string"]
		rules: [
			#"If `value` is a string then it is returned."#,
			#"Otherwise an error is raised."#,
		]
	}
	examples: [
		{
			title: "Delcare a string type"
			input: log: message: '{"field": "value"}'
			source: #"""
				string(.message)
				"""#
			return: input.log.message
		},
	]
}
