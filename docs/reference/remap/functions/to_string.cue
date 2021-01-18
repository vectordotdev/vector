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
			title: "Coerce to a string"
			input: log: {
				boolean: true
				int:     52
				float:   12.2
			}
			source: #"""
				.boolean = to_string(.boolean)
				.int = to_string(.int)
				.float = to_string(.float)
				"""#
			output: log: {
				boolean: "true"
				int:     "52"
				float:   "12.2"
			}
		},
	]
}
