package metadata

remap: functions: type_def: {
	category: "Debug"
	description: """
		A function that returns a representation of the internal compile-time type definition of a variable. This function
		is intended for debugging / development only and should not be used in production. This function is not considered stable
		and can change at any time.
		"""

	arguments: [
		{
			name:        "value"
			description: "The type of this value will be returned"
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: types: ["any"]

	examples: [
		{
			title: "Display type definition of an object"
			source: #"""
				type_def(42)
				"""#
			return: "{\"integer\": true }"
		},
	]
}
