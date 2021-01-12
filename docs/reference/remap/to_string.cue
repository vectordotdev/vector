package metadata

remap: functions: to_string: {
	arguments: [
		{
			name:        "value"
			description: "The value to return a string representation of."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "Coerce"
	description: #"""
		Coerces the provided `value` into a `string`.
		"""#
	examples: [
		{
			title: "Convert number to string"
			input: log: number: 52
			source: #"""
				.number = to_string(.number)
				"""#
			output: log: number: "52"
		},
	]
}
