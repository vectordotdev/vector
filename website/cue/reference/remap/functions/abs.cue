package metadata

remap: functions: abs: {
	category: "Number"
	description: """
		Computes the absolute value of `value`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The number to calculate the absolute value."
			required:    true
			type: ["integer", "float"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["integer", "float"]
		rules: [
			"Returns the absolute value.",
		]
	}

	examples: [
		{
			title: "Computes the absolute value of the integer"
			source: #"""
				abs(-42)
				"""#
			return: 42
		},
		{
			title: "Computes the absolute value of the float"
			source: #"""
				abs(-42.2)
				"""#
			return: 42.2
		},
	]
}
